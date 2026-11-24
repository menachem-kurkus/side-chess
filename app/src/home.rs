//! The home screen and its two sub-pages (Keybinds, and Look and pieces). They are drawn
//! in the board's own window, from the same colours (the pack in use) and the same baked
//! text as the board, so they add no pictures, no window and no memory worth measuring.
//!
//! Top to bottom: a big Score (your rating and the AI's, from rated games against the AI,
//! and your record), what the app is using (memory against the budget, redraw time), then
//! the five things to do: play the AI, play in person, see replays, change keybinds,
//! manage packs.
use crate::canvas::{Canvas, Color};
use crate::draw::{H, W};
use crate::keys::{Keymap, ACTIONS};
use crate::pack::Theme;
use crate::rating::{self, Rating};
use crate::replay::{IN_PERSON_BLACK, IN_PERSON_WHITE};
use crate::resources::{self, LIMIT_MB};
use crate::save::games_dir;
use crate::text::Txt;
use std::fs;

const PAD: f32 = 28.0; // the same side margin as the board's
const BAND_H: f32 = 78.0; // the header band across the top

/// Which page the window is showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Home,
    Game,
    Keys,
    Packs,
    Engine, // choosing a strength to play Stockfish at
}

/// The six things to do from the home screen, top to bottom.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    PlayAi,
    Stockfish,
    InPerson,
    Replays,
    Keybinds,
    Packs,
}

pub const ITEMS: [Item; 6] = [
    Item::PlayAi,
    Item::Stockfish,
    Item::InPerson,
    Item::Replays,
    Item::Keybinds,
    Item::Packs,
];

impl Item {
    pub fn title(self) -> &'static str {
        match self {
            Item::PlayAi => "Play against AI",
            Item::Stockfish => "Play against Stockfish",
            Item::InPerson => "Play in person",
            Item::Replays => "See replays",
            Item::Keybinds => "Change keybinds",
            Item::Packs => "Manage packs",
        }
    }
}

/// What the Score and Resources blocks show. Loaded when the home screen opens; the
/// live numbers (memory, redraw time) are read again while it stays open.
#[derive(Clone, Debug, Default)]
pub struct Info {
    pub you: Rating,
    pub ai: Rating,
    pub won: u32,
    pub drawn: u32,
    pub lost: u32,
    pub in_person: u32,
    pub sf_won: u32, // games against Stockfish, kept apart from the ratings
    pub sf_drawn: u32,
    pub sf_lost: u32,
    pub saved: u32,
    pub stockfish: bool,
    pub memory_mb: Option<f32>,
    pub redraw_ms: f32,
}

/// Add one saved game to the record, from its PGN headers only (no moves are read).
/// A game two people played in person is counted on its own: it is nobody's win or loss.
/// An unfinished game counts as saved but has no result yet.
pub fn tally(pgn: &str, info: &mut Info) {
    let header = |name: &str| -> String {
        pgn.lines()
            .find_map(|l| {
                let rest = l.trim().strip_prefix('[')?.strip_prefix(name)?;
                rest.starts_with(' ').then(|| {
                    rest.trim()
                        .trim_end_matches(']')
                        .trim()
                        .trim_matches('"')
                        .to_string()
                })
            })
            .unwrap_or_default()
    };
    info.saved += 1;
    let (white, black, result) = (header("White"), header("Black"), header("Result"));
    if white == IN_PERSON_WHITE && black == IN_PERSON_BLACK {
        info.in_person += 1;
        return;
    }
    let me_black = black == "You";
    let vs_stockfish = white.starts_with("Stockfish") || black.starts_with("Stockfish");
    // a win, a draw or a loss for the person; None while the game is unfinished
    let outcome = match (result.as_str(), me_black) {
        ("1-0", false) | ("0-1", true) => Some(1),
        ("1-0", true) | ("0-1", false) => Some(-1),
        ("1/2-1/2", _) => Some(0),
        _ => None,
    };
    let (won, drawn, lost) = if vs_stockfish {
        (&mut info.sf_won, &mut info.sf_drawn, &mut info.sf_lost)
    } else {
        (&mut info.won, &mut info.drawn, &mut info.lost)
    };
    match outcome {
        Some(1) => *won += 1,
        Some(0) => *drawn += 1,
        Some(_) => *lost += 1,
        None => {}
    }
}

impl Info {
    pub fn load() -> Info {
        let r = rating::load();
        let mut info = Info {
            you: r.you,
            ai: r.ai,
            stockfish: crate::engine::find_engine().is_some(),
            ..Info::default()
        };
        if let Ok(dir) = fs::read_dir(games_dir()) {
            for e in dir.flatten() {
                let path = e.path();
                if path.extension().is_some_and(|x| x == "pgn") {
                    if let Ok(text) = fs::read_to_string(&path) {
                        tally(&text, &mut info);
                    }
                }
            }
        }
        info.refresh();
        info
    }

    /// Read the live numbers again.
    pub fn refresh(&mut self) {
        self.memory_mb = resources::memory_mb();
    }
}

// How the gauge reads, from the share of the memory budget in use
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Fine,
    #[allow(dead_code)] // until gauge_level below can return it
    Warn,
    Over,
}

/// How worried the memory gauge should look. `share` is memory used divided by the budget
/// (resources::LIMIT_MB): 0.3 is 30 percent of it, 1.0 is the whole budget.
// TODO(human): decide where Fine turns into Warn. Over is already "past the budget".
// The app normally sits near 0.3 on Windows, so a Warn threshold that is too low would
// light up all the time and one that is too high would never warn before CI fails.
pub fn gauge_level(share: f32) -> Level {
    if share >= 1.0 {
        Level::Over
    } else {
        Level::Fine
    }
}

fn level_color(th: &Theme, share: f32) -> Color {
    match gauge_level(share) {
        Level::Fine => th.accent,
        Level::Warn => th.accent.mix(th.error, 0.45),
        Level::Over => th.error,
    }
}

// ---- layout -------------------------------------------------------------------

const MENU_TOP: f32 = 378.0;
const ROW_H: f32 = 46.0;
const ROW_GAP: f32 = 5.0;

type Rect = (f32, f32, f32, f32); // x, y, width, height (points)

fn inside(r: Rect, x: f32, y: f32) -> bool {
    (r.0..=r.0 + r.2).contains(&x) && (r.1..=r.1 + r.3).contains(&y)
}

/// The box of the i-th thing to do.
pub fn item_rect(i: usize) -> Rect {
    (
        PAD,
        MENU_TOP + i as f32 * (ROW_H + ROW_GAP),
        W - 2.0 * PAD,
        ROW_H,
    )
}

/// The thing to do under a point, if any.
pub fn item_at(x: f32, y: f32) -> Option<usize> {
    (0..ITEMS.len()).find(|&i| inside(item_rect(i), x, y))
}

/// The Back button in the header of a sub-page.
pub fn back_rect() -> Rect {
    (W - PAD - 92.0, 22.0, 92.0, 34.0)
}

pub fn over_back(x: f32, y: f32) -> bool {
    inside(back_rect(), x, y)
}

// ---- drawing helpers ------------------------------------------------------------

fn card(th: &Theme) -> Color {
    th.light_square.mix(th.background, 0.55)
}

/// The blended band across the top, with the title; returns the ink used on it.
fn band(cv: &mut Canvas<'_>, th: &Theme, txt: &Txt, title: &str) -> Color {
    let band0 = th.board_border;
    let band1 = band0.mix(Color::rgba8(0, 0, 0, 255), 0.72);
    let on_band = band0.ink(th.background, th.text);
    cv.vgrad(0.0, 0.0, W, BAND_H, band0, band1);
    cv.rect(PAD, 22.0, 6.0, 36.0, th.accent);
    txt.draw(cv, "b30", title, PAD + 18.0, 51.0, on_band);
    on_band
}

fn back_button(cv: &mut Canvas<'_>, th: &Theme, txt: &Txt, hover: bool) {
    let (x, y, w, h) = back_rect();
    let on_band = th.board_border.ink(th.background, th.text);
    let fill = if hover {
        th.accent
    } else {
        th.board_border.mix(on_band, 0.78)
    };
    cv.rrect(x, y, w, h, 17.0, fill);
    let ink = fill.ink(th.background, th.text);
    let label = "Back";
    txt.draw(
        cv,
        "b18",
        label,
        x + (w - txt.width("b18", label)) / 2.0,
        y + 23.0,
        ink,
    );
}

fn chevron(cv: &mut Canvas<'_>, cx: f32, cy: f32, t: f32, ink: Color) {
    cv.line(cx - 4.0, cy - 8.0, cx + 4.0, cy, t, ink);
    cv.line(cx + 4.0, cy, cx - 4.0, cy + 8.0, t, ink);
}

/// A small drawing for each thing to do, from lines and boxes (no pictures).
fn icon(cv: &mut Canvas<'_>, item: Item, cx: f32, cy: f32, ink: Color) {
    match item {
        // a slider: how strong Stockfish plays
        Item::Stockfish => {
            cv.line(cx - 10.0, cy, cx + 10.0, cy, 2.2, ink);
            cv.round(cx + 3.0, cy, 5.5, 0.0, ink);
            for dx in [-8.0f32, -3.0, 8.0] {
                cv.line(cx + dx, cy - 8.0, cx + dx, cy - 5.0, 1.6, ink);
            }
        }
        // a chip: the AI
        Item::PlayAi => {
            cv.rect_lines(cx - 8.0, cy - 8.0, 16.0, 16.0, 2.0, ink);
            cv.round(cx, cy, 3.4, 0.0, ink);
            for d in [-4.0f32, 4.0] {
                cv.line(cx - 12.0, cy + d, cx - 8.0, cy + d, 1.8, ink);
                cv.line(cx + 8.0, cy + d, cx + 12.0, cy + d, 1.8, ink);
                cv.line(cx + d, cy - 12.0, cx + d, cy - 8.0, 1.8, ink);
                cv.line(cx + d, cy + 8.0, cx + d, cy + 12.0, 1.8, ink);
            }
        }
        // two people side by side
        Item::InPerson => {
            for dx in [-6.5f32, 6.5] {
                cv.round(cx + dx, cy - 5.0, 4.2, 0.0, ink);
                cv.rrect(cx + dx - 5.5, cy + 1.5, 11.0, 9.0, 4.5, ink);
            }
        }
        // a play button
        Item::Replays => {
            cv.round(cx, cy, 12.5, 10.5, ink);
            for i in 0..9 {
                let h = 6.5 * (1.0 - i as f32 / 9.0);
                cv.rect(cx - 3.0 + i as f32, cy - h, 1.0, 2.0 * h, ink);
            }
        }
        // a key cap
        Item::Keybinds => {
            cv.rect_lines(cx - 11.0, cy - 9.0, 22.0, 18.0, 2.0, ink);
            for dx in [-5.0f32, 0.0, 5.0] {
                cv.round(cx + dx, cy - 3.5, 1.7, 0.0, ink);
            }
            cv.rect(cx - 7.0, cy + 2.0, 14.0, 3.0, ink);
        }
        // a few squares of a board
        Item::Packs => {
            cv.rect(cx - 10.0, cy - 10.0, 10.0, 10.0, ink);
            cv.rect(cx, cy, 10.0, 10.0, ink);
            cv.rect_lines(cx, cy - 10.0, 10.0, 10.0, 1.8, ink);
            cv.rect_lines(cx - 10.0, cy, 10.0, 10.0, 1.8, ink);
        }
    }
}

// ---- the home screen --------------------------------------------------------------

/// Draw the home screen. `subs` is the line of small print under each thing to do;
/// `focus` is the one the keyboard or the pointer is on.
pub fn draw_home(
    cv: &mut Canvas<'_>,
    th: &Theme,
    txt: &Txt,
    info: &Info,
    subs: &[String; 6],
    focus: usize,
) {
    cv.clear(th.background);
    let on_band = band(cv, th, txt, "Side Chess");
    let tag = "Home";
    let tw = txt.width("b18", tag);
    let capsule = th.board_border.mix(on_band, 0.78);
    cv.rrect(W - PAD - tw - 28.0, 20.0, tw + 28.0, 36.0, 18.0, capsule);
    txt.draw(
        cv,
        "b18",
        tag,
        W - PAD - tw - 14.0,
        44.0,
        capsule.ink(th.background, th.text),
    );

    // Score: the big header, then you and the AI side by side
    txt.draw_scaled(cv, "b30", "Score", PAD, 138.0, th.title, 1.5);
    let note = "rated games against the AI";
    txt.draw(
        cv,
        "r14",
        note,
        W - PAD - txt.width("r14", note),
        134.0,
        th.muted,
    );
    let cw = (W - 2.0 * PAD - 14.0) / 2.0;
    for (i, (who, r)) in [("You", &info.you), ("AI", &info.ai)]
        .into_iter()
        .enumerate()
    {
        let x = PAD + i as f32 * (cw + 14.0);
        cv.rrect(x, 150.0, cw, 76.0, 12.0, card(th));
        txt.draw(cv, "r14", who, x + 16.0, 174.0, th.muted);
        let (big, small) = if r.games == 0 {
            ("-".to_string(), "not rated yet".to_string())
        } else {
            (
                format!("{:.0}", r.rating),
                format!(
                    "+/- {:.0}   {} game{}",
                    r.rd,
                    r.games,
                    if r.games == 1 { "" } else { "s" }
                ),
            )
        };
        txt.draw_scaled(cv, "b30", &big, x + 16.0, 214.0, th.title, 1.5);
        let bw = txt.width_scaled("b30", &big, 1.5);
        txt.draw(cv, "r14", &small, x + 16.0 + bw + 12.0, 214.0, th.muted);
    }
    let record = format!(
        "Won {}    Drawn {}    Lost {}",
        info.won, info.drawn, info.lost
    );
    txt.draw(cv, "b18", &record, PAD, 252.0, th.text);
    let more = format!("in person {}   saved {}", info.in_person, info.saved);
    txt.draw(
        cv,
        "r14",
        &more,
        W - PAD - txt.width("r14", &more),
        252.0,
        th.muted,
    );
    // games against Stockfish are kept apart: they never move the ratings above
    if info.sf_won + info.sf_drawn + info.sf_lost > 0 {
        let sf = format!(
            "against Stockfish   won {}   drawn {}   lost {}",
            info.sf_won, info.sf_drawn, info.sf_lost
        );
        txt.draw(cv, "r14", &sf, PAD, 272.0, th.muted);
    }

    // Resources used: memory against the budget, then the rest
    txt.draw(cv, "b18", "Resources used", PAD, 304.0, th.title);
    let budget = format!("budget {LIMIT_MB:.0} MB");
    txt.draw(
        cv,
        "r14",
        &budget,
        W - PAD - txt.width("r14", &budget),
        304.0,
        th.muted,
    );
    let (bx, by, bw, bh) = (PAD, 314.0, W - 2.0 * PAD, 12.0);
    cv.rrect(bx, by, bw, bh, 6.0, th.dark_square.mix(th.background, 0.35));
    let share = info.memory_mb.map_or(0.0, resources::share);
    if share > 0.0 {
        cv.rrect(
            bx,
            by,
            (bw * share.min(1.0)).max(bh),
            bh,
            6.0,
            level_color(th, share),
        );
    }
    let mem = match info.memory_mb {
        Some(mb) => format!("Memory  {mb:.1} MB of {LIMIT_MB:.0} MB"),
        None => "Memory  not available here".to_string(),
    };
    txt.draw(cv, "r14", &mem, PAD, 348.0, th.text);
    let redraw = if info.redraw_ms > 0.0 {
        format!("Last redraw  {:.1} ms", info.redraw_ms)
    } else {
        "Last redraw  not measured yet".to_string()
    };
    txt.draw(
        cv,
        "r14",
        &redraw,
        W - PAD - txt.width("r14", &redraw),
        348.0,
        th.text,
    );
    let engine = if info.stockfish {
        "Stockfish  found (replays are rated)"
    } else {
        "Stockfish  not installed (optional)"
    };
    txt.draw(cv, "r14", engine, PAD, 368.0, th.muted);
    let idle = "redraws only when something changes";
    txt.draw(
        cv,
        "r14",
        idle,
        W - PAD - txt.width("r14", idle),
        368.0,
        th.muted,
    );

    // The six things to do
    for (i, item) in ITEMS.into_iter().enumerate() {
        let (x, y, w, h) = item_rect(i);
        let primary = i == 0;
        let fill = if primary { th.accent } else { card(th) };
        cv.rrect(x, y, w, h, 12.0, fill);
        if i == focus {
            cv.rect_lines(x, y, w, h, 3.0, th.title);
        }
        let ink = if primary {
            fill.ink(th.title, th.background)
        } else {
            th.title
        };
        let sub_ink = if primary { ink } else { th.muted };
        icon(cv, item, x + 34.0, y + h / 2.0, ink);
        txt.draw(cv, "b18", item.title(), x + 66.0, y + 20.0, ink);
        txt.draw(cv, "r14", &subs[i], x + 66.0, y + 38.0, sub_ink);
        chevron(cv, x + w - 24.0, y + h / 2.0, 2.4, ink);
    }
}

// ---- the keybinds page ------------------------------------------------------------

const KEYS_TOP: f32 = 126.0;
const KEYS_PITCH: f32 = 40.0;
const KEYS_ROW_H: f32 = 34.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeysHit {
    Row(usize),
    Reset,
    Back,
}

fn keys_row_rect(i: usize) -> Rect {
    (
        PAD,
        KEYS_TOP + i as f32 * KEYS_PITCH,
        W - 2.0 * PAD,
        KEYS_ROW_H,
    )
}

fn reset_rect() -> Rect {
    ((W - 220.0) / 2.0, H - 72.0, 220.0, 40.0)
}

pub fn keys_hit(x: f32, y: f32) -> Option<KeysHit> {
    if over_back(x, y) {
        return Some(KeysHit::Back);
    }
    if inside(reset_rect(), x, y) {
        return Some(KeysHit::Reset);
    }
    (0..ACTIONS.len())
        .find(|&i| inside(keys_row_rect(i), x, y))
        .map(KeysHit::Row)
}

/// The Keybinds page. `waiting` is the row that is asking for a new key; `msg` says why
/// the last key was refused (empty if it wasn't).
pub fn draw_keys(
    cv: &mut Canvas<'_>,
    th: &Theme,
    txt: &Txt,
    keys: &Keymap,
    waiting: Option<usize>,
    msg: &str,
    hover: Option<KeysHit>,
) {
    cv.clear(th.background);
    band(cv, th, txt, "Keybinds");
    back_button(cv, th, txt, hover == Some(KeysHit::Back));
    txt.draw(
        cv,
        "r14",
        "Click a row, then press the new key. Esc cancels.",
        PAD,
        108.0,
        th.muted,
    );
    for (i, a) in ACTIONS.into_iter().enumerate() {
        let (x, y, w, h) = keys_row_rect(i);
        let on = hover == Some(KeysHit::Row(i));
        cv.rrect(
            x,
            y,
            w,
            h,
            9.0,
            if on || waiting == Some(i) {
                th.half_typed
            } else {
                card(th)
            },
        );
        txt.draw(cv, "r16", a.label(), x + 16.0, y + 23.0, th.title);
        // the key as a cap on the right; a gold one while it waits for a key
        let (label, cap_w) = if waiting == Some(i) {
            ("Press a key".to_string(), 120.0)
        } else {
            (keys.label_of(a), 44.0)
        };
        let cx = x + w - 14.0 - cap_w;
        let cap = if waiting == Some(i) {
            th.accent
        } else {
            th.board_border
        };
        cv.rrect(cx, y + 4.0, cap_w, h - 8.0, 7.0, cap);
        let lw = txt.width("b18", &label);
        txt.draw(
            cv,
            "b18",
            &label,
            cx + (cap_w - lw) / 2.0,
            y + 23.0,
            cap.ink(th.title, th.background),
        );
    }
    if !msg.is_empty() {
        txt.draw(cv, "r14", msg, PAD, H - 130.0, th.error);
    }
    let note = "A-H and 1-8 name squares, so they can't be used. Quit is Ctrl+Q.";
    txt.draw(cv, "r14", note, PAD, H - 106.0, th.muted);
    let (x, y, w, h) = reset_rect();
    let changed = !keys.is_default();
    let on = hover == Some(KeysHit::Reset) && changed;
    cv.rrect(
        x,
        y,
        w,
        h,
        12.0,
        if on {
            th.accent
        } else if changed {
            card(th)
        } else {
            th.background
        },
    );
    if !changed {
        cv.rect_lines(x, y, w, h, 1.5, th.muted);
    }
    let label = "Reset to defaults";
    let ink = if on {
        th.accent.ink(th.title, th.background)
    } else if changed {
        th.title
    } else {
        th.muted
    };
    txt.draw(
        cv,
        "b18",
        label,
        x + (w - txt.width("b18", label)) / 2.0,
        y + 26.0,
        ink,
    );
}

// ---- the packs page ---------------------------------------------------------------

const LIST_TOP: f32 = 130.0;
const COL_GAP: f32 = 20.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PacksHit {
    Colours(usize),
    Pieces(usize), // row 0 is "same as the colours", then each piece set
    Back,
}

fn list_pitch(rows: usize) -> f32 {
    ((H - LIST_TOP - 70.0) / rows.max(1) as f32).min(34.0)
}

fn col_w() -> f32 {
    (W - 2.0 * PAD - COL_GAP) / 2.0
}

pub fn packs_hit(x: f32, y: f32, colours: usize, pieces_rows: usize) -> Option<PacksHit> {
    if over_back(x, y) {
        return Some(PacksHit::Back);
    }
    let (right, cw) = (x >= PAD + col_w() + COL_GAP, col_w());
    let rows = colours.max(pieces_rows);
    let pitch = list_pitch(rows);
    let left_edge = if right { PAD + cw + COL_GAP } else { PAD };
    if !(left_edge..=left_edge + cw).contains(&x) || y < LIST_TOP {
        return None;
    }
    let i = ((y - LIST_TOP) / pitch) as usize;
    match (right, i) {
        (false, i) if i < colours => Some(PacksHit::Colours(i)),
        (true, i) if i < pieces_rows => Some(PacksHit::Pieces(i)),
        _ => None,
    }
}

/// The Look and pieces page: two lists, a click picks. `swatches` is each look's light
/// square, dark square and accent; `pieces` lists the piece sets (after "Same as colours").
#[allow(clippy::too_many_arguments)]
pub fn draw_packs(
    cv: &mut Canvas<'_>,
    th: &Theme,
    txt: &Txt,
    names: &[String],
    swatches: &[[Color; 3]],
    pieces: &[String],
    pack: usize,
    pieces_row: usize,
    hover: Option<PacksHit>,
) {
    cv.clear(th.background);
    band(cv, th, txt, "Look and pieces");
    back_button(cv, th, txt, hover == Some(PacksHit::Back));
    let cw = col_w();
    let (c1, c2) = (PAD, PAD + cw + COL_GAP);
    txt.draw(cv, "r14", "Colours", c1, 112.0, th.muted);
    txt.draw(cv, "r14", "Pieces", c2, 112.0, th.muted);
    let rows = names.len().max(pieces.len() + 1);
    let pitch = list_pitch(rows);
    let row_box = |cv: &mut Canvas<'_>, col: f32, i: usize, chosen: bool, hot: bool| {
        let y = LIST_TOP + i as f32 * pitch;
        cv.rrect(
            col,
            y + 1.0,
            cw,
            pitch - 3.0,
            8.0,
            if hot {
                th.half_typed
            } else if chosen {
                card(th)
            } else {
                th.background
            },
        );
        if chosen {
            cv.rect(col, y + 5.0, 4.0, pitch - 11.0, th.accent);
        }
        y + pitch / 2.0 + 5.0
    };
    for (i, name) in names.iter().enumerate() {
        let chosen = i == pack;
        let base = row_box(cv, c1, i, chosen, hover == Some(PacksHit::Colours(i)));
        if let Some(sw) = swatches.get(i) {
            for (k, c) in sw.iter().enumerate() {
                cv.rect(c1 + 14.0 + k as f32 * 13.0, base - 12.0, 12.0, 14.0, *c);
            }
        }
        let (style, ink) = if chosen {
            ("r16", th.title)
        } else {
            ("r14", th.muted)
        };
        txt.draw(cv, style, name, c1 + 62.0, base, ink);
    }
    let pieces_all = std::iter::once("Same as colours").chain(pieces.iter().map(String::as_str));
    for (i, name) in pieces_all.enumerate() {
        let chosen = i == pieces_row;
        let base = row_box(cv, c2, i, chosen, hover == Some(PacksHit::Pieces(i)));
        let (style, ink) = if chosen {
            ("r16", th.title)
        } else {
            ("r14", th.muted)
        };
        txt.draw(cv, style, name, c2 + 16.0, base, ink);
    }
    let foot = "Add your own: put a folder in app/packs and rebuild (see packs/README).";
    txt.draw(cv, "r14", foot, PAD, H - 30.0, th.muted);
}

// ---- playing Stockfish: pick a strength and a side -------------------------------

/// What the person chose for a game against Stockfish; kept in ~/.side-chess/engine.txt.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Choice {
    pub elo: u32,
    pub white: bool, // the person plays white
}

impl Default for Choice {
    fn default() -> Self {
        Choice {
            elo: 1500,
            white: true,
        }
    }
}

impl Choice {
    /// Move the strength by `delta` Elo points, staying inside what Stockfish accepts.
    pub fn step(&mut self, delta: i32) {
        self.elo = crate::engine::clamp_elo((self.elo as i32 + delta).max(0) as u32);
    }

    /// Set the strength from typed digits (the typed text is clamped, so 99 means the weakest
    /// and 9999 the strongest); anything that is not a number changes nothing.
    pub fn set_typed(&mut self, typed: &str) {
        if let Ok(n) = typed.trim().parse::<u32>() {
            self.elo = crate::engine::clamp_elo(n);
        }
    }

    pub fn to_text(self) -> String {
        format!(
            "elo {}\nside {}\n",
            self.elo,
            if self.white { "white" } else { "black" }
        )
    }

    pub fn from_text(text: &str) -> Choice {
        let mut c = Choice::default();
        for l in text.lines() {
            let mut w = l.split_whitespace();
            match (w.next(), w.next()) {
                (Some("elo"), Some(n)) => c.set_typed(n),
                (Some("side"), Some(s)) => c.white = s != "black",
                _ => {}
            }
        }
        c
    }

    pub fn load() -> Choice {
        fs::read_to_string(crate::game::live_dir().join("engine.txt"))
            .map(|t| Choice::from_text(&t))
            .unwrap_or_default()
    }

    pub fn save(self) {
        let _ = fs::create_dir_all(crate::game::live_dir());
        let _ = fs::write(crate::game::live_dir().join("engine.txt"), self.to_text());
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EngineHit {
    Back,
    Minus,
    Plus,
    Number, // the big number: click it, then type
    White,
    Black,
    Start,    // a new game at this strength
    Continue, // carry on with the game in progress
}

const STEP_BTN: f32 = 64.0;

fn minus_rect() -> Rect {
    (PAD + 24.0, 168.0, STEP_BTN, STEP_BTN)
}

fn plus_rect() -> Rect {
    (W - PAD - 24.0 - STEP_BTN, 168.0, STEP_BTN, STEP_BTN)
}

fn number_rect() -> Rect {
    let (mx, _, mw, _) = minus_rect();
    let (px, ..) = plus_rect();
    (mx + mw + 12.0, 158.0, px - mx - mw - 24.0, 84.0)
}

fn side_rect(white: bool) -> Rect {
    let w = (W - 2.0 * PAD - 14.0) / 2.0;
    (PAD + if white { 0.0 } else { w + 14.0 }, 360.0, w, 56.0)
}

/// The button that carries on with a game in progress (only there when there is one).
fn continue_rect() -> Rect {
    (PAD, 450.0, W - 2.0 * PAD, 60.0)
}

/// Start a new game: the main button, or the one under Continue when a game is in progress.
fn start_rect(resuming: bool) -> Rect {
    if resuming {
        (PAD, 522.0, W - 2.0 * PAD, 52.0)
    } else {
        (PAD, 450.0, W - 2.0 * PAD, 60.0)
    }
}

/// `resuming` says a game against Stockfish is in progress, so Continue is on the page.
pub fn engine_hit(x: f32, y: f32, resuming: bool) -> Option<EngineHit> {
    if over_back(x, y) {
        return Some(EngineHit::Back);
    }
    let mut hits = vec![
        (minus_rect(), EngineHit::Minus),
        (plus_rect(), EngineHit::Plus),
        (number_rect(), EngineHit::Number),
        (side_rect(true), EngineHit::White),
        (side_rect(false), EngineHit::Black),
        (start_rect(resuming), EngineHit::Start),
    ];
    if resuming {
        hits.push((continue_rect(), EngineHit::Continue));
    }
    hits.into_iter()
        .find(|(r, _)| inside(*r, x, y))
        .map(|(_, h)| h)
}

/// The Stockfish page. `typing` is the digits being typed into the big number (None when
/// it is not being edited); `found` says whether Stockfish is installed.
#[allow(clippy::too_many_arguments)]
pub fn draw_engine(
    cv: &mut Canvas<'_>,
    th: &Theme,
    txt: &Txt,
    choice: Choice,
    typing: Option<&str>,
    found: bool,
    resume: Option<String>,
    hover: Option<EngineHit>,
) {
    cv.clear(th.background);
    band(cv, th, txt, "Play Stockfish");
    back_button(cv, th, txt, hover == Some(EngineHit::Back));
    txt.draw(
        cv,
        "r14",
        "Stockfish plays the other side at the strength you choose.",
        PAD,
        108.0,
        th.muted,
    );
    txt.draw(cv, "b18", "Strength", PAD, 148.0, th.title);

    // the stepper: minus, the number (click to type), plus
    for (r, minus, hot) in [
        (minus_rect(), true, hover == Some(EngineHit::Minus)),
        (plus_rect(), false, hover == Some(EngineHit::Plus)),
    ] {
        let (x, y, w, h) = r;
        cv.rrect(x, y, w, h, 14.0, if hot { th.accent } else { card(th) });
        let ink = if hot {
            th.accent.ink(th.title, th.background)
        } else {
            th.title
        };
        cv.rect(x + 20.0, y + h / 2.0 - 2.0, w - 40.0, 4.0, ink);
        if !minus {
            cv.rect(x + w / 2.0 - 2.0, y + 20.0, 4.0, h - 40.0, ink);
        }
    }
    let (nx, ny, nw, nh) = number_rect();
    let editing = typing.is_some();
    if editing || hover == Some(EngineHit::Number) {
        cv.rrect(nx, ny, nw, nh, 12.0, th.half_typed);
    }
    let shown = match typing {
        Some(t) => format!("{t}_"),
        None => choice.elo.to_string(),
    };
    let sw = txt.width_scaled("b30", &shown, 2.0);
    txt.draw_scaled(
        cv,
        "b30",
        &shown,
        nx + (nw - sw) / 2.0,
        ny + 64.0,
        if editing {
            th.accent.mix(th.title, 0.6)
        } else {
            th.title
        },
        2.0,
    );
    let word = if editing {
        "Type the number, then Enter. Esc cancels."
    } else {
        crate::engine::elo_word(choice.elo)
    };
    txt.draw(
        cv,
        "r16",
        word,
        (W - txt.width("r16", word)) / 2.0,
        268.0,
        th.text,
    );
    let scale = format!(
        "Stockfish's own scale, {} to {}. Not the same as Lichess or chess.com.",
        crate::engine::MIN_ELO,
        crate::engine::MAX_ELO
    );
    txt.draw(
        cv,
        "r14",
        &scale,
        (W - txt.width("r14", &scale)) / 2.0,
        292.0,
        th.muted,
    );

    // which side you play
    txt.draw(cv, "b18", "You play", PAD, 346.0, th.title);
    for white in [true, false] {
        let (x, y, w, h) = side_rect(white);
        let chosen = choice.white == white;
        let hot = hover
            == Some(if white {
                EngineHit::White
            } else {
                EngineHit::Black
            });
        let fill = if chosen {
            th.accent
        } else if hot {
            th.half_typed
        } else {
            card(th)
        };
        cv.rrect(x, y, w, h, 12.0, fill);
        let label = if white { "White" } else { "Black" };
        let ink = if chosen {
            fill.ink(th.title, th.background)
        } else {
            th.title
        };
        txt.draw(
            cv,
            "b18",
            label,
            x + (w - txt.width("b18", label)) / 2.0,
            y + 35.0,
            ink,
        );
    }

    // carry on with a game in progress, or start a new one (Stockfish must be installed
    // to start; a game already begun can always be carried on)
    let resuming = resume.is_some();
    let button = |cv: &mut Canvas<'_>, r: Rect, label: &str, main: bool, on: bool, hot: bool| {
        let (x, y, w, h) = r;
        let fill = match (main, on, hot) {
            (_, false, _) => card(th),
            (true, true, true) => th.accent.mix(th.title, 0.85),
            (true, true, false) => th.accent,
            (false, true, true) => th.half_typed,
            (false, true, false) => card(th),
        };
        cv.rrect(x, y, w, h, 14.0, fill);
        let ink = match (main, on) {
            (_, false) => th.muted,
            (true, true) => fill.ink(th.title, th.background),
            (false, true) => th.title,
        };
        txt.draw(
            cv,
            "b18",
            label,
            x + (w - txt.width("b18", label)) / 2.0,
            y + h / 2.0 + 6.0,
            ink,
        );
    };
    if resuming {
        button(
            cv,
            continue_rect(),
            "Continue your game",
            true,
            true,
            hover == Some(EngineHit::Continue),
        );
    }
    button(
        cv,
        start_rect(resuming),
        if resuming {
            "Start a new game"
        } else {
            "Start game"
        },
        !resuming,
        found,
        hover == Some(EngineHit::Start),
    );
    let below = start_rect(resuming).1 + start_rect(resuming).3 + 28.0;
    if !found {
        let msg = "Stockfish isn't installed. Put it in ~/.side-chess/engine/ as";
        let msg2 = "stockfish (stockfish.exe on Windows). The README has the steps.";
        txt.draw(cv, "r14", msg, PAD, below, th.error);
        txt.draw(cv, "r14", msg2, PAD, below + 20.0, th.error);
    } else if let Some(r) = resume {
        txt.draw(cv, "r14", &r, PAD, below, th.muted);
    }
    let foot = "Saved and replayable like any game; it does not change your rating.";
    txt.draw(cv, "r14", foot, PAD, H - 30.0, th.muted);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(pgn: &str) -> Info {
        let mut i = Info::default();
        tally(pgn, &mut i);
        i
    }

    #[test]
    fn the_record_counts_results_from_the_persons_side_and_keeps_in_person_games_apart() {
        let won_as_black =
            count("[White \"AI\"]\n[Black \"You\"]\n[Result \"0-1\"]\n\n1. e4 0-1\n");
        assert_eq!((won_as_black.won, won_as_black.lost), (1, 0));
        let lost_as_white =
            count("[White \"You\"]\n[Black \"AI\"]\n[Result \"0-1\"]\n\n1. e4 0-1\n");
        assert_eq!((lost_as_white.won, lost_as_white.lost), (0, 1));
        let draw = count("[White \"You\"]\n[Black \"AI\"]\n[Result \"1/2-1/2\"]\n");
        assert_eq!(draw.drawn, 1);
        let open = count("[White \"You\"]\n[Black \"AI\"]\n[Result \"*\"]\n");
        assert_eq!((open.won, open.drawn, open.lost, open.saved), (0, 0, 0, 1));
        let two = count("[White \"Player 1\"]\n[Black \"Player 2\"]\n[Result \"1-0\"]\n");
        assert_eq!((two.in_person, two.won, two.lost), (1, 0, 0));
        // WhiteElo and friends are not the White name
        let elo = count("[WhiteElo \"2000\"]\n[White \"You\"]\n[Black \"AI\"]\n[Result \"1-0\"]\n");
        assert_eq!(elo.won, 1);
    }

    #[test]
    fn the_five_buttons_fit_in_the_window_without_touching() {
        for i in 0..ITEMS.len() {
            let (x, y, w, h) = item_rect(i);
            assert!(x >= 0.0 && x + w <= W && y + h <= H, "button {i}");
            if i > 0 {
                let (_, py, _, ph) = item_rect(i - 1);
                assert!(py + ph < y, "buttons {} and {i} overlap", i - 1);
            }
            assert_eq!(item_at(x + w / 2.0, y + h / 2.0), Some(i));
        }
        assert_eq!(item_at(5.0, 5.0), None);
    }

    #[test]
    fn the_keybind_rows_and_reset_button_fit_and_are_found_by_a_click() {
        for i in 0..ACTIONS.len() {
            let (x, y, w, h) = keys_row_rect(i);
            assert!(y + h < reset_rect().1 - 40.0, "row {i} runs into the notes");
            assert_eq!(keys_hit(x + w / 2.0, y + h / 2.0), Some(KeysHit::Row(i)));
        }
        let (x, y, w, h) = reset_rect();
        assert!(y + h <= H);
        assert_eq!(keys_hit(x + w / 2.0, y + h / 2.0), Some(KeysHit::Reset));
        let (x, y, w, h) = back_rect();
        assert_eq!(keys_hit(x + w / 2.0, y + h / 2.0), Some(KeysHit::Back));
    }

    #[test]
    fn a_click_in_each_pack_list_finds_its_row_however_many_looks_there_are() {
        for (colours, pieces) in [(13, 6), (4, 12), (20, 3)] {
            let pitch = list_pitch(colours.max(pieces));
            assert!(
                LIST_TOP + pitch * colours.max(pieces) as f32 <= H,
                "{colours}"
            );
            for i in 0..colours {
                let (x, y) = (PAD + 20.0, LIST_TOP + pitch * (i as f32 + 0.5));
                assert_eq!(packs_hit(x, y, colours, pieces), Some(PacksHit::Colours(i)));
            }
            for i in 0..pieces {
                let (x, y) = (
                    PAD + col_w() + COL_GAP + 20.0,
                    LIST_TOP + pitch * (i as f32 + 0.5),
                );
                assert_eq!(packs_hit(x, y, colours, pieces), Some(PacksHit::Pieces(i)));
            }
        }
        // the gap between the lists, and below the last row, are nothing
        assert_eq!(packs_hit(PAD + col_w() + 5.0, 200.0, 13, 6), None);
        assert_eq!(packs_hit(PAD + 20.0, H - 5.0, 3, 3), None);
    }

    #[test]
    fn the_gauge_is_fine_when_nearly_empty_and_over_past_the_budget() {
        assert_eq!(gauge_level(0.05), Level::Fine);
        assert_eq!(gauge_level(1.0), Level::Over);
        assert_eq!(gauge_level(1.4), Level::Over);
    }

    /// Not a check, a measurement: how long one frame of each page takes at a 125% screen.
    /// Run with `cargo test --release home_frame_speed -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn home_frame_speed() {
        use std::time::Instant;
        let th = Theme::load(crate::pack::saved_pack()).ok().expect("a pack");
        let mut txt = Txt::load();
        let fitted = std::env::var_os("NO_FIT").is_none();
        let t = std::time::Instant::now();
        if fitted {
            txt.fit(1.25);
        }
        eprintln!(
            "fit (once per screen scale): {:.2} ms",
            t.elapsed().as_secs_f32() * 1000.0
        );
        let (w, h, scale) = ((W * 1.25) as usize, (H * 1.25) as usize, 1.25f32);
        let mut px = vec![0u32; w * h];
        let info = Info::default();
        let subs: [String; 6] =
            std::array::from_fn(|i| format!("small print {i} for the speed test"));
        let mut time = |name: &str, f: &mut dyn FnMut(&mut Canvas<'_>)| {
            let mut cv = Canvas::new(&mut px, w, h, scale);
            f(&mut cv); // once to warm up
            let t = Instant::now();
            for _ in 0..40 {
                f(&mut cv);
            }
            eprintln!(
                "{name}: {:.2} ms",
                t.elapsed().as_secs_f32() * 1000.0 / 40.0
            );
        };
        time("home", &mut |cv| draw_home(cv, &th, &txt, &info, &subs, 0));
        time("clear only", &mut |cv| cv.clear(th.background));
        time("text only (300 letters, r14)", &mut |cv| {
            for i in 0..10 {
                txt.draw(
                    cv,
                    "r14",
                    "abcdefghijklmnopqrstuvwxyz0123",
                    10.0,
                    20.0 + 20.0 * i as f32,
                    th.text,
                );
            }
        });
        time("big score text only", &mut |cv| {
            txt.draw_scaled(cv, "b30", "Score 1338 1662", 10.0, 100.0, th.title, 1.5)
        });
    }

    #[test]
    fn the_strength_stays_inside_what_stockfish_accepts() {
        let mut c = Choice::default();
        assert_eq!((c.elo, c.white), (1500, true));
        c.step(-1000);
        assert_eq!(c.elo, crate::engine::MIN_ELO);
        c.step(5000);
        assert_eq!(c.elo, crate::engine::MAX_ELO);
        c.step(-100);
        assert_eq!(c.elo, crate::engine::MAX_ELO - 100);
        c.set_typed("1850");
        assert_eq!(c.elo, 1850);
        c.set_typed("99");
        assert_eq!(c.elo, crate::engine::MIN_ELO, "too low is the weakest");
        c.set_typed("9999");
        assert_eq!(c.elo, crate::engine::MAX_ELO, "too high is the strongest");
        c.set_typed("abc");
        assert_eq!(
            c.elo,
            crate::engine::MAX_ELO,
            "not a number changes nothing"
        );
    }

    #[test]
    fn the_choice_is_kept_and_a_bad_file_gives_the_defaults() {
        let c = Choice {
            elo: 2200,
            white: false,
        };
        assert_eq!(Choice::from_text(&c.to_text()), c);
        assert_eq!(Choice::from_text(""), Choice::default());
        assert_eq!(Choice::from_text("elo lots\nside sideways\n").elo, 1500);
        assert_eq!(Choice::from_text("elo 5\n").elo, crate::engine::MIN_ELO);
    }

    #[test]
    fn every_control_on_the_stockfish_page_is_found_by_a_click_and_fits_the_window() {
        for (r, hit) in [
            (minus_rect(), EngineHit::Minus),
            (plus_rect(), EngineHit::Plus),
            (number_rect(), EngineHit::Number),
            (side_rect(true), EngineHit::White),
            (side_rect(false), EngineHit::Black),
            (start_rect(false), EngineHit::Start),
        ] {
            assert!(r.0 >= 0.0 && r.0 + r.2 <= W && r.1 + r.3 <= H, "{hit:?}");
            assert_eq!(
                engine_hit(r.0 + r.2 / 2.0, r.1 + r.3 / 2.0, false),
                Some(hit)
            );
        }
        // with a game in progress Continue is on top and Start moves down under it
        for (r, hit) in [
            (continue_rect(), EngineHit::Continue),
            (start_rect(true), EngineHit::Start),
        ] {
            assert!(r.1 + r.3 <= H - 70.0, "{hit:?} leaves room for the notes");
            assert_eq!(
                engine_hit(r.0 + r.2 / 2.0, r.1 + r.3 / 2.0, true),
                Some(hit)
            );
        }
        assert_eq!(
            engine_hit(300.0, continue_rect().1 + 10.0, false),
            Some(EngineHit::Start),
            "without a game, that spot is the Start button"
        );
        let (x, y, w, h) = back_rect();
        assert_eq!(
            engine_hit(x + w / 2.0, y + h / 2.0, false),
            Some(EngineHit::Back)
        );
        // the buttons are separate from each other
        assert!(minus_rect().0 + minus_rect().2 < number_rect().0);
        assert!(number_rect().0 + number_rect().2 < plus_rect().0);
    }

    #[test]
    fn games_against_stockfish_are_counted_on_their_own() {
        let mut i = Info::default();
        tally(
            "[White \"You\"]\n[Black \"Stockfish 1500\"]\n[Result \"1-0\"]\n",
            &mut i,
        );
        tally(
            "[White \"Stockfish 2000\"]\n[Black \"You\"]\n[Result \"1-0\"]\n",
            &mut i,
        );
        tally(
            "[White \"You\"]\n[Black \"AI\"]\n[Result \"1-0\"]\n",
            &mut i,
        );
        assert_eq!((i.sf_won, i.sf_lost, i.sf_drawn), (1, 1, 0));
        assert_eq!((i.won, i.lost), (1, 0), "the AI record is not touched");
        assert_eq!(i.saved, 3);
    }
}
