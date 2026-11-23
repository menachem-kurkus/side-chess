//! What the window shows. Every colour comes from the pack (`th`), every size
//! from the layout constants below, so a new look never needs code changes.
use crate::canvas::{Canvas, Color, Img};
use crate::game::{game_over, move_list, shown_to, sq, Game};
use crate::pack::Theme;
use crate::rails::Rails;
use crate::text::Txt;
use shakmaty::{Color as Side, Position, Role, Square};

// Layout, in points
pub const SQ: f32 = 68.0; // one square
pub const BX: f32 = 28.0; // board left
pub const BY: f32 = 96.0; // board top
pub const W: f32 = BX * 2.0 + SQ * 8.0;
pub const H: f32 = BY + SQ * 8.0 + 52.0;
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
    pub slide: Option<f32>, // 0..1 while the last move's piece is sliding into place
}

/// Where a square is drawn. The player's own pieces are at the bottom, so the
/// board is turned round when the player has black (`flip`).
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
    let (status, capsule) = match game_over(&g.pos, g.player) {
        Some(_) => ("Game over".to_string(), band0.mix(on_band, 0.78)),
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

    // Game over: banner across the board
    if let Some(msg) = game_over(&g.pos, g.player) {
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
        let again = "Press N for a new game";
        txt.draw(cv, "r14", again, center("r14", again), by + 126.0, th.muted);
    }

    // Footer: the last few moves, any note about the AI move, the pack name after T
    let fy = BY + SQ * 8.0 + 34.0;
    let moves = if g.sans.is_empty() {
        "No moves yet.".to_string()
    } else {
        let all = move_list(&g.sans);
        let parts: Vec<&str> = all.split("  ").collect();
        parts[parts.len().saturating_sub(5)..].join("    ")
    };
    txt.draw(cv, "r16", &moves, BX, fy, th.text);
    if !g.note.is_empty() {
        txt.draw(cv, "r14", &g.note, BX, fy + 26.0, th.error);
    } else if !g.hint.is_empty() {
        txt.draw(cv, "r14", &g.hint, BX, fy + 26.0, th.error);
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
                slide: None,
            };
            let mut cv = Canvas::new(&mut px, w, h, 2.0);
            frame(&mut cv, &g, &v);
            cv.save_png(std::path::Path::new(&format!("/tmp/look-{id}.png")), None)
                .unwrap();
        }
    }
}
