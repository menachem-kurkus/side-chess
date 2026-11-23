//! Replaying saved games: reading a PGN, listing the games in ~/.side-chess/games,
//! and stepping through one. A replay has its own `Game` for the board to show; it
//! never touches the live game or the files shared with the AI (state.txt).
use crate::game::{shown_to, Game};
use crate::save::games_dir;
use shakmaty::{fen::Fen, san::San, CastlingMode, Chess, Color as Side, Move, Position, Square};
use std::{fs, path::PathBuf, str::FromStr};

/// A game read from PGN text.
pub struct Parsed {
    pub black: String,
    pub result: String,
    pub start: Chess,
    pub moves: Vec<Move>,
}

impl Parsed {
    /// The side the person played: "You" is theirs; any other game is shown from white.
    pub fn player(&self) -> Side {
        if self.black == "You" {
            Side::Black
        } else {
            Side::White
        }
    }
}

/// Read PGN text: the headers we use, and the main line of moves. Comments,
/// variations, numeric annotations and move numbers are skipped. An illegal or
/// unreadable move is an error naming it (a half game would mislead).
pub fn parse_pgn(text: &str) -> Result<Parsed, String> {
    let (mut black, mut result) = ("?".to_string(), "*".to_string());
    let mut start = Chess::default();
    let mut body = String::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(tag) = t.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
            let (name, rest) = tag.split_once(' ').unwrap_or((tag, ""));
            let value = rest.trim().trim_matches('"').to_string();
            match name {
                "Black" => black = value,
                "Result" => result = value,
                "FEN" => {
                    start = value
                        .parse::<Fen>()
                        .map_err(|e| format!("bad FEN: {e}"))?
                        .into_position(CastlingMode::Standard)
                        .map_err(|e| format!("bad FEN: {e}"))?;
                }
                _ => {}
            }
        } else if !t.starts_with('%') {
            body.push_str(line);
            body.push('\n');
        }
    }
    // take out {comments}, ;rest-of-line comments and (variations, which may nest)
    let mut clean = String::new();
    let (mut brace, mut paren, mut semi) = (false, 0u32, false);
    for c in body.chars() {
        match c {
            '\n' => {
                semi = false;
                clean.push(' ');
            }
            _ if semi => {}
            '{' if paren == 0 => brace = true,
            '}' => brace = false,
            _ if brace => {}
            '(' => paren += 1,
            ')' => paren = paren.saturating_sub(1),
            _ if paren > 0 => {}
            ';' => semi = true,
            c => clean.push(c),
        }
    }
    let mut pos = start.clone();
    let mut moves = vec![];
    for tok in clean.split_whitespace() {
        if matches!(tok, "1-0" | "0-1" | "1/2-1/2" | "*") {
            if result == "*" {
                result = tok.to_string();
            }
            break;
        }
        if tok.starts_with('$') {
            continue;
        }
        // "12." "12..." and "12.e4" all carry a move number in front
        let digits = tok.chars().take_while(|c| c.is_ascii_digit()).count();
        let mut san = tok;
        if digits > 0 && tok[digits..].starts_with('.') {
            san = tok[digits..].trim_start_matches('.');
        }
        let san = san.trim_end_matches(['!', '?']);
        if san.is_empty() {
            continue;
        }
        let m = San::from_str(san)
            .ok()
            .and_then(|s| s.to_move(&pos).ok())
            .ok_or_else(|| format!("move {} ({san}) is not legal", moves.len() + 1))?;
        pos.play_unchecked(&m);
        moves.push(m);
    }
    Ok(Parsed {
        black,
        result,
        start,
        moves,
    })
}

/// A game being replayed, with every position worked out once when it is opened.
pub struct Run {
    pub view: Game, // what the board shows: the position at `idx`
    positions: Vec<Chess>,
    lasts: Vec<Option<(Square, Square)>>,
    sans: Vec<String>,
    pub idx: usize,
    pub title: String,
}

impl Run {
    pub fn new(p: &Parsed, title: String) -> Run {
        let mut positions = vec![p.start.clone()];
        let mut lasts = vec![None];
        let mut sans = vec![];
        let mut pos = p.start.clone();
        for m in &p.moves {
            sans.push(San::from_move(&pos, m).to_string());
            lasts.push(Some((m.from().unwrap_or(m.to()), shown_to(m))));
            pos.play_unchecked(m);
            positions.push(pos.clone());
        }
        let mut run = Run {
            view: Game::new_as(p.player()),
            positions,
            lasts,
            sans,
            idx: 0,
            title,
        };
        run.goto(0);
        run
    }

    pub fn total(&self) -> usize {
        self.sans.len()
    }

    /// Show the position after `idx` moves (0 is the start).
    pub fn goto(&mut self, idx: usize) {
        self.idx = idx.min(self.total());
        self.view.pos = self.positions[self.idx].clone();
        self.view.last = self.lasts[self.idx];
        self.view.sans = self.sans[..self.idx].to_vec();
    }

    pub fn step(&mut self, delta: isize) {
        self.goto(self.idx.saturating_add_signed(delta));
    }
}

/// One line of the list of saved games, as columns so the list lines up.
pub struct Entry {
    pub path: PathBuf,
    pub cols: [String; 4], // when, who you were, how it ended, length
}

/// The saved games, newest first.
pub fn list_games() -> Vec<Entry> {
    let mut paths: Vec<PathBuf> = fs::read_dir(games_dir())
        .map(|d| {
            d.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "pgn"))
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths.reverse();
    paths.truncate(200);
    paths.into_iter().filter_map(entry).collect()
}

fn entry(path: PathBuf) -> Option<Entry> {
    let p = parse_pgn(&fs::read_to_string(&path).ok()?).ok()?;
    let stem = path.file_stem()?.to_string_lossy().to_string();
    // 2026-11-23_14-05-33[-2] -> "2026-11-23 14:05"
    let when = match (stem.get(..10), stem.get(11..16)) {
        (Some(d), Some(t)) => format!("{d} {}", t.replace('-', ":")),
        _ => stem,
    };
    let me = p.player();
    let ended = match (p.result.as_str(), me) {
        ("1-0", Side::White) | ("0-1", Side::Black) => "Won",
        ("1-0", _) | ("0-1", _) => "Lost",
        ("1/2-1/2", _) => "Draw",
        _ => "Not finished",
    };
    Some(Entry {
        path,
        cols: [
            when,
            if me == Side::White { "White" } else { "Black" }.to_string(),
            ended.to_string(),
            {
                let n = p.moves.len().div_ceil(2);
                format!("{n} move{}", if n == 1 { "" } else { "s" })
            },
        ],
    })
}

/// How many rows of the list show at once.
pub const ROWS: usize = 10;

pub struct Picker {
    pub entries: Vec<Entry>,
    pub sel: usize,
    pub top: usize, // first row shown
}

impl Picker {
    pub fn pick(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        self.sel = self
            .sel
            .saturating_add_signed(delta)
            .min(self.entries.len() - 1);
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + ROWS {
            self.top = self.sel + 1 - ROWS;
        }
    }
}

/// The replay part of the window: the list open, a game open, or neither.
#[derive(Default)]
pub struct State {
    pub picker: Option<Picker>,
    pub run: Option<Run>,
    pub error: Option<String>,
}

impl State {
    pub fn active(&self) -> bool {
        self.picker.is_some() || self.run.is_some()
    }

    pub fn open_list(&mut self) {
        self.error = None;
        self.picker = Some(Picker {
            entries: list_games(),
            sel: 0,
            top: 0,
        });
    }

    /// Open the highlighted game. A file that can't be read leaves the list open
    /// with a message.
    pub fn open_selected(&mut self) {
        let Some(picker) = &self.picker else { return };
        let Some(e) = picker.entries.get(picker.sel) else {
            return;
        };
        let loaded = fs::read_to_string(&e.path)
            .map_err(|e| e.to_string())
            .and_then(|t| parse_pgn(&t));
        match loaded {
            Ok(p) => {
                self.run = Some(Run::new(&p, e.cols[0].clone()));
                self.picker = None;
                self.error = None;
            }
            Err(msg) => self.error = Some(format!("Can't open that game: {msg}")),
        }
    }

    pub fn leave(&mut self) {
        *self = State::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_headers_numbers_comments_and_variations() {
        let pgn = "[Event \"x\"]\n[White \"You\"]\n[Black \"AI\"]\n[Result \"1-0\"]\n\n\
            1. e4 {king pawn} e5 (1... c5 2. Nf3) 2. Nf3 $1 Nc6 3.Bb5 a6!? ; late\n4. Ba4 1-0\n";
        let p = parse_pgn(pgn).unwrap();
        assert_eq!(p.moves.len(), 7);
        assert_eq!(p.result, "1-0");
        assert_eq!(p.player(), Side::White);
    }

    #[test]
    fn a_black_game_from_a_set_position() {
        let pgn = "[White \"AI\"]\n[Black \"You\"]\n[SetUp \"1\"]\n\
            [FEN \"4k3/8/8/8/8/8/4P3/4K3 b - - 0 7\"]\n\n7... Kd7 8. e4 *\n";
        let p = parse_pgn(pgn).unwrap();
        assert_eq!(p.player(), Side::Black);
        assert_eq!(p.moves.len(), 2);
        assert_eq!(p.start.turn(), Side::Black);
    }

    #[test]
    fn an_illegal_move_is_named() {
        let err = parse_pgn("1. e4 e5 2. Ke3 *").err().unwrap();
        assert!(err.contains("move 3"), "{err}");
    }

    #[test]
    fn stepping_shows_each_position_and_stops_at_the_ends() {
        let p = parse_pgn("1. e4 e5 2. Nf3 *").unwrap();
        let mut r = Run::new(&p, "t".into());
        assert_eq!((r.idx, r.total()), (0, 3));
        assert_eq!(r.view.pos, Chess::default());
        r.step(-1);
        assert_eq!(r.idx, 0);
        r.step(1);
        assert_eq!(r.view.sans, ["e4"]);
        assert_eq!(
            r.view.last,
            Some(("e2".parse().unwrap(), "e4".parse().unwrap()))
        );
        r.goto(99);
        assert_eq!(r.idx, 3);
        assert_eq!(r.view.sans, ["e4", "e5", "Nf3"]);
        r.step(1);
        assert_eq!(r.idx, 3);
        r.goto(0);
        assert_eq!(r.view.last, None);
    }

    #[test]
    fn the_picker_keeps_the_choice_in_view() {
        let entries = (0..25)
            .map(|i| Entry {
                path: PathBuf::from(format!("{i}.pgn")),
                cols: Default::default(),
            })
            .collect();
        let mut pk = Picker {
            entries,
            sel: 0,
            top: 0,
        };
        pk.pick(-1);
        assert_eq!((pk.sel, pk.top), (0, 0));
        pk.pick(12);
        assert_eq!(pk.sel, 12);
        assert!(pk.sel >= pk.top && pk.sel < pk.top + ROWS);
        pk.pick(100);
        assert_eq!(pk.sel, 24);
        pk.pick(-100);
        assert_eq!((pk.sel, pk.top), (0, 0));
    }

    #[test]
    fn saved_files_come_back_the_same() {
        use crate::save::pgn_text;
        let mut g = Game::new_as(Side::Black);
        for m in [
            "d4", "d5", "c4", "e6", "Nc3", "Nf6", "Bg5", "Be7", "e3", "O-O",
        ] {
            let mv = crate::game::parse_move(&g.pos, m).expect(m);
            g.play(&mv);
        }
        let p = parse_pgn(&pgn_text(&g, 0)).unwrap();
        assert_eq!(p.moves.len(), 10);
        assert_eq!(p.player(), Side::Black);
        let r = Run::new(&p, String::new());
        let mut r = r;
        r.goto(10);
        assert_eq!(r.view.pos, g.pos);
    }

    #[test]
    fn date_is_read_back_from_the_file_name() {
        let dir = std::env::temp_dir().join(format!("side-chess-replay-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let f = dir.join("2026-11-23_14-05-33.pgn");
        fs::write(
            &f,
            "[White \"AI\"]\n[Black \"You\"]\n[Result \"1-0\"]\n\n1. e4 e5 1-0\n",
        )
        .unwrap();
        let e = entry(f).unwrap();
        assert_eq!(e.cols, ["2026-11-23 14:05", "Black", "Lost", "1 move"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
