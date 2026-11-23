//! How well each side played a game, from Stockfish's scores: the average
//! centipawn loss per move, an accuracy percentage and a rough rating estimate.
//! The result is kept in a small file beside the game's PGN.
use shakmaty::Color as Side;
use std::{fs, path::Path};

/// How one side played.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SideScore {
    pub moves: usize,
    pub acpl: f32,     // average centipawn loss per move (0 is perfect)
    pub accuracy: f32, // average accuracy of its moves, percent
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Score {
    pub white: SideScore,
    pub black: SideScore,
}

/// The chance of winning, in percent, for a score in centipawns (lichess's curve).
pub fn win_percent(cp: i32) -> f32 {
    50.0 + 50.0 * (2.0 / (1.0 + (-0.003_682_08 * cp as f32).exp()) - 1.0)
}

/// Accuracy of one move, 100 for no loss, from the win chance before and after it
/// (both from the mover's side). This is lichess's formula.
pub fn move_accuracy(before_cp: i32, after_cp: i32) -> f32 {
    let drop = (win_percent(before_cp) - win_percent(after_cp)).max(0.0);
    (103.1668 * (-0.04354 * drop).exp() - 3.1669).clamp(0.0, 100.0)
}

impl Score {
    /// `moves` are each move's score before and after it, in centipawns from the
    /// mover's side, in game order (white's first move first).
    pub fn from_moves(moves: &[(i32, i32)]) -> Score {
        let side = |first: usize| {
            let m: Vec<&(i32, i32)> = moves.iter().skip(first).step_by(2).collect();
            let n = m.len().max(1) as f32;
            SideScore {
                moves: m.len(),
                acpl: m.iter().map(|(b, a)| (b - a).max(0)).sum::<i32>() as f32 / n,
                accuracy: if m.is_empty() {
                    100.0
                } else {
                    m.iter().map(|(b, a)| move_accuracy(*b, *a)).sum::<f32>() / n
                },
            }
        };
        Score {
            white: side(0),
            black: side(1),
        }
    }

    pub fn of(&self, side: Side) -> SideScore {
        if side == Side::White {
            self.white
        } else {
            self.black
        }
    }
}

/// A rough rating for that level of play: 3100 * e^(-0.01 * ACPL), a rule of thumb
/// from studies of engine-scored games. It is a guide, not a measurement: one game
/// is a small sample, and the figure is relative to Stockfish's own scale.
pub fn estimate(acpl: f32) -> u32 {
    (3100.0 * (-0.01 * acpl).exp()).round() as u32
}

/// The file kept beside a game: `<game>.pgn` gets `<game>.score`.
fn score_path(pgn: &Path) -> std::path::PathBuf {
    pgn.with_extension("score")
}

pub fn save(pgn: &Path, s: &Score) {
    let text = format!(
        "white_moves={}\nwhite_acpl={:.1}\nwhite_accuracy={:.1}\nblack_moves={}\nblack_acpl={:.1}\nblack_accuracy={:.1}\n",
        s.white.moves, s.white.acpl, s.white.accuracy, s.black.moves, s.black.acpl, s.black.accuracy
    );
    let _ = fs::write(score_path(pgn), text);
}

pub fn load(pgn: &Path) -> Option<Score> {
    let text = fs::read_to_string(score_path(pgn)).ok()?;
    let get = |key: &str| -> Option<f32> {
        text.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('=')?.trim().parse().ok())
    };
    Some(Score {
        white: SideScore {
            moves: get("white_moves")? as usize,
            acpl: get("white_acpl")?,
            accuracy: get("white_accuracy")?,
        },
        black: SideScore {
            moves: get("black_moves")? as usize,
            acpl: get("black_acpl")?,
            accuracy: get("black_accuracy")?,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_side_gets_its_own_average() {
        // (before, after) from the mover's side: white loses 10, 30, 50; black 0, 100
        let s = Score::from_moves(&[(0, -10), (0, 0), (0, -30), (50, -50), (0, -50)]);
        assert_eq!((s.white.moves, s.white.acpl), (3, 30.0));
        assert_eq!((s.black.moves, s.black.acpl), (2, 50.0));
        assert_eq!(s.of(Side::Black).acpl, 50.0);
        assert_eq!(Score::from_moves(&[]).white.moves, 0);
    }

    #[test]
    fn a_perfect_move_is_full_marks_and_a_blunder_is_not() {
        assert!(move_accuracy(30, 30) > 99.9);
        assert_eq!(
            move_accuracy(30, 80),
            move_accuracy(30, 30),
            "a gain is no loss"
        );
        assert!(move_accuracy(30, -300) < 30.0, "a three pawn drop");
        assert!(move_accuracy(30, 0) > 85.0, "a small slip costs little");
        assert!((win_percent(0) - 50.0).abs() < 0.01);
        assert!(win_percent(300) > 70.0 && win_percent(-300) < 30.0);
    }

    #[test]
    fn the_estimate_falls_as_the_loss_grows() {
        assert_eq!(estimate(0.0), 3100);
        assert!(estimate(25.0) > estimate(60.0));
        assert!((1400..1700).contains(&estimate(63.0))); // 3100 * e^-0.63 = 1651
    }

    #[test]
    fn the_score_file_reads_back() {
        let dir = std::env::temp_dir().join(format!("side-chess-score-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let pgn = dir.join("g.pgn");
        let s = Score::from_moves(&[(0, -10), (0, 0), (0, -30), (50, -50), (0, -50)]);
        save(&pgn, &s);
        let back = load(&pgn).unwrap();
        assert_eq!(back.white.moves, 3);
        assert!((back.black.accuracy - s.black.accuracy).abs() < 0.06);
        assert_eq!(load(&dir.join("none.pgn")), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
