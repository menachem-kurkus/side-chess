//! What the window shows. Every colour comes from the pack (`th`), every size
//! from the layout constants below, so a new look never needs code changes.
use crate::canvas::{Canvas, Color, Img};
use crate::game::{move_list, shown_to, sq, Game};
use crate::keys::{Action, Keymap};
use crate::pack::Theme;
use crate::rails::Rails;
use crate::text::Txt;
use shakmaty::{Color as Side, Position, Role, Square};

// Layout, in points
pub const SQ: f32 = 68.0; // one square
pub const BX: f32 = 28.0; // board left
pub const BY: f32 = 96.0; // board top
pub const W: f32 = BX * 2.0 + SQ * 8.0;
pub const H: f32 = BY + SQ * 8.0 + 68.0;
const HEADER_H: f32 = 78.0; // the header band across the top

/// Everything a frame needs to know beyond the game itself.
pub struct View<'a> {
    pub th: &'a Theme,
    pub pieces: &'a Img,
    pub tile: f32, // size of one piece tile in `pieces` (128, or what the screen scale needs)
    pub txt: &'a Txt,
    pub kb_file: Option<u32>, // half-typed square: the letter so far
    pub kb_rank: Option<u32>, // half-typed square: the number so far
    pub thinking_dots: usize, // 1..=3, animates the opponent's thinking status
    pub show_pack_name: bool, // for two seconds after T
    pub rails: &'a Rails,     // the thin side panels
    pub pack: usize,          // which look is in use
    pub pack_names: &'a [String],
    pub piece_names: &'a [String], // the piece sets in the Pieces list (after "Same as colours")
    pub pieces_row: usize,         // the row of the Pieces list in use (0 follows the colours)
    pub slide: Option<f32>,        // 0..1 while the last move's piece is sliding into place
    pub arrows: bool,              // X: arrows from every attacker to what it attacks
    pub replay: Option<&'a crate::replay::State>, // a saved game's list or a game being replayed
    pub keys: &'a Keymap,          // which key does what (shown where a key is named)
}

/// The Menu button in the header (it takes the person to the home screen): x, y, w, h.
pub const MENU_BTN: (f32, f32, f32, f32) = (150.0, 24.0, 74.0, 30.0);

/// Is the point on the header's Menu button?
pub fn menu_button_at(x: f32, y: f32) -> bool {
    let (bx, by, bw, bh) = MENU_BTN;
    (bx..=bx + bw).contains(&x) && (by..=by + bh).contains(&y)
}

/// Where a square is drawn. The player's own pieces are at the bottom, so the
/// board is turned round when the player has black (`flip`).
// The box that asks which piece a pawn becomes: four tiles in a row over the board
const PROMO_TILE: f32 = 68.0;
const PROMO_GAP: f32 = 14.0;
const PROMO_PAD: f32 = 24.0;
const PROMO_W: f32 = PROMO_PAD * 2.0 + PROMO_TILE * 4.0 + PROMO_GAP * 3.0;
const PROMO_H: f32 = 172.0;
const PROMO_ROLES: [(Role, &str); 4] = [
    (Role::Queen, "Q"),
    (Role::Rook, "R"),
    (Role::Bishop, "B"),
    (Role::Knight, "N"),
];

fn promo_origin() -> (f32, f32) {
    (
        BX + (SQ * 8.0 - PROMO_W) / 2.0,
        BY + (SQ * 8.0 - PROMO_H) / 2.0,
    )
}

/// The piece under a point in the promotion box, if any.
pub fn promotion_at(x: f32, y: f32) -> Option<Role> {
    let (ox, oy) = promo_origin();
    PROMO_ROLES.iter().enumerate().find_map(|(i, (role, _))| {
        let tx = ox + PROMO_PAD + i as f32 * (PROMO_TILE + PROMO_GAP);
        let ty = oy + 58.0;
        ((tx..=tx + PROMO_TILE).contains(&x) && (ty..=ty + PROMO_TILE).contains(&y))
            .then_some(*role)
    })
}

pub fn square_xy(s: Square, flip: bool) -> (f32, f32) {
    let f = u32::from(s.file()) as f32;
    let r = u32::from(s.rank()) as f32;
    if flip {
        (BX + (7.0 - f) * SQ, BY + r * SQ)
    } else {
        (BX + f * SQ, BY + (7.0 - r) * SQ)
    }
}

/// The square under a point in the window, if any.
pub fn square_at(x: f32, y: f32, flip: bool) -> Option<Square> {
    let (fx, ry) = ((x - BX) / SQ, (y - BY) / SQ);
    ((0.0..8.0).contains(&fx) && (0.0..8.0).contains(&ry)).then(|| {
        let (col, row) = (fx as u32, ry as u32);
        if flip {
            sq(7 - col, row)
        } else {
            sq(col, 7 - row)
        }
    })
}

/// Pieces image: King Queen Rook Bishop Knight Pawn across; white row, then black row.
pub fn piece_tile(role: Role, side: Side) -> (f32, f32) {
    let col = match role {
        Role::King => 0.0,
        Role::Queen => 1.0,
        Role::Rook => 2.0,
        Role::Bishop => 3.0,
        Role::Knight => 4.0,
        Role::Pawn => 5.0,
    };
    (col * 128.0, if side == Side::White { 0.0 } else { 128.0 })
}

pub fn frame(cv: &mut Canvas<'_>, g: &Game, v: &View) {
    let (th, txt) = (v.th, v.txt);
    // in a replay the board shows the replayed game instead of the live one
    let run = v.replay.and_then(|r| r.run.as_ref());
    let g = run.map_or(g, |r| &r.view);
    let flip = g.player == Side::Black;

    // The window is one flat colour; the header is a blended band across the top
    cv.clear(th.background);
    let (band0, band1) = (
        th.board_border,
        th.board_border.mix(Color::rgba8(0, 0, 0, 255), 0.72),
    );
    cv.vgrad(0.0, 0.0, W, HEADER_H, band0, band1);
    let on_band = band0.ink(th.background, th.text);
    cv.rect(BX, 22.0, 6.0, 36.0, th.accent);
    txt.draw(cv, "b30", "Chess", BX + 18.0, 51.0, on_band);
    // the status sits in a capsule: gold-accent when it is your move, quiet otherwise
    let (status, capsule) = match g.over() {
        _ if run.is_some() => (
            run.map_or(String::new(), |r| {
                if r.idx == 0 {
                    "Replay: start".to_string()
                } else {
                    format!(
                        "Replay: move {} of {}",
                        r.idx.div_ceil(2),
                        r.total().div_ceil(2)
                    )
                }
            }),
            band0.mix(on_band, 0.78),
        ),
        Some(_) => ("Game over".to_string(), band0.mix(on_band, 0.78)),
        None if g.two => (
            format!(
                "{} to move{}",
                if g.pos.turn() == Side::White {
                    "White"
                } else {
                    "Black"
                },
                if g.pos.is_check() { ". Check." } else { "" }
            ),
            if g.pos.is_check() {
                th.error
            } else {
                th.accent
            },
        ),
        None if g.pos.turn() == g.player => (
            if g.pos.is_check() {
                "Your move. You're in check."
            } else {
                "Your move"
            }
            .to_string(),
            if g.pos.is_check() {
                th.error
            } else {
                th.accent
            },
        ),
        None => (
            format!("AI is thinking{}", ".".repeat(v.thinking_dots)),
            band0.mix(on_band, 0.72),
        ),
    };
    let sw = txt.width("b18", &status);
    let capsule_ink = capsule.ink(th.background, th.text);
    cv.rrect(W - BX - sw - 28.0, 20.0, sw + 28.0, 36.0, 18.0, capsule);
    txt.draw(cv, "b18", &status, W - BX - sw - 14.0, 44.0, capsule_ink);

    // The way back to the home screen (not while a replay is using this part of the band)
    if run.is_none() {
        let (bx, by, bw, bh) = MENU_BTN;
        cv.rrect(bx, by, bw, bh, 15.0, band0.mix(on_band, 0.78));
        let ink = band0.mix(on_band, 0.78).ink(th.background, th.text);
        txt.draw(
            cv,
            "b18",
            "Menu",
            bx + (bw - txt.width("b18", "Menu")) / 2.0,
            by + 21.0,
            ink,
        );
    }

    // A replay whose positions are all scored: how each side played, the person first
    if let Some(sc) = run.and_then(|r| r.score) {
        let x = BX + 18.0 + txt.width("b30", "Chess") + 28.0;
        for side in [Side::White, Side::Black] {
            let s = sc.of(side);
            let mine = side == g.player;
            let who = match (g.two, side) {
                (true, Side::White) => "White",
                (true, Side::Black) => "Black",
                (false, _) if mine => "You",
                _ => "AI",
            };
            let line = format!(
                "{} {:.0}% accurate, about {}",
                who,
                s.accuracy,
                crate::score::estimate(s.acpl)
            );
            let row = if mine { 0.0 } else { 1.0 };
            txt.draw(cv, "r14", &line, x, 38.0 + 20.0 * row, on_band);
        }
    }

    // Where the selected piece can go (castling shows the king's square)
    let mut goes: Vec<Square> = vec![];
    if let Some(from) = g.selected {
        for m in g.pos.legal_moves() {
            if m.from() == Some(from) && !goes.contains(&shown_to(&m)) {
                goes.push(shown_to(&m));
            }
        }
    }

    // Board, lifted off the page by a soft shadow
    cv.shadow(BX - 4.0, BY - 4.0, SQ * 8.0 + 8.0, SQ * 8.0 + 8.0, 7);
    cv.rect(
        BX - 4.0,
        BY - 4.0,
        SQ * 8.0 + 8.0,
        SQ * 8.0 + 8.0,
        th.board_border,
    );
    for r in 0..8u32 {
        for f in 0..8u32 {
            let s = sq(f, r);
            let (x, y) = square_xy(s, flip);
            let light = (f + r) % 2 == 1;
            cv.rect(
                x,
                y,
                SQ,
                SQ,
                if light {
                    th.light_square
                } else {
                    th.dark_square
                },
            );
            // last move: where it came from (lighter) and where it landed (stronger)
            if let Some((a, b)) = g.last {
                if a == s {
                    cv.rect(x, y, SQ, SQ, th.last_from);
                }
                if b == s {
                    cv.rect(x, y, SQ, SQ, th.last_to);
                    cv.rect_lines(x + 1.5, y + 1.5, SQ - 3.0, SQ - 3.0, 3.0, th.last_outline);
                }
            }
            // half-typed square: tint the file or rank typed so far
            if v.kb_file == Some(f) || v.kb_rank == Some(r) {
                cv.rect(x, y, SQ, SQ, th.half_typed);
            }
            if g.selected == Some(s) {
                cv.rect(x, y, SQ, SQ, th.selected);
            }
            if g.target == Some(s) {
                cv.rect(x, y, SQ, SQ, th.target);
                cv.rect_lines(x + 2.0, y + 2.0, SQ - 4.0, SQ - 4.0, 5.0, th.target_outline);
            }
            // coordinates: dark ink on a light square, light on a dark one, so they read
            let lbl = if light {
                th.board_border
            } else {
                th.light_square
            };
            // rank numbers down the left edge, file letters along the bottom edge
            if f == if flip { 7 } else { 0 } {
                txt.draw(cv, "b11", &(r + 1).to_string(), x + 4.0, y + 14.0, lbl);
            }
            if r == if flip { 7 } else { 0 } {
                let file = ((b'a' + f as u8) as char).to_string();
                txt.draw(cv, "b11", &file, x + SQ - 11.0, y + SQ - 5.0, lbl);
            }
            // the piece that just moved is drawn last while it slides
            let sliding = v.slide.is_some() && g.last.map(|l| l.1) == Some(s);
            if let Some(p) = g.pos.board().piece_at(s).filter(|_| !sliding) {
                if p.role == Role::King && p.color == g.pos.turn() && g.pos.is_check() {
                    cv.rect(x, y, SQ, SQ, th.check);
                    cv.rect_lines(x + 2.0, y + 2.0, SQ - 4.0, SQ - 4.0, 4.0, th.check_outline);
                }
                let (tx, ty) = piece_tile(p.role, p.color);
                let k = v.tile / 128.0;
                cv.image(
                    v.pieces,
                    (tx * k, ty * k, v.tile, v.tile),
                    (x + 2.0, y + 2.0, SQ - 4.0, SQ - 4.0),
                    None,
                );
            }
            // a faded dot on an empty square it can go to, a ring round a piece it can take
            if goes.contains(&s) {
                let (cx, cy) = (x + SQ / 2.0, y + SQ / 2.0);
                if g.pos.board().piece_at(s).is_some() {
                    cv.round(cx, cy, SQ / 2.0 - 3.0, SQ / 2.0 - 9.0, th.move_dot);
                } else {
                    cv.round(cx, cy, 10.0, 0.0, th.move_dot);
                }
            }
        }
    }

    // The piece that has just moved glides from its old square to its new one
    if let (Some(t), Some((from, to))) = (v.slide, g.last) {
        if let Some(p) = g.pos.board().piece_at(to) {
            let e = 1.0 - (1.0 - t).powi(3); // ease out
            let ((fx, fy), (tx, ty)) = (square_xy(from, flip), square_xy(to, flip));
            let (x, y) = (fx + (tx - fx) * e, fy + (ty - fy) * e);
            let (sx, sy) = piece_tile(p.role, p.color);
            let k = v.tile / 128.0;
            cv.image(
                v.pieces,
                (sx * k, sy * k, v.tile, v.tile),
                (x + 2.0, y + 2.0, SQ - 4.0, SQ - 4.0),
                None,
            );
        }
    }

    // A pawn on its last step: ask which piece it becomes
    if g.promotion.is_some() {
        cv.rect(BX, BY, SQ * 8.0, SQ * 8.0, th.shade);
        let (ox, oy) = promo_origin();
        cv.shadow(ox, oy, PROMO_W, PROMO_H, 6);
        cv.rrect(ox, oy, PROMO_W, PROMO_H, 12.0, th.accent);
        cv.rrect(ox, oy + 6.0, PROMO_W, PROMO_H - 6.0, 12.0, th.banner);
        txt.draw(
            cv,
            "b18",
            "Become which piece?",
            ox + PROMO_PAD,
            oy + 40.0,
            th.banner_title,
        );
        for (i, (role, letter)) in PROMO_ROLES.iter().enumerate() {
            let tx = ox + PROMO_PAD + i as f32 * (PROMO_TILE + PROMO_GAP);
            let ty = oy + 58.0;
            cv.rect(
                tx,
                ty,
                PROMO_TILE,
                PROMO_TILE,
                if i % 2 == 0 {
                    th.light_square
                } else {
                    th.dark_square
                },
            );
            cv.rect_lines(tx, ty, PROMO_TILE, PROMO_TILE, 2.0, th.board_border);
            let (sx, sy) = piece_tile(*role, g.player);
            let k = v.tile / 128.0;
            cv.image(
                v.pieces,
                (sx * k, sy * k, v.tile, v.tile),
                (tx + 2.0, ty + 2.0, PROMO_TILE - 4.0, PROMO_TILE - 4.0),
                None,
            );
            let lw = txt.width("b11", letter);
            txt.draw(
                cv,
                "b11",
                letter,
                tx + (PROMO_TILE - lw) / 2.0,
                ty + PROMO_TILE + 20.0,
                th.muted,
            );
        }
    }

    // Threat arrows: from each attacking piece to each enemy piece it attacks
    if v.arrows {
        draw_threats(cv, g, v, flip);
    }

    // The engine's view in a replay: the bar, a mark on a weak move, the best move
    if let Some(r) = run {
        draw_engine(cv, g, v, r, flip);
    }

    // Game over: banner across the board
    if let Some(msg) = g.over().filter(|_| run.is_none()) {
        cv.rect(BX, BY, SQ * 8.0, SQ * 8.0, th.shade);
        let (bw, bh) = (380.0, 150.0);
        let (bx, by) = (BX + (SQ * 8.0 - bw) / 2.0, BY + (SQ * 8.0 - bh) / 2.0);
        cv.shadow(bx, by, bw, bh, 6);
        cv.rrect(bx, by, bw, bh, 12.0, th.accent);
        cv.rrect(bx, by + 6.0, bw, bh - 6.0, 12.0, th.banner);
        let (title, sub) = msg.split_once(". ").unwrap_or((&msg, ""));
        let center = |style: &str, t: &str| bx + (bw - txt.width(style, t)) / 2.0;
        txt.draw(
            cv,
            "b30",
            title,
            center("b30", title),
            by + 58.0,
            th.banner_title,
        );
        txt.draw(
            cv,
            "b18",
            sub,
            center("b18", sub),
            by + 92.0,
            th.banner_result,
        );
        let again = format!("Press {} for a new game", v.keys.label_of(Action::New));
        let again = again.as_str();
        txt.draw(cv, "r14", again, center("r14", again), by + 126.0, th.muted);
    }

    // Footer: the last few moves, any note about the AI move, the pack name after T
    let fy = BY + SQ * 8.0 + 34.0;
    let moves = if g.sans.is_empty() {
        "No moves yet.".to_string()
    } else {
        let all = move_list(&g.sans);
        let parts: Vec<&str> = all.split("  ").collect();
        let keep = if run.is_some_and(|r| r.analysing()) {
            4
        } else {
            5
        };
        parts[parts.len().saturating_sub(keep)..].join("    ")
    };
    txt.draw(cv, "r16", &moves, BX, fy, th.text);
    if let Some(r) = run {
        // what the engine made of the move just played, else the date of the game
        let verdict = match (r.idx, r.grade_at(r.idx), g.sans.last()) {
            (1.., Some((grade, l)), Some(san)) if grade != crate::engine::Grade::Fine => {
                format!(
                    "{san}: {} (cost {:.1} pawns).",
                    grade.word(),
                    l as f32 / 100.0
                )
            }
            (1.., Some(_), Some(san)) => format!("{san}: fine."),
            _ => format!("{}.", r.title),
        };
        let keys = if !r.analysing() {
            "Left / Right step, Esc leaves.".to_string()
        } else if r.analysed() < r.evals.len() {
            format!("Rating {} of {}...", r.analysed(), r.evals.len())
        } else {
            "Up / Down: marked moves. Esc leaves.".to_string()
        };
        let help = format!("{verdict}  {keys}");
        txt.draw(cv, "r14", &help, BX, fy + 24.0, th.muted);
        if let Some(e) = r.eval_here() {
            let t = e.text();
            txt.draw(cv, "b18", &t, W - BX - txt.width("b18", &t), fy, th.text);
        }
    } else if !g.note.is_empty() {
        txt.draw(cv, "r14", &g.note, BX, fy + 24.0, th.error);
    } else if !g.hint.is_empty() {
        txt.draw(cv, "r14", &g.hint, BX, fy + 24.0, th.error);
    }
    if let Some(pk) = v
        .replay
        .and_then(|r| r.picker.as_ref().map(|p| (p, &r.error)))
    {
        draw_picker(
            cv,
            v,
            pk.0,
            pk.1.as_deref(),
            v.replay.is_some_and(|r| r.engine_found),
        );
    }
    if v.show_pack_name {
        txt.draw(
            cv,
            "r14",
            &th.name,
            W - BX - txt.width("r14", &th.name),
            fy,
            th.muted,
        );
    }
}

/// The evaluation bar beside the board, a mark on the last move when it was weak,
/// and an arrow for the engine's move in the position shown.
fn draw_engine(cv: &mut Canvas<'_>, g: &Game, v: &View, r: &crate::replay::Run, flip: bool) {
    let th = v.th;
    let Some(e) = r.eval_here() else { return };
    // the bar: white's share rises from white's side of the board
    let (bx, bw, by, bh) = (7.0, 12.0, BY, SQ * 8.0);
    let share = 1.0 / (1.0 + (-(e.cp as f32) / 300.0).exp()); // 0.5 when level, near 1 when winning
    let white_h = bh * share;
    cv.rect(bx, by, bw, bh, th.board_border);
    if flip {
        cv.rect(bx, by, bw, white_h, th.light_square);
    } else {
        cv.rect(bx, by + bh - white_h, bw, white_h, th.light_square);
    }
    cv.rect(bx, by + bh / 2.0 - 0.5, bw, 1.0, th.accent);
    // a weak move gets a small badge on the square it moved to
    if let (Some((grade, _)), Some((_, to))) = (r.grade_at(r.idx), g.last) {
        if grade != crate::engine::Grade::Fine {
            let colour = if grade == crate::engine::Grade::Inaccuracy {
                th.status_over
            } else {
                th.error
            };
            let (x, y) = square_xy(to, flip);
            let mark = grade.mark();
            let w = v.txt.width("b11", mark) + 10.0;
            cv.rrect(x + SQ - w - 2.0, y + 2.0, w, 18.0, 9.0, colour);
            v.txt.draw(
                cv,
                "b11",
                mark,
                x + SQ - w + 3.0,
                y + 15.0,
                colour.ink(th.light_square, th.board_border),
            );
        }
    }
    // the engine's move, as an arrow in the look's accent
    if let (Some((from, to)), None) = (e.best, g.over()) {
        let (fx, fy) = square_xy(from, flip);
        let (tx, ty) = square_xy(to, flip);
        let half = SQ / 2.0;
        cv.arrow(
            (fx + half, fy + half),
            (tx + half, ty + half),
            8.0,
            20.0,
            Color {
                a: 0.8,
                ..th.accent
            },
        );
    }
}

/// One arrow per attack: white's and black's in the pack's own colours, and a stronger,
/// thicker one where the target is undefended or worth more than the attacker.
fn draw_threats(cv: &mut Canvas<'_>, g: &Game, v: &View, flip: bool) {
    let th = v.th;
    for t in crate::threats::threats(&g.pos) {
        let side = if t.by == Side::White {
            th.status_your_move
        } else {
            th.status_over
        };
        let (c, w) = if t.strong {
            (
                Color {
                    a: 0.92,
                    ..side.mix(th.error, 0.45)
                },
                9.0,
            )
        } else {
            (Color { a: 0.55, ..side }, 6.0)
        };
        let (fx, fy) = square_xy(t.from, flip);
        let (tx, ty) = square_xy(t.to, flip);
        let half = SQ / 2.0;
        // start a little out from the attacker's centre and stop at the target's
        let (dx, dy) = (tx - fx, ty - fy);
        let n = dx.hypot(dy).max(1.0);
        let (ux, uy) = (dx / n, dy / n);
        cv.arrow(
            (fx + half + ux * 14.0, fy + half + uy * 14.0),
            (tx + half - ux * 8.0, ty + half - uy * 8.0),
            w,
            w * 2.4,
            c,
        );
    }
}

/// The list of saved games over the board: aligned columns, the chosen row in the accent.
fn draw_picker(
    cv: &mut Canvas<'_>,
    v: &View,
    pk: &crate::replay::Picker,
    error: Option<&str>,
    engine_found: bool,
) {
    let (th, txt) = (v.th, v.txt);
    cv.rect(BX, BY, SQ * 8.0, SQ * 8.0, th.shade);
    let (bw, row) = (SQ * 8.0 - 40.0, 30.0);
    let bh = 138.0 + row * crate::replay::ROWS as f32;
    let (bx, by) = (BX + 20.0, BY + (SQ * 8.0 - bh) / 2.0);
    cv.shadow(bx, by, bw, bh, 6);
    cv.rrect(bx, by, bw, bh, 12.0, th.accent);
    cv.rrect(bx, by + 6.0, bw, bh - 6.0, 12.0, th.banner);
    txt.draw(
        cv,
        "b18",
        "Saved games",
        bx + 20.0,
        by + 40.0,
        th.banner_title,
    );
    if let Some(r) = v.replay.map(|r| &r.ratings) {
        let t = if r.you.games == 0 {
            "No rated games yet".to_string()
        } else {
            format!(
                "You {:.0} (+/- {:.0})   AI {:.0} (+/- {:.0})",
                r.you.rating, r.you.rd, r.ai.rating, r.ai.rd
            )
        };
        let x = bx + bw - 20.0 - txt.width("r14", &t);
        txt.draw(cv, "r14", &t, x, by + 40.0, th.muted);
    }
    if pk.entries.is_empty() {
        let lines = [
            "No saved games yet.",
            "A game of five moves or more is saved when it ends,",
            "when you start a new game, and when you quit.",
        ];
        for (i, l) in lines.iter().enumerate() {
            txt.draw(
                cv,
                "r16",
                l,
                bx + 20.0,
                by + 80.0 + 26.0 * i as f32,
                th.banner_result,
            );
        }
    }
    let xs = [20.0, 170.0, 250.0, 370.0]; // the columns
    let ink = th.accent.ink(th.background, th.text);
    for (i, e) in pk
        .entries
        .iter()
        .enumerate()
        .skip(pk.top)
        .take(crate::replay::ROWS)
    {
        let y = by + 60.0 + row * (i - pk.top) as f32;
        let chosen = i == pk.sel;
        if chosen {
            cv.rrect(bx + 10.0, y, bw - 20.0, row - 2.0, 8.0, th.accent);
        }
        for (c, t) in e.cols.iter().enumerate() {
            let colour = if chosen { ink } else { th.banner_result };
            txt.draw(cv, "r16", t, bx + xs[c], y + 20.0, colour);
        }
    }
    if !engine_found {
        let lines = [
            "To rate the moves, install Stockfish (free). Put it on your PATH",
            "or in ~/.side-chess/engine/. Replays work without it.",
        ];
        for (i, l) in lines.iter().enumerate() {
            txt.draw(
                cv,
                "r14",
                l,
                bx + 20.0,
                by + bh - 62.0 + 18.0 * i as f32,
                th.muted,
            );
        }
    }
    let foot = error.unwrap_or("Up / Down choose, Enter opens, Esc closes.");
    let colour = if error.is_some() { th.error } else { th.muted };
    txt.draw(cv, "r14", foot, bx + 20.0, by + bh - 16.0, colour);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_lands_on_the_square_that_was_drawn_there_either_way_up() {
        for flip in [false, true] {
            for f in 0..8 {
                for r in 0..8 {
                    let s = sq(f, r);
                    let (x, y) = square_xy(s, flip);
                    assert_eq!(square_at(x + SQ / 2.0, y + SQ / 2.0, flip), Some(s));
                }
            }
        }
    }

    #[test]
    fn the_players_own_back_rank_is_at_the_bottom() {
        // white at the bottom normally; black at the bottom when flipped
        let (_, white_bottom) = square_xy(sq(4, 0), false);
        let (_, black_bottom) = square_xy(sq(4, 7), true);
        assert_eq!(white_bottom, black_bottom);
        assert!(white_bottom > BY + SQ * 7.0 - 1.0);
    }

    /// Draws the board window in every look to /tmp/look-<id>.png, to compare them.
    /// Run by hand: cargo test draw_every_look -- --ignored
    #[test]
    #[ignore]
    fn draw_every_look() {
        use crate::game::Game;
        use crate::pack::{load_pack, PACKS};
        use crate::rails::Rails;
        use crate::text::Txt;
        use shakmaty::{fen::Fen, CastlingMode, Chess};
        let txt = Txt::load();
        let rails = Rails::new();
        let names: Vec<String> = PACKS.iter().map(|p| p.0.to_string()).collect();
        let mut g = Game::new();
        g.pos = "r1bqk2r/pppp1ppp/2n2n2/2b1p3/2B1P3/3P1N2/PPP2PPP/RNBQK2R w KQkq - 0 5"
            .parse::<Fen>()
            .unwrap()
            .into_position::<Chess>(CastlingMode::Standard)
            .unwrap();
        g.last = Some(("d2".parse().unwrap(), "d3".parse().unwrap()));
        for (i, (id, _, _)) in PACKS.iter().enumerate() {
            let (th, pieces) = load_pack(i);
            let (w, h) = ((W * 2.0) as usize, (H * 2.0) as usize);
            let mut px = vec![0u32; w * h];
            let v = View {
                th: &th,
                pieces: &pieces,
                tile: 128.0,
                txt: &txt,
                kb_file: None,
                kb_rank: None,
                thinking_dots: 1,
                show_pack_name: false,
                rails: &rails,
                pack: i,
                pack_names: &names,
                piece_names: &names,
                pieces_row: 0,
                slide: None,
                replay: None,
                keys: &crate::keys::Keymap::defaults(),
                arrows: false,
            };
            let mut cv = Canvas::new(&mut px, w, h, 2.0);
            frame(&mut cv, &g, &v);
            cv.save_png(std::path::Path::new(&format!("/tmp/look-{id}.png")), None)
                .unwrap();
        }
    }

    #[test]
    fn each_piece_in_the_promotion_box_is_picked_by_where_you_point() {
        let (ox, oy) = promo_origin();
        for (i, (role, _)) in PROMO_ROLES.iter().enumerate() {
            let x = ox + PROMO_PAD + i as f32 * (PROMO_TILE + PROMO_GAP) + PROMO_TILE / 2.0;
            assert_eq!(promotion_at(x, oy + 58.0 + PROMO_TILE / 2.0), Some(*role));
        }
        // between tiles, above them, and outside the box: nothing
        let gap_x = ox + PROMO_PAD + PROMO_TILE + PROMO_GAP / 2.0;
        assert_eq!(promotion_at(gap_x, oy + 90.0), None);
        assert_eq!(promotion_at(ox + 40.0, oy + 10.0), None);
        assert_eq!(promotion_at(5.0, 5.0), None);
    }

    /// Draws a replay (scored with made-up numbers) and the saved-games list to PNGs in
    /// $SIDE_CHESS_PNG_DIR, with no window. Run by hand:
    /// SIDE_CHESS_PNG_DIR=/tmp cargo test draw_replay_sheet -- --ignored
    #[test]
    #[ignore]
    fn draw_replay_sheet() {
        use crate::engine::Eval;
        use crate::pack::{load_pack, PACKS};
        use crate::rails::Rails;
        use crate::replay::{parse_pgn, Entry, Picker, Run, State};
        use crate::text::Txt;
        let dir = std::env::var("SIDE_CHESS_PNG_DIR").unwrap_or("/tmp".into());
        let txt = Txt::load();
        let rails = Rails::new();
        let names: Vec<String> = PACKS.iter().map(|p| p.0.to_string()).collect();
        let (th, pieces) = load_pack(0);
        let p =
            parse_pgn("1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. b4 Bxb4 5. c3 Ba5 6. d4 exd4 *").unwrap();
        let mut run = Run::new(&p, "2026-11-23 09:10".into());
        let n = run.total() + 1;
        run.evals = (0..n)
            .map(|i| {
                Some(Eval {
                    cp: [20, 25, 20, 30, 35, 30, 40, -90, -80, -85, -60, 150][i.min(11)],
                    mate: None,
                    best: Some(("g1".parse().unwrap(), "f3".parse().unwrap())),
                })
            })
            .collect();
        run.score = Some(crate::score::Score::from_moves(&[
            (20, 15),
            (25, 20),
            (20, 10),
            (-30, -40),
            (35, 20),
            (-30, 100),
            (40, -90),
            (80, 70),
            (90, 60),
            (60, 50),
            (-150, -350),
        ]));
        run.goto(8);
        let mut st = State::default();
        st.ratings.you.games = 3;
        st.ratings.you.rating = 1523.0;
        st.ratings.you.rd = 180.0;
        st.run = Some(run);
        let mut picker_st = State::default();
        picker_st.ratings = st.ratings.clone();
        picker_st.picker = Some(Picker {
            entries: vec![Entry {
                path: "x.pgn".into(),
                cols: [
                    "2026-11-23 09:10".into(),
                    "White".into(),
                    "Won".into(),
                    "36 moves".into(),
                ],
            }],
            sel: 0,
            top: 0,
        });
        let g = Game::new();
        for (name, state) in [("replay", &st), ("picker", &picker_st)] {
            let (w, h) = ((W * 2.0) as usize, (H * 2.0) as usize);
            let mut px = vec![0u32; w * h];
            let v = View {
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
                piece_names: &names,
                pieces_row: 0,
                slide: None,
                arrows: false,
                replay: Some(state),
                keys: &crate::keys::Keymap::defaults(),
            };
            let mut cv = Canvas::new(&mut px, w, h, 2.0);
            frame(&mut cv, &g, &v);
            cv.save_png(std::path::Path::new(&format!("{dir}/{name}.png")), None)
                .unwrap();
        }
    }
}
