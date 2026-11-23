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

/// How the rails look (Ann picks from a sheet of these).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RailStyle {
    Pale,  // a light strip with a blue outline
    Spine, // a blue strip with a gold edge facing the board
    Tab,   // a light strip with a blue tab at the top
    Dots,  // a dark strip with three dots and no words
}

pub const STYLES: [RailStyle; 4] = [
    RailStyle::Pale,
    RailStyle::Spine,
    RailStyle::Tab,
    RailStyle::Dots,
];

impl RailStyle {
    fn name(self) -> &'static str {
        match self {
            RailStyle::Pale => "pale",
            RailStyle::Spine => "spine",
            RailStyle::Tab => "tab",
            RailStyle::Dots => "dots",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rails {
    pub keys: Mode,
    pub look: Mode,
    pub style: RailStyle,
}

/// A button's box: x, y, size.
type Box3 = (f32, f32, f32);

impl Rails {
    pub fn new() -> Self {
        Rails {
            keys: Mode::Rail,
            look: Mode::Rail,
            style: RailStyle::Spine,
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
                    (w - BTN - 8.0, w - 2.0 * BTN - 16.0)
                } else {
                    (w - 2.0 * BTN - 16.0, w - BTN - 8.0)
                };
                ((near, 4.0, BTN), (far, 4.0, BTN))
            }
            _ => (
                (0.0, 0.0, RAIL_W),
                (
                    (RAIL_W - CROSS_BOX) / 2.0,
                    LEN - CROSS_BOX - 14.0,
                    CROSS_BOX,
                ),
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
        format!(
            "keys = {}\nlook = {}\nstyle = {}\n",
            name(self.keys),
            name(self.look),
            self.style.name()
        )
    }

    pub fn from_text(text: &str) -> Rails {
        let mut r = Rails::new();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            if k.trim() == "style" {
                if let Some(st) = STYLES.into_iter().find(|st| st.name() == v.trim()) {
                    r.style = st;
                }
                continue;
            }
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

/// A square button in an open panel's header: an outline, filling with the accent when
/// the pointer is over it. `fold` is the arrow's direction (+1 right, -1 left); `None`
/// makes it the hide cross.
fn header_button(
    cv: &mut Canvas<'_>,
    th: &Theme,
    (x, y, s): Box3,
    fold: Option<f32>,
    hover: bool,
    (edge, ink, hover_ink): (Color, Color, Color),
) {
    if hover {
        cv.rect(x, y, s, s, th.accent);
    }
    cv.rect_lines(x, y, s, s, 1.5, edge);
    let ink = if hover { hover_ink } else { ink };
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
    let st = v.rails.style;
    let band0 = th.board_border;
    let band1 = band0.mix(Color::rgba8(0, 0, 0, 255), 0.72);
    let on_band = band0.ink(th.background, th.text);
    let (title, label) = match p {
        Panel::Keys => ("Keys", "KEYS"),
        Panel::Look => ("Look", "LOOK"),
    };
    let out = if left { -1.0 } else { 1.0 }; // the way a rail opens, away from the board
    let on = hover == Some(Hit::Toggle);
    let hot = hover == Some(Hit::Hide);
    let (hx, hy) = (hide.0 + hide.2 / 2.0, hide.1 + hide.2 / 2.0);
    if !open {
        // A thin strip, and all of it is the button that opens it. An arrow pointing out,
        // the name reading up the strip (or dots), and a cross at the foot to hide it.
        let cx = w / 2.0;
        let len = txt.width("b11", label);
        let faint = |c: Color| Color { a: 0.7, ..c };
        let (ink, foot) = match st {
            RailStyle::Pale => {
                cv.clear(band0);
                let fill = if on { th.last_to } else { th.light_square };
                cv.rect(2.0, 2.0, w - 4.0, LEN - 4.0, fill);
                (th.title, th.muted)
            }
            RailStyle::Spine => {
                cv.vgrad(0.0, 0.0, w, LEN, band0, band1);
                if on {
                    cv.rect(0.0, 0.0, w, LEN, Color::rgba8(255, 255, 255, 38));
                }
                // a gold edge on the side that faces the board
                let ex = if left { w - 3.0 } else { 0.0 };
                cv.rect(ex, 0.0, 3.0, LEN, th.accent);
                (on_band, faint(on_band))
            }
            RailStyle::Tab => {
                cv.clear(band0);
                cv.rect(1.5, 1.5, w - 3.0, LEN - 3.0, th.background);
                let tab = if on { th.accent } else { band0 };
                cv.rrect(3.0, 6.0, w - 6.0, 92.0, 6.0, tab);
                let tab_ink = if on { band0 } else { on_band };
                chevron(cv, cx + out * 1.5, 52.0, out, 5.5, 3.0, tab_ink);
                txt.draw_up(cv, "b11", label, cx, 122.0 + len, th.title);
                (th.title, th.muted)
            }
            RailStyle::Dots => {
                cv.vgrad(
                    0.0,
                    0.0,
                    w,
                    LEN,
                    band1,
                    band1.mix(Color::rgba8(0, 0, 0, 255), 0.7),
                );
                if on {
                    cv.rect(0.0, 0.0, w, LEN, Color::rgba8(255, 255, 255, 30));
                }
                // one filled dot and two rings: three things to open
                for (i, y) in [44.0f32, 62.0, 80.0].into_iter().enumerate() {
                    if i == 0 {
                        cv.round(cx, y, 4.5, 0.0, th.accent);
                    } else {
                        cv.round(cx, y, 4.5, 2.8, faint(on_band));
                    }
                }
                (on_band, faint(on_band))
            }
        };
        match st {
            RailStyle::Tab | RailStyle::Dots => {}
            _ => {
                chevron(cv, cx + out * 1.5, 44.0, out, 5.5, 3.0, ink);
                txt.draw_up(cv, "b11", label, cx, LEN / 2.0 + len / 2.0, ink);
            }
        }
        if hot {
            cv.round(hx, hy, hide.2 / 2.0, 0.0, th.error);
        }
        cross(
            cv,
            hx,
            hy,
            4.0,
            2.0,
            if hot { th.light_square } else { foot },
        );
        return;
    }
    cv.clear(band0);
    cv.rect(2.0, 2.0, w - 4.0, LEN - 4.0, th.background);
    // header: the name, an arrow that folds the panel (pointing at the board) and a cross to hide it
    let towards_board = if left { 1.0 } else { -1.0 };
    let (edge, ink, hover_ink) = match st {
        RailStyle::Tab => {
            cv.rect(0.0, 0.0, w, HEAD, th.background);
            cv.rect(0.0, HEAD - 2.0, w, 2.0, band0);
            cv.rect(0.0, 0.0, w, 2.0, band0);
            txt.draw(cv, "b18", title, 14.0, 24.0, th.title);
            (band0, band0, band0)
        }
        RailStyle::Pale => {
            cv.rect(0.0, 0.0, w, HEAD, band0);
            txt.draw(cv, "b18", title, 14.0, 24.0, on_band);
            (on_band, on_band, band0)
        }
        RailStyle::Spine => {
            cv.vgrad(0.0, 0.0, w, HEAD, band0, band1);
            txt.draw(cv, "b18", title, 14.0, 23.0, on_band);
            cv.rect(14.0, HEAD - 5.0, 34.0, 3.0, th.accent); // a gold underline
            (on_band, on_band, band0)
        }
        RailStyle::Dots => {
            cv.vgrad(0.0, 0.0, w, HEAD, band1, band0);
            cv.round(22.0, 17.0, 4.5, 0.0, th.accent);
            txt.draw(cv, "b18", title, 36.0, 24.0, on_band);
            (on_band, on_band, band0)
        }
    };
    let colors = (edge, ink, hover_ink);
    header_button(cv, th, toggle, Some(towards_board), on, colors);
    header_button(cv, th, hide, None, hot, colors);
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
            txt.draw(cv, "r14", "Looks", 14.0, top + 28.0, th.muted);
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
                th.board_border,
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
            // every piece of the pack, white then black, on the board's two colours
            let order = [
                Role::King,
                Role::Queen,
                Role::Rook,
                Role::Bishop,
                Role::Knight,
                Role::Pawn,
            ];
            for (row, side) in [Side::White, Side::Black].into_iter().enumerate() {
                for (i, role) in order.into_iter().enumerate() {
                    let tx = 14.0 + i as f32 * 30.0;
                    let ty = y + row as f32 * 30.0;
                    let dark = (i + row) % 2 == 0;
                    let sq = if dark {
                        th.dark_square
                    } else {
                        th.light_square
                    };
                    cv.rect(tx, ty, 30.0, 30.0, sq);
                    let (sx, sy) = piece_tile(role, side);
                    let k = v.tile / 128.0;
                    cv.image(
                        v.pieces,
                        (sx * k, sy * k, v.tile, v.tile),
                        (tx + 1.0, ty + 1.0, 28.0, 28.0),
                        None,
                    );
                }
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
            style: RailStyle::Dots,
        };
        assert_eq!(Rails::from_text(&r.to_text()), r);
        let odd = Rails::from_text("keys = sideways\nlook = open\nnonsense\n");
        assert_eq!((odd.keys, odd.look), (Mode::Rail, Mode::Open));
    }

    /// Draws the rail styles side by side to /tmp/rail-styles.png, to pick from.
    /// Run by hand: cargo test draw_the_rail_style_sheet -- --ignored
    #[test]
    #[ignore]
    fn draw_the_rail_style_sheet() {
        use crate::canvas::Color;
        use crate::pack::{load_pack, pack_index};
        use crate::text::Txt;
        let (th, pieces) = load_pack(pack_index("blue-gold").unwrap());
        let txt = Txt::load();
        let names = vec![
            "Blue and Gold".to_string(),
            "No Crosses".into(),
            "Wood".into(),
        ];
        let scale = 2.0f32;
        let render = |style: RailStyle, p: Panel, mode: Mode, hover: Option<Hit>| {
            let mut rails = Rails::new();
            rails.style = style;
            match p {
                Panel::Keys => rails.keys = mode,
                Panel::Look => rails.look = mode,
            }
            let (w, h) = (
                (Rails::width_of(p, mode) * scale) as usize,
                (LEN * scale) as usize,
            );
            let mut px = vec![0u32; w * h];
            let view = View {
                th: &th,
                pieces: &pieces,
                tile: 128.0,
                txt: &txt,
                kb_file: None,
                kb_rank: None,
                thinking_dots: 1,
                show_pack_name: false,
                rails: &rails,
                pack: 0,
                pack_names: &names,
                slide: None,
            };
            draw_panel(&mut Canvas::new(&mut px, w, h, scale), &view, p, hover);
            (px, w, h)
        };
        let (col_w, gap) = (310.0f32, 26.0f32);
        let (sw, sh) = (
            ((col_w + gap) * 4.0 + gap) as usize * 2,
            (LEN + 70.0) as usize * 2,
        );
        let mut sheet = vec![0x002C2C30u32; sw * sh];
        let blit = |sheet: &mut Vec<u32>, src: &(Vec<u32>, usize, usize), x: f32, y: f32| {
            let (ox, oy) = ((x * scale) as usize, (y * scale) as usize);
            for row in 0..src.2 {
                let d = (oy + row) * sw + ox;
                sheet[d..d + src.1].copy_from_slice(&src.0[row * src.1..(row + 1) * src.1]);
            }
        };
        let labels = ["1  Pale", "2  Spine", "3  Tab", "4  Dots"];
        for (i, style) in STYLES.into_iter().enumerate() {
            let x0 = gap + i as f32 * (col_w + gap);
            blit(
                &mut sheet,
                &render(style, Panel::Keys, Mode::Rail, None),
                x0,
                56.0,
            );
            blit(
                &mut sheet,
                &render(style, Panel::Look, Mode::Rail, Some(Hit::Toggle)),
                x0 + 34.0,
                56.0,
            );
            blit(
                &mut sheet,
                &render(style, Panel::Keys, Mode::Open, Some(Hit::Toggle)),
                x0 + 74.0,
                56.0,
            );
            let mut cv = Canvas::new(&mut sheet, sw, sh, scale);
            txt.draw(
                &mut cv,
                "b18",
                labels[i],
                x0,
                36.0,
                Color::rgba8(240, 240, 240, 255),
            );
        }
        let mut cv = Canvas::new(&mut sheet, sw, sh, scale);
        txt.draw(
            &mut cv,
            "r14",
            "thin rail / thin rail with the pointer on it / open panel",
            gap,
            LEN + 64.0,
            Color::rgba8(160, 160, 170, 255),
        );
        cv.save_png(std::path::Path::new("/tmp/rail-styles.png"), None)
            .unwrap();
    }
}
