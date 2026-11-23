//! What the window shows. Every colour comes from the pack (`th`), every size
//! from the layout constants below, so a new look never needs code changes.
use crate::canvas::{Canvas, Img};
use crate::game::{game_over, move_list, shown_to, sq, Game};
use crate::pack::Theme;
use crate::text::Txt;
use shakmaty::{Color as Side, Position, Role, Square};

// Layout, in points
pub const SQ: f32 = 68.0; // one square
pub const BX: f32 = 28.0; // board left
pub const BY: f32 = 96.0; // board top
pub const W: f32 = BX * 2.0 + SQ * 8.0;
pub const H: f32 = BY + SQ * 8.0 + 76.0;

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
fn piece_tile(role: Role, side: Side) -> (f32, f32) {
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

    // Header
    cv.clear(th.background);
    cv.rect(BX, 30.0, 6.0, 40.0, th.accent);
    txt.draw(cv, "b30", "Chess", BX + 18.0, 58.0, th.title);
    let (status, col) = match game_over(&g.pos, g.player) {
        Some(_) => ("Game over".to_string(), th.status_over),
        None if g.pos.turn() == g.player => (
            if g.pos.is_check() {
                "Your move. You're in check."
            } else {
                "Your move"
            }
            .to_string(),
            th.status_your_move,
        ),
        None => (
            format!("AI is thinking{}", ".".repeat(v.thinking_dots)),
            th.status_waiting,
        ),
    };
    txt.draw(
        cv,
        "b18",
        &status,
        W - BX - txt.width("b18", &status),
        56.0,
        col,
    );

    // Where the selected piece can go (castling shows the king's square)
    let mut goes: Vec<Square> = vec![];
    if let Some(from) = g.selected {
        for m in g.pos.legal_moves() {
            if m.from() == Some(from) && !goes.contains(&shown_to(&m)) {
                goes.push(shown_to(&m));
            }
        }
    }

    // Board
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
            let lbl = if light {
                th.dark_square
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
            if let Some(p) = g.pos.board().piece_at(s) {
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

    // Game over: banner across the board
    if let Some(msg) = game_over(&g.pos, g.player) {
        cv.rect(BX, BY, SQ * 8.0, SQ * 8.0, th.shade);
        let (bw, bh) = (380.0, 150.0);
        let (bx, by) = (BX + (SQ * 8.0 - bw) / 2.0, BY + (SQ * 8.0 - bh) / 2.0);
        cv.rect(bx, by, bw, bh, th.banner);
        cv.rect(bx, by, bw, 6.0, th.accent);
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
    let fy = BY + SQ * 8.0 + 36.0;
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
}
