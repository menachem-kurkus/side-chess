//! Replaying saved games: reading a PGN, listing the games in ~/.side-chess/games,
//! and stepping through one. A replay has its own `Game` for the board to show; it
//! never touches the live game or the files shared with the AI (state.txt).
use crate::engine::{self, Engine, Eval, Grade};
use crate::game::{shown_to, Game};
use crate::rating::{self, Ratings};
use crate::save::games_dir;
use crate::score::{self, Score};
use shakmaty::{fen::Fen, san::San, CastlingMode, Chess, Color as Side, Move, Position, Square};
use std::{fs, path::PathBuf, str::FromStr};

/// A game read from PGN text.
pub struct Parsed {
    pub white: String,
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

    /// The Elo Stockfish was held to, for a game played against it (it is saved as the
    /// opponent's name, "Stockfish 1500").
    pub fn engine_elo(&self) -> Option<u32> {
        [&self.white, &self.black]
            .into_iter()
            .find_map(|n| n.strip_prefix("Stockfish ")?.trim().parse().ok())
    }

    /// A game played against Stockfish, whatever the strength was.
    pub fn vs_engine(&self) -> bool {
        self.white.starts_with("Stockfish") || self.black.starts_with("Stockfish")
    }

    /// A game two people played at one screen (in person): there is no AI in it, so it
    /// has no "You", is never rated, and is shown by colour.
    pub fn two(&self) -> bool {
        self.white == IN_PERSON_WHITE && self.black == IN_PERSON_BLACK
    }
}

/// The names an in-person game is saved under.
pub const IN_PERSON_WHITE: &str = "Player 1";
pub const IN_PERSON_BLACK: &str = "Player 2";

/// Read PGN text: the headers we use, and the main line of moves. Comments,
/// variations, numeric annotations and move numbers are skipped. An illegal or
/// unreadable move is an error naming it (a half game would mislead).
pub fn parse_pgn(text: &str) -> Result<Parsed, String> {
    let (mut white, mut black, mut result) = ("?".to_string(), "?".to_string(), "*".to_string());
    let mut start = Chess::default();
    let mut body = String::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(tag) = t.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
            let (name, rest) = tag.split_once(' ').unwrap_or((tag, ""));
            let value = rest.trim().trim_matches('"').to_string();
            match name {
                "White" => white = value,
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
        white,
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
    engine: Option<Engine>,
    pub evals: Vec<Option<Eval>>, // what the engine thinks of each position, as it gets there
    pub engine_missing: bool,     // no Stockfish was found
    pub score: Option<Score>,     // how each side played, once every position is scored
    pgn: Option<std::path::PathBuf>, // the saved game this is, to keep the score beside it
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
            view: Game {
                two: p.two(),
                engine: p.engine_elo(),
                ..Game::new_as(p.player())
            },
            positions,
            lasts,
            sans,
            idx: 0,
            title,
            engine: None,
            evals: vec![],
            engine_missing: false,
            score: None,
            pgn: None,
        };
        run.goto(0);
        run
    }

    /// Ask the engine at `path` about every position, in the background.
    pub fn start_analysis(&mut self, path: std::path::PathBuf) {
        let fens = self
            .positions
            .iter()
            .map(|p| Fen::from_position(p.clone(), shakmaty::EnPassantMode::Legal).to_string())
            .collect();
        self.evals = vec![None; self.positions.len()];
        self.engine = Some(Engine::start(path, fens));
    }

    /// Take in what the engine has finished since the last look; true if anything came.
    pub fn poll(&mut self) -> bool {
        let Some(e) = &self.engine else { return false };
        let mut got = false;
        while let Ok((i, eval)) = e.results.try_recv() {
            if let Some(slot) = self.evals.get_mut(i) {
                *slot = Some(eval);
                got = true;
            }
        }
        if got && self.score.is_none() && self.analysed() == self.evals.len() {
            let moves: Option<Vec<(i32, i32)>> = (1..=self.total())
                .map(|i| {
                    // both scores from the mover's side
                    let white = self.positions[i - 1].turn() == Side::White;
                    let sign = if white { 1 } else { -1 };
                    let (b, a) = (self.evals[i - 1].as_ref()?, self.evals[i].as_ref()?);
                    Some((b.cp * sign, a.cp * sign))
                })
                .collect();
            self.score = moves
                .filter(|m| !m.is_empty())
                .map(|m| Score::from_moves(&m));
            if let (Some(s), Some(p)) = (&self.score, &self.pgn) {
                score::save(p, s);
            }
        }
        got
    }

    pub fn analysed(&self) -> usize {
        self.evals.iter().filter(|e| e.is_some()).count()
    }

    pub fn analysing(&self) -> bool {
        self.engine.is_some()
    }

    pub fn eval_here(&self) -> Option<&Eval> {
        self.evals.get(self.idx)?.as_ref()
    }

    /// What move number `i` (1 is the first) cost the side that played it, once the
    /// engine has scored the positions before and after it.
    pub fn loss_at(&self, i: usize) -> Option<i32> {
        let (before, after) = (self.evals.get(i.checked_sub(1)?)?, self.evals.get(i)?);
        let mover = self.positions.get(i - 1)?.turn();
        Some(engine::loss(before.as_ref()?, after.as_ref()?, mover))
    }

    pub fn grade_at(&self, i: usize) -> Option<(Grade, i32)> {
        let l = self.loss_at(i)?;
        Some((engine::grade(l), l))
    }

    /// The nearest move before (`dir` -1) or after (+1) this one that is an
    /// inaccuracy or worse, if the engine has scored that far.
    pub fn next_mark(&self, dir: isize) -> Option<usize> {
        let mut i = self.idx as isize + dir;
        while i >= 1 && i as usize <= self.total() {
            if self
                .grade_at(i as usize)
                .is_some_and(|g| g.0 != Grade::Fine)
            {
                return Some(i as usize);
            }
            i += dir;
        }
        None
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
        ("1-0", _) if p.two() => "White won",
        ("0-1", _) if p.two() => "Black won",
        ("1-0", Side::White) | ("0-1", Side::Black) => "Won",
        ("1-0", _) | ("0-1", _) => "Lost",
        ("1/2-1/2", _) => "Draw",
        _ => "Not finished",
    };
    Some(Entry {
        path,
        cols: [
            when,
            if p.two() {
                "In person"
            } else if me == Side::White {
                "White"
            } else {
                "Black"
            }
            .to_string(),
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
    pub ratings: Ratings, // both Glicko-2 ratings, read when the list opens
    pub picker: Option<Picker>,
    pub run: Option<Run>,
    pub error: Option<String>,
    pub engine_found: bool, // whether a Stockfish exists, so the list can say how to get one
}

impl State {
    pub fn active(&self) -> bool {
        self.picker.is_some() || self.run.is_some()
    }

    pub fn open_list(&mut self) {
        self.error = None;
        self.engine_found = engine::find_engine().is_some();
        self.ratings = rating::load();
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
                let mut run = Run::new(&p, e.cols[0].clone());
                run.score = score::load(&e.path);
                run.pgn = Some(e.path.clone());
                match engine::find_engine() {
                    Some(path) => run.start_analysis(path),
                    None => run.engine_missing = true,
                }
                self.run = Some(run);
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

    #[test]
    fn moves_are_graded_and_marks_can_be_jumped_to() {
        let p = parse_pgn("1. e4 e5 2. Nf3 Nc6 3. Bb5 *").unwrap();
        let mut r = Run::new(&p, String::new());
        let ev = |cp| {
            Some(Eval {
                cp,
                mate: None,
                best: None,
            })
        };
        // after 1.e4 level, 1...e5 level, 2.Nf3 level, 2...Nc6 drops 150 for black, 3.Bb5 level
        r.evals = vec![ev(0), ev(20), ev(10), ev(15), ev(165), ev(160)];
        assert_eq!(r.grade_at(1), Some((Grade::Fine, 0)));
        assert_eq!(
            r.grade_at(4),
            Some((Grade::Blunder, 0)).map(|_| (Grade::Mistake, 150))
        );
        assert_eq!(r.loss_at(0), None);
        r.goto(0);
        assert_eq!(r.next_mark(1), Some(4));
        r.goto(5);
        assert_eq!(r.next_mark(-1), Some(4));
        r.goto(4);
        assert_eq!(r.next_mark(1), None);
        r.evals[5] = None;
        assert_eq!(r.grade_at(5), None, "not scored yet");
    }

    /// Ann's six earlier games, from her review notes (read only; skipped where the file
    /// is not there). Each move list must replay legally from the standard start, and
    /// the finished games must end in checkmate.
    #[test]
    fn her_six_earlier_games_import() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let path = std::path::Path::new(&home).join("Documents/Chess/Game Reviews.md");
        let Ok(text) = fs::read_to_string(path) else {
            return;
        };
        let lists: Vec<&str> = text
            .lines()
            .filter(|l| l.starts_with("`1. "))
            .map(|l| l.trim_matches('`'))
            .collect();
        assert_eq!(lists.len(), 6, "six move lists in the notes");
        for (n, moves) in lists.iter().enumerate() {
            let black = n == 5; // in game 6 she played black
            let pgn = format!(
                "[White \"{}\"]\n[Black \"{}\"]\n\n{moves}\n",
                if black { "AI" } else { "You" },
                if black { "You" } else { "AI" },
            );
            let p = parse_pgn(&pgn).unwrap_or_else(|e| panic!("game {}: {e}", n + 1));
            assert_eq!(p.player(), if black { Side::Black } else { Side::White });
            let mut r = Run::new(&p, String::new());
            r.goto(r.total());
            if matches!(n, 0 | 3 | 5) {
                assert!(r.view.pos.is_checkmate(), "game {} ends in mate", n + 1);
            }
            assert!(r.total() >= 14, "game {} has its moves", n + 1);
        }
    }

    /// With a real Stockfish installed: rate a short game and check the scores are sane.
    /// Run by hand: cargo test real_stockfish -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_stockfish_rates_a_game() {
        let Some(path) = engine::find_engine() else {
            panic!("no Stockfish found");
        };
        // 1.f3 e5 2.g4 Qh4# : white's first two moves are blunders-in-waiting
        let p = parse_pgn("1. f3 e5 2. g4 Qh4# 0-1").unwrap();
        let mut r = Run::new(&p, String::new());
        r.start_analysis(path);
        let start = std::time::Instant::now();
        while r.analysed() < r.evals.len() && start.elapsed().as_secs() < 60 {
            r.poll();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(r.analysed(), 5, "all five positions scored");
        for i in 1..=4 {
            println!(
                "move {i}: {:?} eval {:?}",
                r.grade_at(i),
                r.evals[i].as_ref().map(|e| e.text())
            );
        }
        assert_eq!(
            r.grade_at(3).map(|g| g.0),
            Some(Grade::Blunder),
            "2.g4 allows mate"
        );
    }

    /// Scores a full game (an Italian Four Knights, 36 moves, embedded here) with a real
    /// Stockfish and prints both sides. Run by hand:
    /// cargo test score_a_whole_game -- --ignored --nocapture
    #[test]
    #[ignore]
    fn score_a_whole_game_with_real_stockfish() {
        let path = engine::find_engine().expect("no Stockfish found");
        let moves = "1. e4 e5 2. Nf3 Nc6 3. Nc3 Nf6 4. Bc4 Bc5 5. Nb5 Nxe4 6. O-O d5 7. Bd3 O-O \
            8. Bxe4 dxe4 9. d4 exf3 10. dxc5 Qxd1 11. Rxd1 fxg2 12. f4 Bg4 13. Rd3 Rac8 \
            14. fxe5 Nxe5 15. Rg3 Nf3+ 16. Kxg2 Ne5 17. b4 Rfe8 18. Bb2 Be6 19. Bxe5 f6 \
            20. Bc3 Bd5+ 21. Kg1 Re2 22. Rd1 Bxa2 23. Nd4 Re7 24. Nf5 Re5 25. Bxe5 fxe5 \
            26. Ne7+ Kf7 27. Nxc8 Be6 28. Nxa7 c6 29. Rf1+ Ke7 30. Rxg7+ Ke8 31. Rxb7 Bc4 \
            32. Rf6 Ba6 33. Rxh7 Bc8 34. Nxc8 e4 35. Nd6+ Kd8 36. Rf8#";
        let p = parse_pgn(moves).unwrap();
        let mut r = Run::new(&p, String::new());
        r.start_analysis(path);
        let start = std::time::Instant::now();
        while r.score.is_none() && start.elapsed().as_secs() < 240 {
            r.poll();
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let sc = r.score.expect("scored in time");
        for (n, side) in [("white (Ann)", Side::White), ("black (AI)", Side::Black)] {
            let s = sc.of(side);
            println!(
                "{n}: {} moves, avg loss {:.0} cp, accuracy {:.0}%, about {}",
                s.moves,
                s.acpl,
                s.accuracy,
                crate::score::estimate(s.acpl)
            );
        }
        println!("took {:?}", start.elapsed());
    }
}
