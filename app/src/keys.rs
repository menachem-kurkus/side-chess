//! Which key does what. The defaults are the ones the README lists. The person can
//! change them on the Keys page of the home screen; the choices are kept in
//! ~/.side-chess/keys.txt, one `action key` pair a line.
//!
//! Not changeable: a-h and 1-8 (they name squares), Enter, Esc, the arrow keys in a
//! replay, Q R B N while a pawn is being promoted, and Ctrl/Cmd+Q to quit.
use crate::game::live_dir;
use std::fs;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Menu,
    New,
    Swap,
    Undo,
    Look,
    Shot,
    Replays,
    Arrows,
    KeysPanel,
    LookPanel,
}

/// Every action, in the order the Keys page lists them.
pub const ACTIONS: [Action; 10] = [
    Action::Menu,
    Action::New,
    Action::Swap,
    Action::Undo,
    Action::Replays,
    Action::Arrows,
    Action::Look,
    Action::Shot,
    Action::KeysPanel,
    Action::LookPanel,
];

impl Action {
    pub fn label(self) -> &'static str {
        match self {
            Action::Menu => "Home screen",
            Action::New => "New game",
            Action::Swap => "Swap sides",
            Action::Undo => "Take back",
            Action::Look => "Next look",
            Action::Shot => "Screenshot",
            Action::Replays => "Saved games",
            Action::Arrows => "Threat arrows",
            Action::KeysPanel => "Keys panel",
            Action::LookPanel => "Look panel",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Action::Menu => "menu",
            Action::New => "new",
            Action::Swap => "swap",
            Action::Undo => "undo",
            Action::Look => "look",
            Action::Shot => "shot",
            Action::Replays => "replays",
            Action::Arrows => "arrows",
            Action::KeysPanel => "keys_panel",
            Action::LookPanel => "look_panel",
        }
    }

    pub fn default_key(self) -> char {
        match self {
            Action::Menu => 'm',
            Action::New => 'n',
            Action::Swap => 's',
            Action::Undo => 'u',
            Action::Look => 't',
            Action::Shot => 'p',
            Action::Replays => 'r',
            Action::Arrows => 'x',
            Action::KeysPanel => 'k',
            Action::LookPanel => 'l',
        }
    }

    fn index(self) -> usize {
        ACTIONS.iter().position(|a| *a == self).unwrap_or(0)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Keymap {
    keys: [char; ACTIONS.len()],
}

/// Keys that cannot be given to an action: the ones that name a square.
pub fn is_square_key(c: char) -> bool {
    matches!(c, 'a'..='h' | '1'..='8')
}

/// A key an action may use: one plain printable character that does not name a square.
pub fn usable(c: char) -> bool {
    c.is_ascii_graphic() && !is_square_key(c.to_ascii_lowercase())
}

impl Keymap {
    pub fn defaults() -> Keymap {
        let mut keys = ['?'; ACTIONS.len()];
        for a in ACTIONS {
            keys[a.index()] = a.default_key();
        }
        Keymap { keys }
    }

    pub fn key_of(&self, a: Action) -> char {
        self.keys[a.index()]
    }

    /// The key as it is shown on screen (capital letters, like the printed key caps).
    pub fn label_of(&self, a: Action) -> String {
        self.key_of(a).to_ascii_uppercase().to_string()
    }

    /// What a typed character does, if anything.
    pub fn action(&self, c: char) -> Option<Action> {
        let c = c.to_ascii_lowercase();
        ACTIONS.iter().copied().find(|a| self.key_of(*a) == c)
    }

    /// Give an action a new key. Refused (with the reason, in words for the window) if the
    /// key names a square or already belongs to another action.
    pub fn set(&mut self, a: Action, c: char) -> Result<(), String> {
        let c = c.to_ascii_lowercase();
        if !usable(c) {
            return Err(if is_square_key(c) {
                format!(
                    "{} names a square, so it can't be a shortcut.",
                    c.to_ascii_uppercase()
                )
            } else {
                "That key can't be used.".to_string()
            });
        }
        if let Some(other) = self.action(c).filter(|o| *o != a) {
            return Err(format!(
                "{} is already {}. Change that one first.",
                c.to_ascii_uppercase(),
                other.label()
            ));
        }
        self.keys[a.index()] = c;
        Ok(())
    }

    pub fn is_default(&self) -> bool {
        *self == Keymap::defaults()
    }

    pub fn to_text(&self) -> String {
        ACTIONS
            .iter()
            .map(|a| format!("{} {}\n", a.id(), self.key_of(*a)))
            .collect()
    }

    /// Read a keys.txt. Anything unreadable, unusable or doubled falls back to the
    /// default for that action, so a hand-edited file can never lock a key out.
    pub fn from_text(text: &str) -> Keymap {
        let mut map = Keymap::defaults();
        for line in text.lines() {
            let mut w = line.split_whitespace();
            let (Some(id), Some(k), None) = (w.next(), w.next(), w.next()) else {
                continue;
            };
            let mut chars = k.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                continue;
            };
            if let Some(a) = ACTIONS.iter().copied().find(|a| a.id() == id) {
                let was = map.key_of(a);
                if map.set(a, c).is_err() {
                    map.keys[a.index()] = was;
                }
            }
        }
        map
    }

    pub fn load() -> Keymap {
        fs::read_to_string(live_dir().join("keys.txt"))
            .map(|t| Keymap::from_text(&t))
            .unwrap_or_else(|_| Keymap::defaults())
    }

    pub fn save(&self) {
        let path = live_dir().join("keys.txt");
        if self.is_default() {
            let _ = fs::remove_file(path); // nothing to remember
            return;
        }
        let _ = fs::create_dir_all(live_dir());
        let _ = fs::write(path, self.to_text());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_keys_the_readme_lists_and_none_names_a_square() {
        let m = Keymap::defaults();
        assert_eq!(m.action('n'), Some(Action::New));
        assert_eq!(m.action('S'), Some(Action::Swap), "capitals count too");
        assert_eq!(m.action('m'), Some(Action::Menu));
        for a in ACTIONS {
            assert!(usable(m.key_of(a)), "{a:?}");
        }
        let mut seen = vec![];
        for a in ACTIONS {
            assert!(!seen.contains(&m.key_of(a)), "two actions share a default");
            seen.push(m.key_of(a));
        }
    }

    #[test]
    fn a_square_key_or_a_taken_key_is_refused_with_a_reason() {
        let mut m = Keymap::defaults();
        assert!(m.set(Action::New, 'e').unwrap_err().contains("square"));
        assert!(m.set(Action::New, '5').unwrap_err().contains("square"));
        let err = m.set(Action::New, 's').unwrap_err();
        assert!(err.contains("Swap sides"), "{err}");
        assert_eq!(m.key_of(Action::New), 'n', "a refused key changes nothing");
        assert!(m.set(Action::New, 'z').is_ok());
        assert_eq!(m.action('z'), Some(Action::New));
        assert_eq!(m.action('n'), None, "the old key is free again");
        assert!(
            m.set(Action::New, 'z').is_ok(),
            "the same key again is fine"
        );
        assert!(m.set(Action::Undo, '9').is_ok(), "digits 0 and 9 are free");
    }

    #[test]
    fn the_file_reads_back_and_a_bad_file_cannot_lock_a_key_out() {
        let mut m = Keymap::defaults();
        m.set(Action::New, 'z').unwrap();
        m.set(Action::Shot, '/').unwrap();
        let back = Keymap::from_text(&m.to_text());
        assert_eq!(back, m);
        // a hand-edited file: a square key, a doubled key, nonsense, an unknown action
        let bad = Keymap::from_text("new e\nswap u\nundo\nshot ab\nnonsense x\nlook q\n");
        assert_eq!(bad.key_of(Action::New), 'n', "e names a square");
        assert_eq!(bad.key_of(Action::Swap), 's', "u is Take back's");
        assert_eq!(bad.key_of(Action::Undo), 'u');
        assert_eq!(bad.key_of(Action::Shot), 'p');
        assert_eq!(bad.key_of(Action::Look), 'q', "a good line is kept");
        assert!(Keymap::from_text("").is_default());
    }
}
