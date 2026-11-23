//! Threats: every piece that attacks an enemy piece, worked out from shakmaty's
//! attack tables. Used to draw the arrows toggled with X.
use shakmaty::{Chess, Color as Side, Position, Role, Square};

pub struct Threat {
    pub from: Square,
    pub to: Square,
    pub by: Side,     // the side doing the attacking
    pub strong: bool, // the target has no defender, or is worth more than the attacker
}

/// What a piece is worth in pawns; the king is worth more than any capture could win,
/// so a check always counts as a strong threat.
pub fn value(role: Role) -> u32 {
    match role {
        Role::Pawn => 1,
        Role::Knight | Role::Bishop => 3,
        Role::Rook => 5,
        Role::Queen => 9,
        Role::King => 100,
    }
}

/// All attacks in the position, both sides, weaker ones first (so strong arrows draw on top).
pub fn threats(pos: &Chess) -> Vec<Threat> {
    let board = pos.board();
    let occupied = board.occupied();
    let mut out = vec![];
    for from in occupied {
        let Some(piece) = board.piece_at(from) else {
            continue;
        };
        for to in board.attacks_from(from) & board.by_color(piece.color.other()) {
            let Some(target) = board.piece_at(to) else {
                continue;
            };
            let defended = board.attacks_to(to, target.color, occupied).any();
            out.push(Threat {
                from,
                to,
                by: piece.color,
                strong: !defended || value(target.role) > value(piece.role),
            });
        }
    }
    out.sort_by_key(|t| t.strong);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::{fen::Fen, CastlingMode};

    fn pos(fen: &str) -> Chess {
        fen.parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap()
    }

    fn find(ts: &[Threat], from: &str, to: &str) -> Option<bool> {
        let (f, t): (Square, Square) = (from.parse().unwrap(), to.parse().unwrap());
        ts.iter()
            .find(|x| x.from == f && x.to == t)
            .map(|x| x.strong)
    }

    #[test]
    fn the_start_position_has_no_threats() {
        assert!(threats(&Chess::default()).is_empty());
    }

    #[test]
    fn an_undefended_piece_is_a_strong_threat() {
        // white bishop c4 hits the black pawn f7, which the king guards: not strong;
        // the rook on a1 hits a loose black knight on a5: strong
        let p = pos("4k3/5p2/8/n7/2B5/8/8/R3K3 w - - 0 1");
        let ts = threats(&p);
        assert_eq!(find(&ts, "c4", "f7"), Some(false));
        assert_eq!(find(&ts, "a1", "a5"), Some(true));
        // the loose knight on a5 attacks the bishop on c4, which nothing guards
        assert_eq!(find(&ts, "a5", "c4"), Some(true));
    }

    #[test]
    fn winning_more_than_the_attacker_is_strong_even_when_defended() {
        // black pawn d5 attacks the white knight c4; the knight attacks the pawn on e5
        // (guarded by d6, so not strong) and the pawn on d6 (nothing guards it)
        let p = pos("4k3/8/3p4/3pp3/2N5/8/8/4K3 w - - 0 1");
        let ts = threats(&p);
        assert_eq!(find(&ts, "d5", "c4"), Some(true)); // a pawn takes a knight
        assert_eq!(find(&ts, "c4", "e5"), Some(false));
        assert_eq!(find(&ts, "c4", "d6"), Some(true));
        assert!(ts.iter().any(|t| t.by == Side::Black));
    }

    #[test]
    fn a_check_is_strong_and_strong_ones_come_last() {
        let p = pos("4k3/8/8/8/8/8/4R3/4K3 b - - 0 1");
        let ts = threats(&p);
        assert_eq!(find(&ts, "e2", "e8"), Some(true));
        assert!(!ts.first().map_or(false, |t| t.strong) || ts.iter().all(|t| t.strong));
        let first_strong = ts.iter().position(|t| t.strong).unwrap_or(ts.len());
        assert!(ts[first_strong..].iter().all(|t| t.strong));
    }
}
