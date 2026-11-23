//! Thin rails beside the board: one on the left lists the keys, one on the right
//! shows the look (pack) in use. Each rail is its own small window, docked to its
//! side of the board and moving with it (the board never follows a rail).
//! It is a thin strip that opens into a panel, folds back to the strip, or hides
//! away (its window is closed until it is brought back).
//! The choice is saved in ~/.side-chess/panels.txt.
use crate::canvas::{Canvas, Color};
use crate::draw::{piece_tile, View, BY, SQ, W};
use crate::game::live_dir;
use crate::pack::Theme;
use shakmaty::{Color as Side, Role};
use std::fs;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Panel {
    Keys,
    Look,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Hidden,
    Rail,
    Open,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Toggle, // open a rail, or fold an open panel back to its rail
    Hide,
}

pub const RAIL_W: f32 = 26.0;
const KEYS_W: f32 = 236.0;
const LOOK_W: f32 = 208.0;
const GAP: f32 = 6.0; // between a rail window and the board window
pub const TOP: f32 = BY - 4.0; // level with the board's border, from the board window's top
pub const LEN: f32 = SQ * 8.0 + 8.0; // as tall as the board and its border
const HEAD: f32 = 34.0;
const BTN: f32 = 26.0; // the buttons in an open panel's header
const CROSS_BOX: f32 = 22.0; // the hide cross's hit box at the foot of a thin rail

/// The keys rail sits on the left of the board, the look rail on the right.
pub fn on_left(p: Panel) -> bool {
    p == Panel::Keys
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rails {
    pub keys: Mode,
    pub look: Mode,
}

/// A button's box: x, y, size.
type Box3 = (f32, f32, f32);

impl Rails {
    pub fn new() -> Self {
        Rails {
            keys: Mode::Rail,
            look: Mode::Rail,
        }
    }

    pub fn mode(&self, p: Panel) -> Mode {
        match p {
            Panel::Keys => self.keys,
            Panel::Look => self.look,
        }
    }

    fn set(&mut self, p: Panel, m: Mode) {
        match p {
            Panel::Keys => self.keys = m,
            Panel::Look => self.look = m,
        }
    }

    /// How wide a panel's window is in each mode.
    pub fn width_of(p: Panel, m: Mode) -> f32 {
        match (m, p) {
            (Mode::Hidden, _) => 0.0,
            (Mode::Rail, _) => RAIL_W,
            (Mode::Open, Panel::Keys) => KEYS_W,
            (Mode::Open, Panel::Look) => LOOK_W,
        }
    }

    /// Each panel on show: (which, left edge from the board window's left, width).
    /// The left rail ends just before the board window; the right one starts just after.
    pub fn layout(&self) -> Vec<(Panel, f32, f32)> {
        let mut out = vec![];
        for p in [Panel::Keys, Panel::Look] {
            let w = Rails::width_of(p, self.mode(p));
            if w > 0.0 {
                let x = if on_left(p) { -GAP - w } else { W + GAP };
                out.push((p, x, w));
            }
        }
        out
    }

    /// The panel's key: from hidden or a rail it opens, from open it folds to the rail.
    pub fn press_key(&mut self, p: Panel) {
        let next = match self.mode(p) {
            Mode::Hidden | Mode::Rail => Mode::Open,
            Mode::Open => Mode::Rail,
        };
        self.set(p, next);
    }

    /// Where the buttons are in a panel's own window: (toggle, hide). On a thin rail
    /// the toggle is the whole strip.
    fn buttons(&self, p: Panel) -> (Box3, Box3) {
        match self.mode(p) {
            Mode::Open => {
                let w = Rails::width_of(p, Mode::Open);
                // the arrow that folds the panel is the one nearer the board, the cross the one further
                let (near, far) = if on_left(p) {
                    (w - BTN - 6.0, w - 2.0 * BTN - 10.0)
                } else {
                    (w - 2.0 * BTN - 10.0, w - BTN - 6.0)
                };
                ((near, 4.0, BTN), (far, 4.0, BTN))
            }
            _ => (
                (0.0, 0.0, RAIL_W),
                ((RAIL_W - CROSS_BOX) / 2.0, LEN - CROSS_BOX - 8.0, CROSS_BOX),
            ),
        }
    }

    /// What a point in a panel's own window is: a button, or (on a thin rail) the
    /// strip itself, which opens it.
    pub fn hit(&self, p: Panel, x: f32, y: f32) -> Option<Hit> {
        let inside = |(bx, by, s): Box3| (bx..=bx + s).contains(&x) && (by..=by + s).contains(&y);
        let (toggle, hide) = self.buttons(p);
        if inside(hide) {
            return Some(Hit::Hide);
        }
        match self.mode(p) {
            Mode::Rail => Some(Hit::Toggle),
            Mode::Open if inside(toggle) => Some(Hit::Toggle),
            _ => None,
        }
    }

    /// A click at a point in a panel's own window; true if a rail changed.
    pub fn click(&mut self, p: Panel, x: f32, y: f32) -> bool {
        match self.hit(p, x, y) {
            Some(Hit::Toggle) => {
                self.press_key(p);
                true
            }
            Some(Hit::Hide) => {
                self.set(p, Mode::Hidden);
                true
            }
            None => false,
        }
    }

    pub fn to_text(self) -> String {
        let name = |m: Mode| match m {
            Mode::Hidden => "hidden",
            Mode::Rail => "rail",
            Mode::Open => "open",
        };
        format!("keys = {}\nlook = {}\n", name(self.keys), name(self.look))
    }

    pub fn from_text(text: &str) -> Rails {
        let mut r = Rails::new();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let m = match v.trim() {
                "hidden" => Mode::Hidden,
                "rail" => Mode::Rail,
                "open" => Mode::Open,
                _ => continue,
            };
            match k.trim() {
                "keys" => r.keys = m,
                "look" => r.look = m,
                _ => {}
            }
        }
        r
    }

    pub fn load() -> Rails {
        fs::read_to_string(live_dir().join("panels.txt"))
            .map(|t| Rails::from_text(&t))
            .unwrap_or_else(|_| Rails::new())
    }

    pub fn save(&self) {
        let _ = fs::create_dir_all(live_dir());
        let _ = fs::write(live_dir().join("panels.txt"), self.to_text());
    }
}

/// What each key does, as (key, what it does).
fn key_rows() -> Vec<(&'static str, &'static str)> {
    vec![
        ("a-h  1-8", "Pick a square"),
        ("Enter", "Play the move"),
        ("Esc", "Clear it"),
        ("U", "Take back"),
        ("N", "New game"),
        ("S", "Swap sides"),
        ("T", "Next look"),
        ("P", "Screenshot"),
        ("K", "This panel"),
        ("L", "Look panel"),
        (
            if cfg!(target_os = "macos") {
                "Cmd+Q"
            } else {
                "Ctrl+Q"
            },
            "Quit",
        ),
    ]
}

/// A chevron: `dir` is +1 to point right, -1 to point left.
fn chevron(cv: &mut Canvas<'_>, cx: f32, cy: f32, dir: f32, a: f32, t: f32, ink: Color) {
    cv.line(cx - dir * a, cy - 2.0 * a, cx + dir * a, cy, t, ink);
    cv.line(cx + dir * a, cy, cx - dir * a, cy + 2.0 * a, t, ink);
}

fn cross(cv: &mut Canvas<'_>, cx: f32, cy: f32, d: f32, t: f32, ink: Color) {
    cv.line(cx - d, cy - d, cx + d, cy + d, t, ink);
    cv.line(cx - d, cy + d, cx + d, cy - d, t, ink);
}

/// A square button for an open panel's dark header: an outline, filling with the
/// accent when the pointer is over it. `fold` is the arrow's direction (+1 right,
/// -1 left); `None` makes it the hide cross.
fn header_button(cv: &mut Canvas<'_>, th: &Theme, (x, y, s): Box3, fold: Option<f32>, hover: bool) {
    cv.rect(x, y, s, s, th.light_square);
    cv.rect(
        x + 1.5,
        y + 1.5,
        s - 3.0,
        s - 3.0,
        if hover { th.accent } else { th.board_border },
    );
    let ink = if hover {
        th.board_border
    } else {
        th.light_square
    };
    let (cx, cy) = (x + s / 2.0, y + s / 2.0);
    match fold {
        Some(dir) => chevron(cv, cx - dir, cy, dir, s * 0.17, 2.4, ink),
        None => cross(cv, cx, cy, s * 0.2, 2.4, ink),
    }
}

/// One panel, filling its own window. `hover` is the button under the pointer, if any.
pub fn draw_panel(cv: &mut Canvas<'_>, v: &View, p: Panel, hover: Option<Hit>) {
    let (th, txt) = (v.th, v.txt);
    let mode = v.rails.mode(p);
    let w = Rails::width_of(p, mode);
    let open = mode == Mode::Open;
    let left = on_left(p);
    let (toggle, hide) = v.rails.buttons(p);
    cv.clear(th.board_border);
    let (title, label) = match p {
        Panel::Keys => ("Keys", "KEYS"),
        Panel::Look => ("Look", "LOOK"),
    };
    if !open {
        // A thin strip, and all of it is the button that opens it: it fills with the
        // accent under the pointer. An arrow pointing out (away from the board), the
        // name reading up the strip, and a cross at the foot to hide it.
        let on = hover == Some(Hit::Toggle);
        let fill = if on { th.last_to } else { th.light_square };
        cv.rect(2.0, 2.0, w - 4.0, LEN - 4.0, fill);
        let cx = w / 2.0;
        let out = if left { -1.0 } else { 1.0 };
        chevron(cv, cx + out * 1.5, 44.0, out, 5.5, 3.0, th.title);
        let len = txt.width("b11", label);
        txt.draw_up(cv, "b11", label, cx, LEN / 2.0 + len / 2.0, th.title);
        let hot = hover == Some(Hit::Hide);
        let (hx, hy) = (hide.0 + hide.2 / 2.0, hide.1 + hide.2 / 2.0);
        if hot {
            cv.round(hx, hy, hide.2 / 2.0, 0.0, th.error);
        }
        cross(
            cv,
            hx,
            hy,
            4.0,
            2.0,
            if hot { th.light_square } else { th.muted },
        );
        return;
    }
    cv.rect(2.0, 2.0, w - 4.0, LEN - 4.0, th.background);
    // header: the name, an arrow that folds the panel (pointing at the board) and a cross to hide it
    cv.rect(0.0, 0.0, w, HEAD, th.board_border);
    txt.draw(cv, "b18", title, 14.0, 24.0, th.light_square);
    let towards_board = if left { 1.0 } else { -1.0 };
    header_button(
        cv,
        th,
        toggle,
        Some(towards_board),
        hover == Some(Hit::Toggle),
    );
    header_button(cv, th, hide, None, hover == Some(Hit::Hide));
    let top = HEAD;
    match p {
        Panel::Keys => {
            for (i, (key, what)) in key_rows().iter().enumerate() {
                let y = top + 32.0 + i as f32 * 30.0;
                txt.draw(cv, "r14", key, 14.0, y, th.title);
                txt.draw(cv, "r14", what, 100.0, y, th.text);
            }
        }
        Panel::Look => {
            txt.draw(cv, "r14", "In use", 14.0, top + 28.0, th.muted);
            for (i, name) in v.pack_names.iter().enumerate() {
                let y = top + 58.0 + i as f32 * 30.0;
                if i == v.pack {
                    cv.rect(14.0, y - 16.0, 4.0, 22.0, th.accent);
                    txt.draw(cv, "r16", name, 28.0, y, th.title);
                } else {
                    txt.draw(cv, "r14", name, 28.0, y, th.muted);
                }
            }
            let mut y = top + 58.0 + v.pack_names.len() as f32 * 30.0 + 14.0;
            txt.draw(cv, "r14", "Colours", 14.0, y + 14.0, th.muted);
            y += 24.0;
            let swatches = [
                th.background,
                th.light_square,
                th.dark_square,
                th.accent,
                th.title,
                th.text,
            ];
            for (i, c) in swatches.iter().enumerate() {
                let sx = 14.0 + i as f32 * 28.0;
                cv.rect(sx - 1.0, y - 1.0, 26.0, 26.0, th.muted);
                cv.rect(sx, y, 24.0, 24.0, *c);
            }
            y += 48.0;
            txt.draw(cv, "r14", "Pieces", 14.0, y + 14.0, th.muted);
            y += 24.0;
            // a few pieces on the board's two colours
            let tiles = [
                (Role::King, Side::White, true),
                (Role::Queen, Side::Black, false),
                (Role::Knight, Side::White, false),
                (Role::Pawn, Side::Black, true),
            ];
            for (i, (role, side, dark)) in tiles.iter().enumerate() {
                let tx = 14.0 + i as f32 * 44.0;
                let sq = if *dark {
                    th.dark_square
                } else {
                    th.light_square
                };
                cv.rect(tx, y, 42.0, 42.0, sq);
                let (sx, sy) = piece_tile(*role, *side);
                let k = v.tile / 128.0;
                cv.image(
                    v.pieces,
                    (sx * k, sy * k, v.tile, v.tile),
                    (tx + 1.0, y + 1.0, 40.0, 40.0),
                    None,
                );
            }
            txt.draw(cv, "r14", "T: next look", 14.0, LEN - 22.0, th.muted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_rail_sits_on_each_side_of_the_board_window_and_grows_away_from_it() {
        let mut r = Rails::new();
        let l = r.layout();
        assert_eq!(l.len(), 2);
        // the keys rail is on the left: it ends before the board window starts
        assert_eq!(l[0].0, Panel::Keys);
        assert!(l[0].1 + l[0].2 <= 0.0, "{l:?}");
        // the look rail is on the right: it starts after the board window ends
        assert_eq!(l[1].0, Panel::Look);
        assert!(l[1].1 >= W, "{l:?}");
        // an open panel grows away from the board: its right edge (left rail) or its
        // left edge (right rail) stays where the thin rail's was
        let (thin_left_edge_of_right, thin_right_edge_of_left) = (l[1].1, l[0].1 + l[0].2);
        r.keys = Mode::Open;
        r.look = Mode::Open;
        let l = r.layout();
        assert!((l[0].1 + l[0].2 - thin_right_edge_of_left).abs() < 0.01);
        assert!((l[1].1 - thin_left_edge_of_right).abs() < 0.01);
        assert!((l[0].2 - KEYS_W).abs() < 0.01 && (l[1].2 - LOOK_W).abs() < 0.01);
        r.keys = Mode::Hidden;
        r.look = Mode::Hidden;
        assert!(r.layout().is_empty(), "nothing showing: no rail windows");
    }

    #[test]
    fn clicking_a_rail_opens_it_and_the_header_buttons_close_or_hide_it() {
        let mut r = Rails::new();
        // anywhere on a thin strip (not the cross) opens it
        assert!(r.click(Panel::Keys, 13.0, 200.0));
        assert_eq!(r.keys, Mode::Open);
        // the body of an open panel does nothing
        assert!(!r.click(Panel::Keys, 20.0, 200.0));
        // the first header button closes it back to the strip
        let (toggle, hide) = r.buttons(Panel::Keys);
        assert!(r.click(
            Panel::Keys,
            toggle.0 + toggle.2 / 2.0,
            toggle.1 + toggle.2 / 2.0
        ));
        assert_eq!(r.keys, Mode::Rail);
        // the cross at the foot of a strip hides it, without opening it first
        let (_, hide_rail) = r.buttons(Panel::Keys);
        assert_ne!(
            hide, hide_rail,
            "the hide button moves between the two shapes"
        );
        assert!(r.click(Panel::Keys, hide_rail.0 + 3.0, hide_rail.1 + 3.0));
        assert_eq!(r.keys, Mode::Hidden);
        assert_eq!(r.look, Mode::Rail, "the other rail is left alone");
        // the second header button of an open panel hides it
        r.look = Mode::Open;
        let (_, hide) = r.buttons(Panel::Look);
        assert!(r.click(Panel::Look, hide.0 + hide.2 / 2.0, hide.1 + hide.2 / 2.0));
        assert_eq!(r.look, Mode::Hidden);
    }

    #[test]
    fn hover_names_the_button_under_the_pointer() {
        let mut r = Rails::new();
        r.keys = Mode::Open;
        let (toggle, hide) = r.buttons(Panel::Keys);
        assert_eq!(
            r.hit(Panel::Keys, toggle.0 + 2.0, toggle.1 + 2.0),
            Some(Hit::Toggle)
        );
        assert_eq!(
            r.hit(Panel::Keys, hide.0 + 2.0, hide.1 + 2.0),
            Some(Hit::Hide)
        );
        assert_eq!(r.hit(Panel::Keys, 20.0, 300.0), None);
    }

    #[test]
    fn the_keys_cycle_hidden_to_open_to_rail_to_open() {
        let mut r = Rails::new();
        r.keys = Mode::Hidden;
        r.press_key(Panel::Keys);
        assert_eq!(r.keys, Mode::Open);
        r.press_key(Panel::Keys);
        assert_eq!(r.keys, Mode::Rail);
        r.press_key(Panel::Keys);
        assert_eq!(r.keys, Mode::Open);
        assert_eq!(r.look, Mode::Rail, "the other rail is left alone");
    }

    #[test]
    fn the_choice_is_saved_and_read_back_and_bad_lines_are_ignored() {
        let r = Rails {
            keys: Mode::Open,
            look: Mode::Hidden,
        };
        assert_eq!(Rails::from_text(&r.to_text()), r);
        let odd = Rails::from_text("keys = sideways\nlook = open\nnonsense\n");
        assert_eq!((odd.keys, odd.look), (Mode::Rail, Mode::Open));
    }
}
