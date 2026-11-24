//! Saving games: every game of at least a few moves is written as a PGN file to
//! ~/.side-chess/games/ (never into the repo), so it can be replayed later.
use crate::game::{live_dir, Game};
use shakmaty::{
    fen::Fen,
    san::{San, SanPlus},
    Chess, Color as Side, EnPassantMode, Position,
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

/// A game shorter than this many moves (both sides counted) is not kept: pressing N
/// in the first moves is a restart, not a game.
pub const MIN_MOVES: usize = 5;

pub fn games_dir() -> PathBuf {
    live_dir().join("games")
}

/// Year, month, day, hour, minute, second (UTC) for a Unix time.
pub fn civil(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        y,
        m,
        d,
        (rem / 3600) as u32,
        (rem % 3600 / 60) as u32,
        (rem % 60) as u32,
    )
}

/// The PGN result: 1-0, 0-1, 1/2-1/2, or * for a game not finished.
pub fn result_text(pos: &Chess) -> &'static str {
    if pos.is_checkmate() {
        if pos.turn() == Side::White {
            "0-1"
        } else {
            "1-0"
        }
    } else if pos.is_stalemate() || pos.is_insufficient_material() {
        "1/2-1/2"
    } else {
        "*"
    }
}

/// The game as PGN text. `secs` is the Unix time it is dated with (UTC).
pub fn pgn_text(g: &Game, secs: u64) -> String {
    let (y, mo, d, ..) = civil(secs);
    let (you, ai) = ("You", "AI");
    let stockfish = g.engine.map(|elo| format!("Stockfish {elo}"));
    let ai = stockfish.as_deref().unwrap_or(ai);
    let (white, black) = if g.two {
        (
            crate::replay::IN_PERSON_WHITE,
            crate::replay::IN_PERSON_BLACK,
        )
    } else if g.player == Side::White {
        (you, ai)
    } else {
        (ai, you)
    };
    let start = g.history.first().map_or(&g.pos, |(p, _)| p);
    let mut out = format!(
        "[Event \"Side Chess\"]\n[Date \"{y:04}.{mo:02}.{d:02}\"]\n[White \"{white}\"]\n[Black \"{black}\"]\n[Result \"{}\"]\n",
        result_text(&g.pos)
    );
    if *start != Chess::default() {
        let fen = Fen::from_position(start.clone(), EnPassantMode::Legal);
        out.push_str(&format!("[SetUp \"1\"]\n[FEN \"{fen}\"]\n"));
    }
    out.push('\n');
    let mut number = start.fullmoves().get();
    let mut turn = start.turn();
    let mut words: Vec<String> = vec![];
    let mut walk = start.clone();
    for (i, san) in g.sans.iter().enumerate() {
        // the game keeps plain SAN; PGN wants the + and # as well
        let san = match san.parse::<San>().ok().and_then(|s| s.to_move(&walk).ok()) {
            Some(m) => SanPlus::from_move_and_play_unchecked(&mut walk, &m).to_string(),
            None => san.clone(),
        };
        if turn == Side::White {
            words.push(format!("{number}. {san}"));
        } else if i == 0 {
            words.push(format!("{number}... {san}"));
        } else {
            words.push(san);
        }
        if turn == Side::Black {
            number += 1;
        }
        turn = turn.other();
    }
    // lines of at most about 80 characters, as PGN expects
    let mut line = String::new();
    for w in words.into_iter().chain([result_text(&g.pos).to_string()]) {
        if !line.is_empty() && line.len() + 1 + w.len() > 80 {
            out.push_str(&line);
            out.push('\n');
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&w);
    }
    out.push_str(&line);
    out.push('\n');
    out
}

/// Write the game if it has enough moves and has changed since the last save.
/// The same game keeps one file, so quitting after a game ended adds nothing.
pub fn save(g: &mut Game) -> Option<PathBuf> {
    save_in(g, &games_dir())
}

fn save_in(g: &mut Game, dir: &std::path::Path) -> Option<PathBuf> {
    if g.sans.len() < MIN_MOVES {
        return None;
    }
    if let Some((path, n)) = &g.saved {
        if *n == g.sans.len() {
            return Some(path.clone());
        }
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    fs::create_dir_all(dir).ok()?;
    let path = match &g.saved {
        Some((p, _)) => p.clone(),
        None => {
            let (y, mo, d, h, mi, s) = civil(now);
            let stem = format!("{y:04}-{mo:02}-{d:02}_{h:02}-{mi:02}-{s:02}");
            let mut path = dir.join(format!("{stem}.pgn"));
            let mut n = 2;
            while path.exists() {
                path = dir.join(format!("{stem}-{n}.pgn"));
                n += 1;
            }
            path
        }
    };
    fs::write(&path, pgn_text(g, now)).ok()?;
    g.saved = Some((path.clone(), g.sans.len()));
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::parse_move;

    fn play(g: &mut Game, moves: &[&str]) {
        for m in moves {
            let mv = parse_move(&g.pos, m).expect(m);
            g.play(&mv);
        }
    }

    #[test]
    fn dates_are_right() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        // 2024-02-29 12:34:56 UTC
        assert_eq!(civil(1_709_210_096), (2024, 2, 29, 12, 34, 56));
    }

    #[test]
    fn pgn_has_headers_moves_and_result() {
        let mut g = Game::new();
        play(&mut g, &["f3", "e5", "g4", "Qh4#"]);
        let t = pgn_text(&g, 1_709_210_096);
        assert!(t.contains("[Event \"Side Chess\"]"));
        assert!(t.contains("[Date \"2024.02.29\"]"));
        assert!(t.contains("[White \"You\"]") && t.contains("[Black \"AI\"]"));
        assert!(t.contains("[Result \"0-1\"]"));
        assert!(t.contains("1. f3 e5 2. g4 Qh4# 0-1"), "{t}");
        assert!(!t.contains("FEN"));
    }

    #[test]
    fn black_player_is_named_black() {
        let mut g = Game::new_as(Side::Black);
        play(&mut g, &["e4", "e5"]);
        let t = pgn_text(&g, 0);
        assert!(t.contains("[White \"AI\"]") && t.contains("[Black \"You\"]"));
        assert!(t.contains("[Result \"*\"]"));
    }

    #[test]
    fn a_custom_start_is_recorded() {
        let mut g = Game::new();
        g.pos = "4k3/8/8/8/8/8/4P3/4K3 b - - 0 7"
            .parse::<Fen>()
            .unwrap()
            .into_position(shakmaty::CastlingMode::Standard)
            .unwrap();
        play(&mut g, &["Kd7", "e4"]);
        let t = pgn_text(&g, 0);
        assert!(
            t.contains("[SetUp \"1\"]") && t.contains("[FEN \"4k3/8/8/8/8/8/4P3/4K3 b - - 0 7\"]")
        );
        assert!(t.contains("7... Kd7 8. e4 *"), "{t}");
    }

    #[test]
    fn short_games_are_not_saved_and_one_game_keeps_one_file() {
        let dir = std::env::temp_dir().join(format!("side-chess-save-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let games = dir.join("games");
        let mut g = Game::new();
        play(&mut g, &["e4", "e5", "Nf3", "Nc6"]);
        assert!(save_in(&mut g, &games).is_none(), "4 moves is not enough");
        play(&mut g, &["Bb5"]);
        let first = save_in(&mut g, &games).expect("5 moves is saved");
        assert!(first.starts_with(&games));
        assert_eq!(
            save_in(&mut g, &games),
            Some(first.clone()),
            "no second file"
        );
        play(&mut g, &["a6"]);
        assert_eq!(
            save_in(&mut g, &games),
            Some(first.clone()),
            "the same file is updated"
        );
        assert!(fs::read_to_string(&first).unwrap().contains("3. Bb5 a6"));
        assert_eq!(fs::read_dir(&games).unwrap().count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
