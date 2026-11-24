//! The game: rules, moves typed or clicked by the player, and the two files that
//! connect the window to an AI session (~/.side-chess/state.txt and a move file).
use shakmaty::{
    fen::Fen, san::San, Chess, Color as Side, EnPassantMode, File, Move, Position, Rank, Role,
    Square,
};
use std::{
    fs,
    path::PathBuf,
    str::FromStr,
    time::{Duration, SystemTime},
};

pub struct Game {
    pub pos: Chess,
    pub player: Side, // the side the person plays; the AI plays the other
    pub history: Vec<(Chess, Option<(Square, Square)>)>,
    pub sans: Vec<String>,
    pub selected: Option<Square>,
    pub target: Option<Square>, // keyboard: square offered for the selected piece, played on Enter
    pub last: Option<(Square, Square)>,
    pub note: String,
    pub promotion: Option<(Square, Square)>, // a pawn's last step waiting for the piece the person picks
    pub rated: bool, // the finished game's result has been counted in the ratings
    pub saved: Option<(PathBuf, usize)>, // the file this game was saved to, and how many moves it had then
    pub hint: String, // shown in the window only, never in state.txt (a non-empty `note:` there makes the AI think its move was refused)
    pub two: bool, // two people at one screen: no AI, both sides moved by clicking, `player` is white
    pub engine: Option<u32>, // Stockfish is the opponent, held to this Elo (no AI session is asked)
}

impl Game {
    pub fn new() -> Self {
        Game::new_as(Side::White)
    }

    /// A new game where the person plays `player` (white moves first, so the AI
    /// opens when the person plays black).
    pub fn new_as(player: Side) -> Self {
        Game {
            pos: Chess::default(),
            player,
            history: vec![],
            sans: vec![],
            selected: None,
            target: None,
            last: None,
            note: String::new(),
            promotion: None,
            rated: false,
            saved: None,
            hint: String::new(),
            two: false,
            engine: None,
        }
    }

    /// A new game against Stockfish at `elo`, the person playing `player`.
    pub fn new_vs_engine(player: Side, elo: u32) -> Self {
        Game {
            engine: Some(elo),
            ..Game::new_as(player)
        }
    }

    /// A fresh game of the same kind as this one: the same mode, the same side, the same
    /// Stockfish strength.
    pub fn fresh(&self) -> Game {
        if self.two {
            Game::new_two()
        } else if let Some(elo) = self.engine {
            Game::new_vs_engine(self.player, elo)
        } else {
            Game::new_as(self.player)
        }
    }

    /// A new game for two people at one screen (in person): the window moves whichever
    /// side's turn it is, and no AI is asked.
    pub fn new_two() -> Self {
        Game {
            two: true,
            ..Game::new_as(Side::White)
        }
    }

    /// The side whose pieces the person moves now: the chosen side against the AI,
    /// whoever is to move when two people share the screen.
    pub fn mover(&self) -> Side {
        if self.two {
            self.pos.turn()
        } else {
            self.player
        }
    }

    /// The result in words once the game is over (None while it goes on).
    pub fn over(&self) -> Option<String> {
        if !self.two {
            let over = game_over(&self.pos, self.player);
            return match self.engine {
                Some(_) => over.map(|s| s.replace("AI wins", "Stockfish wins")),
                None => over,
            };
        }
        if self.pos.is_checkmate() {
            Some(match self.pos.turn() {
                Side::White => "Checkmate. Black wins.".into(),
                Side::Black => "Checkmate. White wins.".into(),
            })
        } else {
            game_over(&self.pos, self.player)
        }
    }

    /// Has the person a move to make: their turn against the AI, or any turn when two share.
    pub fn person_to_move(&self) -> bool {
        (self.two || self.pos.turn() == self.player) && self.over().is_none()
    }

    /// S swaps sides, but only between games: before the first move, or once the game
    /// is over. In the middle of a game it is refused with a message in the window and
    /// nothing changes (start a new game first).
    pub fn swap_sides(&mut self) {
        if self.two {
            self.hint = "Swapping sides is for games against the AI.".into();
            return;
        }
        let in_progress = !self.sans.is_empty() && self.over().is_none();
        if in_progress {
            self.hint = "Game in progress. Start a new game first, then swap sides.".into();
        } else {
            let engine = self.engine;
            *self = Game::new_as(self.player.other());
            self.engine = engine; // the same opponent, on the other side
            write_state(self);
        }
    }

    /// A move the person has chosen. A pawn reaching the last rank waits for the
    /// piece to be picked (see `promote`); anything else is played at once.
    pub fn try_move(&mut self, from: Square, to: Square) -> MoveStart {
        let moves: Vec<Move> = self
            .pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from() == Some(from) && shown_to(m) == to)
            .collect();
        if moves.iter().any(|m| m.promotion().is_some()) {
            self.promotion = Some((from, to));
            self.target = None;
            MoveStart::NeedPromotion
        } else if let Some(m) = moves.first() {
            self.play(m);
            MoveStart::Played
        } else {
            MoveStart::Illegal
        }
    }

    /// The person picked the piece for the waiting pawn move: queen, rook, bishop or knight.
    pub fn promote(&mut self, role: Role) -> bool {
        let Some((from, to)) = self.promotion.take() else {
            return false;
        };
        let m =
            self.pos.legal_moves().into_iter().find(|m| {
                m.from() == Some(from) && shown_to(m) == to && m.promotion() == Some(role)
            });
        match m {
            Some(m) => {
                self.play(&m);
                true
            }
            None => false,
        }
    }

    pub fn cancel_promotion(&mut self) {
        self.promotion = None;
    }

    pub fn play(&mut self, m: &Move) {
        self.promotion = None;
        let san = San::from_move(&self.pos, m).to_string();
        self.history.push((self.pos.clone(), self.last));
        self.pos = self.pos.clone().play(m).expect("legal move");
        self.last = Some((m.from().unwrap_or(m.to()), shown_to(m)));
        self.sans.push(san);
        self.selected = None;
        self.target = None;
        self.note.clear();
        self.hint.clear();
        write_state(self);
    }

    // Take back to the last time it was the player's move
    pub fn undo(&mut self) {
        self.promotion = None;
        while let Some((p, l)) = self.history.pop() {
            self.pos = p;
            self.last = l;
            self.sans.pop();
            // two people: take back one move; against the AI: back to the person's turn
            if self.two || self.pos.turn() == self.player {
                break;
            }
        }
        self.selected = None;
        self.target = None;
        write_state(self);
    }
}

/// What came of a move the person chose.
#[derive(Debug, PartialEq, Eq)]
pub enum MoveStart {
    Played,
    NeedPromotion, // a pawn on its last step: waiting for queen, rook, bishop or knight
    Illegal,
}

pub fn queen_or_plain(m: &Move) -> bool {
    matches!(m.promotion(), None | Some(Role::Queen))
}

/// The square the moving piece lands on. shakmaty names the rook's square for
/// castling; on the board the king lands two squares along (g or c file).
pub fn shown_to(m: &Move) -> Square {
    match m {
        Move::Castle { king, rook } => {
            let file = if rook.file() > king.file() {
                File::G
            } else {
                File::C
            };
            Square::from_coords(file, king.rank())
        }
        _ => m.to(),
    }
}

pub fn legal_to(pos: &Chess, from: Square, to: Square) -> Option<Move> {
    pos.legal_moves()
        .into_iter()
        .find(|m| m.from() == Some(from) && shown_to(m) == to && queen_or_plain(m))
}

pub fn dist(a: Square, b: Square) -> u32 {
    let df = (u32::from(a.file()) as i32 - u32::from(b.file()) as i32).unsigned_abs();
    let dr = (u32::from(a.rank()) as i32 - u32::from(b.rank()) as i32).unsigned_abs();
    df.max(dr) * 10 + df + dr // nearest by king steps, ties broken by straight-line feel
}

// A square typed on the keyboard (letter + number)
pub fn pick_square(g: &mut Game, s: Square) {
    let me = g.mover();
    // One of my pieces: select it
    if g.pos.board().piece_at(s).map(|p| p.color) == Some(me) {
        g.selected = Some(s);
        g.target = None;
        return;
    }
    // A piece is selected: offer the move only if it is legal
    if let Some(from) = g.selected {
        if legal_to(&g.pos, from, s).is_some() {
            g.target = Some(s);
        }
        return;
    }
    // Nothing selected: take my closest piece that can go there and offer it,
    // otherwise just select my closest piece
    let mine: Vec<Square> = g.pos.board().by_color(me).into_iter().collect();
    let can_go = mine
        .iter()
        .copied()
        .filter(|&f| legal_to(&g.pos, f, s).is_some())
        .min_by_key(|&f| dist(f, s));
    match can_go {
        Some(f) => {
            g.selected = Some(f);
            g.target = Some(s);
        }
        None => g.selected = mine.into_iter().min_by_key(|&f| dist(f, s)),
    }
}

pub fn live_dir() -> PathBuf {
    // HOME on Mac and in tests; USERPROFILE is the home folder on Windows
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".side-chess")
}

pub fn sq(file: u32, rank: u32) -> Square {
    Square::from_coords(File::new(file), Rank::new(rank))
}

pub fn game_over(pos: &Chess, player: Side) -> Option<String> {
    if pos.is_checkmate() {
        Some(if pos.turn() == player {
            "Checkmate. AI wins.".into()
        } else {
            "Checkmate. You win.".into()
        })
    } else if pos.is_stalemate() {
        Some("Stalemate. It's a draw.".into())
    } else if pos.is_insufficient_material() {
        Some("Draw. Not enough pieces to mate.".into())
    } else {
        None
    }
}

pub fn move_list(sans: &[String]) -> String {
    sans.chunks(2)
        .enumerate()
        .map(|(i, p)| format!("{}. {}", i + 1, p.join(" ")))
        .collect::<Vec<_>>()
        .join("  ")
}

/// The first line of the state file: whose move it is, and whether that is the
/// person or the AI.
pub fn turn_text(g: &Game) -> String {
    let pos = &g.pos;
    match g.over() {
        Some(s) => format!("game over: {s}"),
        // Stockfish is not an AI session: nothing here may wake one, so it is always "(player)"
        None if g.engine.is_some() => format!(
            "{} (player) vs Stockfish",
            if pos.turn() == Side::White {
                "white"
            } else {
                "black"
            }
        ),
        None => format!(
            "{} ({})",
            if pos.turn() == Side::White {
                "white"
            } else {
                "black"
            },
            // in person both sides are people: nothing here is the AI's turn
            if g.two || pos.turn() == g.player {
                "player"
            } else {
                "AI"
            }
        ),
    }
}

// The file an AI session reads every turn
pub fn write_state(g: &Game) {
    let text = state_text(g);
    let dir = live_dir();
    let _ = fs::create_dir_all(&dir);
    let tmp = dir.join("state.tmp");
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(tmp, dir.join("state.txt"));
    }
}

/// What state.txt says. (The window-only `hint` is not in it.)
pub fn state_text(g: &Game) -> String {
    let pos = &g.pos;
    let fen = Fen::from_position(pos.clone(), EnPassantMode::Legal).to_string();
    let turn = turn_text(g);
    let mut board = String::new();
    for r in (0..8).rev() {
        board.push_str(&format!("{} ", r + 1));
        for f in 0..8 {
            board.push(match pos.board().piece_at(sq(f, r)) {
                Some(p) => p.char(),
                None => '.',
            });
            board.push(' ');
        }
        board.push('\n');
    }
    board.push_str("  a b c d e f g h\n");
    let legal: Vec<String> = pos
        .legal_moves()
        .iter()
        .map(|m| San::from_move(pos, m).to_string())
        .collect();
    format!(
        "turn: {turn}\nfen: {fen}\ncheck: {}\nmoves: {}\nnote: {}\nlegal: {}\n\n{board}",
        pos.is_check(),
        move_list(&g.sans),
        g.note,
        legal.join(" "),
    )
}

// An AI reply: SAN (Nf6) or from-to (g8f6)
pub fn parse_move(pos: &Chess, s: &str) -> Option<Move> {
    if let Ok(san) = San::from_str(s) {
        if let Ok(m) = san.to_move(pos) {
            return Some(m);
        }
    }
    let s = s.to_lowercase();
    // get() instead of slicing: a stray non-ASCII character must not crash the app
    if let (Some(from), Some(to)) = (s.get(0..2), s.get(2..4)) {
        return pos
            .legal_moves()
            .into_iter()
            .filter(|m| m.from().map(|f| f.to_string()) == Some(from.into()))
            .filter(|m| m.to().to_string() == to || shown_to(m).to_string() == to)
            .find(|m| match s.chars().nth(4) {
                Some(ch) => m.promotion().map(|r| r.char()) == Some(ch),
                None => m.promotion().is_none() || m.promotion() == Some(Role::Queen),
            });
    }
    None
}

/// The text of a move file. A UTF-8 byte-order mark is dropped, and UTF-16 (what
/// Windows PowerShell 5 writes with `>` or Out-File) is read too, with or without
/// its byte-order mark. None means the bytes aren't text in any of these.
pub fn decode_move_file(bytes: &[u8]) -> Option<String> {
    fn utf16(b: &[u8], big_endian: bool) -> Option<String> {
        let pairs = b.chunks_exact(2);
        if !pairs.remainder().is_empty() {
            return None;
        }
        let units: Vec<u16> = pairs
            .map(|c| {
                if big_endian {
                    u16::from_be_bytes([c[0], c[1]])
                } else {
                    u16::from_le_bytes([c[0], c[1]])
                }
            })
            .collect();
        String::from_utf16(&units).ok()
    }
    // UTF-16 without a byte-order mark: ASCII text with a zero byte beside each letter
    let zeros_at = |start: usize| {
        bytes.len() >= 2
            && bytes.chunks_exact(2).remainder().is_empty()
            && bytes.iter().skip(start).step_by(2).all(|&b| b == 0)
    };
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8(rest.to_vec()).ok(),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, false),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, true),
        _ if zeros_at(1) => utf16(bytes, false),
        _ if zeros_at(0) => utf16(bytes, true),
        _ => String::from_utf8(bytes.to_vec()).ok(),
    }
}

fn move_note(g: &mut Game, note: String) {
    g.note = note;
    write_state(g);
}

pub fn check_opponent_move(g: &mut Game) {
    if g.two || g.engine.is_some() {
        return; // two people at one screen, or Stockfish: no AI session to listen to
    }
    let dir = live_dir();
    // Codex writes the shared name; the Claude-specific name remains supported
    // for existing Claude Code installs and older app versions.
    let Some(path) = [dir.join("opponent_move.txt"), dir.join("claude_move.txt")]
        .into_iter()
        .find(|path| path.is_file())
    else {
        return;
    };
    let our_turn = g.pos.turn() != g.player && game_over(&g.pos, g.player).is_none();
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(_) => {
            // Usually it's still being written (Windows locks it), so try again next
            // time. If it can't be read but can be removed, say so instead of stalling.
            if fs::remove_file(&path).is_ok() && our_turn {
                move_note(
                    g,
                    "Couldn't read the AI's move file. Waiting for another move.".into(),
                );
            }
            return;
        }
    };
    let text = decode_move_file(&bytes);
    if text.as_deref().is_some_and(|t| t.trim().is_empty()) {
        // Empty: most likely caught between creating the file and writing the move,
        // so leave it for a moment. One that stays empty is cleared.
        let fresh = fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < Duration::from_secs(2));
        if !fresh {
            let _ = fs::remove_file(&path);
        }
        return;
    }
    let _ = fs::remove_file(dir.join("claude_move.txt"));
    let _ = fs::remove_file(dir.join("opponent_move.txt"));
    if !our_turn {
        return;
    }
    let Some(text) = text else {
        move_note(
            g,
            "The AI's move file isn't plain text. Waiting for another move.".into(),
        );
        return;
    };
    let s = text.trim();
    match parse_move(&g.pos, s) {
        Some(m) => g.play(&m),
        None => {
            // The window can only draw plain ASCII, so show anything else as '?'
            let shown: String = s
                .chars()
                .take(16)
                .map(|c| {
                    if c.is_ascii_graphic() || c == ' ' {
                        c
                    } else {
                        '?'
                    }
                })
                .collect();
            move_note(
                g,
                format!("AI sent '{shown}', which isn't legal here. Waiting for another move."),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        // keep test games out of the real ~/.side-chess
        let dir = std::env::temp_dir().join("side-chess-test-home");
        let _ = fs::create_dir_all(&dir);
        std::env::set_var("HOME", dir);
    }

    fn from_fen(fen: &str) -> Game {
        setup();
        let mut g = Game::new();
        g.pos = fen
            .parse::<Fen>()
            .unwrap()
            .into_position(shakmaty::CastlingMode::Standard)
            .unwrap();
        g
    }

    fn s(name: &str) -> Square {
        name.parse().unwrap()
    }

    #[test]
    fn claude_moves_parse_as_san_or_from_to() {
        let g = from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
        assert!(parse_move(&g.pos, "Nf6").is_some());
        assert!(parse_move(&g.pos, "g8f6").is_some());
        assert!(parse_move(&g.pos, "e5").is_some());
        assert!(parse_move(&g.pos, "Ke7").is_none());
        assert!(parse_move(&g.pos, "zz9").is_none());
        assert!(parse_move(&g.pos, "").is_none());
        // non-ASCII text must not crash the from-to reader
        assert!(parse_move(&g.pos, "é5e4").is_none());
        assert!(parse_move(&g.pos, "e7é5").is_none());
        assert!(parse_move(&g.pos, "ééé").is_none());
    }

    #[test]
    fn move_files_with_a_byte_order_mark_or_in_utf16_are_read() {
        let utf16le = |s: &str| {
            s.encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<u8>>()
        };
        let utf16be = |s: &str| {
            s.encode_utf16()
                .flat_map(u16::to_be_bytes)
                .collect::<Vec<u8>>()
        };
        assert_eq!(decode_move_file(b"Nf6\n").as_deref(), Some("Nf6\n"));
        assert_eq!(decode_move_file(b"\xEF\xBB\xBFNf6").as_deref(), Some("Nf6"));
        let le_bom = [&[0xFF, 0xFE][..], &utf16le("Nf6\r\n")].concat();
        assert_eq!(decode_move_file(&le_bom).as_deref(), Some("Nf6\r\n"));
        let be_bom = [&[0xFE, 0xFF][..], &utf16be("Nf6")].concat();
        assert_eq!(decode_move_file(&be_bom).as_deref(), Some("Nf6"));
        assert_eq!(decode_move_file(&utf16le("e5")).as_deref(), Some("e5"));
        assert_eq!(decode_move_file(&[0xC3, 0x28, 0xFF]), None);
        assert_eq!(decode_move_file(&[0xFF, 0xFE, 0x41]), None);
    }

    // One test for every move-file case, so they don't race on the shared folder
    #[test]
    fn move_files_play_or_leave_a_note_and_never_stall() {
        let after_e4 = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1";
        let dir = {
            setup();
            live_dir()
        };
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("opponent_move.txt");
        let send = |bytes: &[u8]| {
            let mut g = from_fen(after_e4);
            fs::write(&file, bytes).unwrap();
            check_opponent_move(&mut g);
            g
        };

        // UTF-8 with a byte-order mark (PowerShell 7's Out-File -Encoding utf8BOM)
        let g = send(b"\xEF\xBB\xBFNf6\r\n");
        assert_eq!(g.sans.last().map(String::as_str), Some("Nf6"));
        assert!(!file.exists());

        // UTF-16 with a byte-order mark (Windows PowerShell 5's `echo Nf6 > file`)
        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend("e5\r\n".encode_utf16().flat_map(u16::to_le_bytes));
        let g = send(&utf16);
        assert_eq!(g.sans.last().map(String::as_str), Some("e5"));
        assert!(!file.exists());

        // Not text at all: removed, with a note, instead of waiting forever
        let g = send(&[0xC3, 0x28, 0xFF]);
        assert!(g.sans.is_empty());
        assert!(g.note.contains("isn't plain text"), "{}", g.note);
        assert!(!file.exists());

        // Non-ASCII text: a note, not a crash
        let g = send("é5é5".as_bytes());
        assert!(g.note.contains("isn't legal"), "{}", g.note);
        assert!(g.note.is_ascii(), "{}", g.note);
        assert!(!file.exists());

        // Just created and still empty: left alone so the move can still arrive
        let g = send(b"");
        assert!(g.note.is_empty() && g.sans.is_empty());
        assert!(file.exists());
        let _ = fs::remove_file(&file);
    }

    #[test]
    fn game_over_detects_mate_stalemate_and_draws() {
        let mate = from_fen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3");
        assert_eq!(
            game_over(&mate.pos, Side::White).as_deref(),
            Some("Checkmate. AI wins.")
        );
        let stale = from_fen("k7/2Q5/1K6/8/8/8/8/8 b - - 0 1");
        assert_eq!(
            game_over(&stale.pos, Side::White).as_deref(),
            Some("Stalemate. It's a draw.")
        );
        let bare = from_fen("k7/8/1K6/8/8/8/8/8 w - - 0 1");
        assert!(game_over(&bare.pos, Side::White)
            .unwrap()
            .starts_with("Draw."));
        assert!(game_over(&Chess::default(), Side::White).is_none());
    }

    #[test]
    fn typing_a_square_with_nothing_selected_picks_the_closest_piece_that_can_go_there() {
        let mut g = from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        pick_square(&mut g, s("e4"));
        assert_eq!((g.selected, g.target), (Some(s("e2")), Some(s("e4"))));
    }

    #[test]
    fn typing_another_own_piece_switches_and_illegal_squares_are_ignored() {
        let mut g = from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        pick_square(&mut g, s("e2"));
        assert_eq!(g.selected, Some(s("e2")));
        pick_square(&mut g, s("g1"));
        assert_eq!((g.selected, g.target), (Some(s("g1")), None));
        pick_square(&mut g, s("e5"));
        assert_eq!(
            g.target, None,
            "a knight can't reach e5, so nothing is offered"
        );
        pick_square(&mut g, s("f3"));
        assert_eq!(g.target, Some(s("f3")));
    }

    #[test]
    fn playing_records_the_last_move_and_undo_returns_to_whites_turn() {
        let mut g = from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        let m = legal_to(&g.pos, s("e2"), s("e4")).unwrap();
        g.play(&m);
        assert_eq!(g.last, Some((s("e2"), s("e4"))));
        let reply = parse_move(&g.pos, "c5").unwrap();
        g.play(&reply);
        assert_eq!(move_list(&g.sans), "1. e4 c5");
        g.undo();
        assert_eq!(g.pos, Chess::default());
        assert!(g.sans.is_empty());
    }

    #[test]
    fn castling_works_by_click_and_by_from_to() {
        let g = from_fen("r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq - 0 1");
        let s = |n: &str| Square::from_str(n).unwrap();
        // the king moves two squares along; the rook comes with it
        let m = legal_to(&g.pos, s("e1"), s("g1")).expect("O-O by clicking g1");
        let after = g.pos.clone().play(&m).unwrap();
        assert_eq!(
            after.board().piece_at(s("f1")).map(|p| p.role),
            Some(Role::Rook)
        );
        assert!(
            legal_to(&g.pos, s("e1"), s("c1")).is_some(),
            "O-O-O by clicking c1"
        );
        assert!(parse_move(&g.pos, "e1g1").is_some());
        assert!(parse_move(&g.pos, "O-O").is_some());
        // the last move shown is the king's, not the rook's
        let mut g = g;
        g.play(&m);
        assert_eq!(g.last, Some((s("e1"), s("g1"))));
    }

    #[test]
    fn en_passant_works_by_click_and_by_move_text() {
        // black has just played d7-d5 next to the white pawn on e5
        let mut g = from_fen("rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 3");
        let s = |n: &str| Square::from_str(n).unwrap();
        assert!(parse_move(&g.pos, "exd6").is_some());
        assert!(parse_move(&g.pos, "e5d6").is_some());
        let m = legal_to(&g.pos, s("e5"), s("d6")).expect("clicking d6 captures en passant");
        g.play(&m);
        assert!(
            g.pos.board().piece_at(s("d5")).is_none(),
            "the d5 pawn is taken"
        );
        assert_eq!(g.last, Some((s("e5"), s("d6"))));
        assert_eq!(g.sans.last().map(String::as_str), Some("exd6"));
    }

    #[test]
    fn pawns_promote_to_a_queen() {
        let g = from_fen("8/4P1k1/8/8/8/8/8/4K3 w - - 0 1");
        let m = legal_to(&g.pos, s("e7"), s("e8")).unwrap();
        assert_eq!(m.promotion(), Some(Role::Queen));
    }

    #[test]
    fn playing_black_the_ai_opens_and_the_result_is_told_from_the_players_side() {
        setup();
        let g = Game::new_as(Side::Black);
        assert_eq!(g.player, Side::Black);
        assert_eq!(g.pos.turn(), Side::White, "white (the AI) moves first");
        // the same mate: white to move and mated means the player (black) wins
        let mate = from_fen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3");
        assert_eq!(
            game_over(&mate.pos, Side::Black).as_deref(),
            Some("Checkmate. You win.")
        );
    }

    #[test]
    fn playing_black_typed_squares_pick_black_pieces_and_undo_goes_back_to_blacks_move() {
        let mut g = from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
        g.player = Side::Black;
        pick_square(&mut g, s("e5"));
        assert_eq!((g.selected, g.target), (Some(s("e7")), Some(s("e5"))));
        let m = legal_to(&g.pos, s("e7"), s("e5")).unwrap();
        g.play(&m);
        let reply = parse_move(&g.pos, "Nf3").unwrap();
        g.play(&reply);
        assert_eq!(g.pos.turn(), Side::Black);
        g.play(&parse_move(&g.pos, "Nc6").unwrap());
        g.play(&parse_move(&g.pos, "Bb5").unwrap());
        g.undo();
        assert_eq!(g.pos.turn(), Side::Black, "back to a move the player makes");
        assert_eq!(g.sans.len(), 2);
    }

    #[test]
    fn the_turn_line_says_who_is_the_player_and_who_is_the_ai() {
        let mut g = from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        assert_eq!(turn_text(&g), "white (player)");
        g.player = Side::Black;
        assert_eq!(turn_text(&g), "white (AI)");
        let g = from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");
        assert_eq!(turn_text(&g), "black (AI)");
        let mut g = g;
        g.player = Side::Black;
        assert_eq!(turn_text(&g), "black (player)");
        let mate = from_fen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3");
        assert_eq!(turn_text(&mate), "game over: Checkmate. AI wins.");
    }

    #[test]
    fn s_is_refused_mid_game_and_only_swaps_a_new_or_finished_game() {
        setup();
        // a game in progress: refused, nothing changes, a message for the window
        let mut g = from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        g.play(&parse_move(&g.pos, "e4").unwrap());
        g.play(&parse_move(&g.pos, "e5").unwrap());
        let before = (g.player, g.sans.clone(), g.pos.clone());
        g.swap_sides();
        assert_eq!((g.player, g.sans.clone(), g.pos.clone()), before);
        assert!(g.hint.contains("Start a new game"), "{}", g.hint);
        // the message is for the window only: state.txt (and its note:) never carries it
        assert!(g.note.is_empty());
        assert!(
            !state_text(&g).contains("Start a new game"),
            "{}",
            state_text(&g)
        );
        assert!(state_text(&g).contains("\nnote: \n"));
        // the next move clears the message
        g.play(&parse_move(&g.pos, "Nf3").unwrap());
        assert!(g.hint.is_empty());
        // before the first move: swaps, and the AI (white) opens
        let mut fresh = Game::new();
        fresh.swap_sides();
        assert_eq!(fresh.player, Side::Black);
        assert!(fresh.hint.is_empty());
        assert_eq!(turn_text(&fresh), "white (AI)");
        // after the game has ended: swaps, back to a new game
        let mut over = from_fen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3");
        over.sans.push("Qh4#".into());
        over.swap_sides();
        assert_eq!(over.player, Side::Black);
        assert!(over.sans.is_empty());
    }

    #[test]
    fn a_pawn_on_its_last_step_waits_for_the_piece_you_pick() {
        for (role, san) in [
            (Role::Queen, "e8=Q"),
            (Role::Rook, "e8=R"),
            (Role::Bishop, "e8=B"),
            (Role::Knight, "e8=N"),
        ] {
            let mut g = from_fen("8/4P1k1/8/8/8/8/8/4K3 w - - 0 1");
            assert_eq!(g.try_move(s("e7"), s("e8")), MoveStart::NeedPromotion);
            assert!(
                g.sans.is_empty(),
                "nothing is played until a piece is picked"
            );
            assert_eq!(g.promotion, Some((s("e7"), s("e8"))));
            assert!(g.promote(role));
            assert_eq!(g.sans.last().map(String::as_str), Some(san), "{role:?}");
            assert_eq!(g.pos.board().piece_at(s("e8")).map(|p| p.role), Some(role));
            assert!(g.promotion.is_none());
        }
        // changing your mind: nothing is played
        let mut g = from_fen("8/4P1k1/8/8/8/8/8/4K3 w - - 0 1");
        g.try_move(s("e7"), s("e8"));
        g.cancel_promotion();
        assert!(g.promotion.is_none() && g.sans.is_empty());
        // ordinary moves are played straight away, bad ones refused
        assert_eq!(g.try_move(s("e1"), s("e2")), MoveStart::Played);
        let mut h = from_fen("8/4P1k1/8/8/8/8/8/4K3 w - - 0 1");
        assert_eq!(h.try_move(s("e7"), s("e6")), MoveStart::Illegal);
        assert!(h.promote(Role::Queen) == false, "nothing is waiting");
        // the opponent's replies still name the piece themselves
        let b = from_fen("4k3/8/8/8/8/8/4p3/7K b - - 0 1");
        assert_eq!(
            parse_move(&b.pos, "e2e1n").and_then(|m| m.promotion()),
            Some(Role::Knight)
        );
        assert_eq!(
            parse_move(&b.pos, "e1=R").and_then(|m| m.promotion()),
            Some(Role::Rook)
        );
    }
}

#[cfg(test)]
mod in_person_tests {
    use super::*;

    fn home() {
        let dir = std::env::temp_dir().join("side-chess-test-home");
        let _ = fs::create_dir_all(&dir);
        std::env::set_var("HOME", dir);
    }

    fn play(g: &mut Game, from: &str, to: &str) {
        let (f, t): (Square, Square) = (from.parse().unwrap(), to.parse().unwrap());
        assert_eq!(g.try_move(f, t), MoveStart::Played, "{from}{to}");
    }

    #[test]
    fn both_sides_are_moved_from_the_window_and_the_ai_is_never_the_one_to_move() {
        home();
        let mut g = Game::new_two();
        assert!(g.two && g.person_to_move() && g.mover() == Side::White);
        assert_eq!(turn_text(&g), "white (player)");
        play(&mut g, "e2", "e4");
        assert!(g.person_to_move(), "black's turn is a person's too");
        assert_eq!(g.mover(), Side::Black);
        assert_eq!(turn_text(&g), "black (player)");
        assert!(state_text(&g).starts_with(
            "turn: black (player)
"
        ));
        // the AI watch only wakes for "(AI)", so an in-person game never wakes it
        assert!(!state_text(&g).contains("(AI)"));
    }

    #[test]
    fn against_stockfish_it_is_never_an_ai_sessions_turn_and_the_result_names_stockfish() {
        home();
        let mut g = Game::new_vs_engine(Side::White, 1500);
        assert_eq!(turn_text(&g), "white (player) vs Stockfish");
        play(&mut g, "f2", "f3");
        // black's turn is Stockfish's, but the state file never says "(AI)": a watching
        // assistant would take that for its own turn
        assert_eq!(turn_text(&g), "black (player) vs Stockfish");
        assert!(!state_text(&g).contains("(AI)"));
        assert!(!g.person_to_move(), "the person waits for the engine");
        for m in [("e7", "e5"), ("g2", "g4"), ("d8", "h4")] {
            let (f, t) = (m.0.parse().unwrap(), m.1.parse().unwrap());
            let mv = legal_to(&g.pos, f, t).unwrap();
            g.play(&mv);
        }
        assert_eq!(g.over().as_deref(), Some("Checkmate. Stockfish wins."));
        let pgn = crate::save::pgn_text(&g, 0);
        assert!(pgn.contains("[Black \"Stockfish 1500\"]"), "{pgn}");
        let parsed = crate::replay::parse_pgn(&pgn).unwrap();
        assert_eq!(parsed.engine_elo(), Some(1500));
        assert!(parsed.vs_engine() && !parsed.two());
    }

    #[test]
    fn a_fresh_game_and_swapping_sides_keep_the_opponent() {
        home();
        let mut g = Game::new_vs_engine(Side::White, 2200);
        assert_eq!(
            (g.fresh().engine, g.fresh().player),
            (Some(2200), Side::White)
        );
        g.swap_sides(); // nothing played yet: allowed
        assert_eq!((g.engine, g.player), (Some(2200), Side::Black));
        assert_eq!(Game::new_two().fresh().two, true);
        assert_eq!(Game::new_as(Side::Black).fresh().engine, None);
    }

    #[test]
    fn typed_squares_pick_the_side_to_move() {
        home();
        let mut g = Game::new_two();
        play(&mut g, "e2", "e4");
        // black to move: typing e7 selects black's pawn, not an opponent's
        pick_square(&mut g, "e7".parse().unwrap());
        assert_eq!(g.selected, Some("e7".parse().unwrap()));
    }

    #[test]
    fn take_back_undoes_one_move_and_swap_sides_is_refused() {
        home();
        let mut g = Game::new_two();
        play(&mut g, "e2", "e4");
        play(&mut g, "e7", "e5");
        g.undo();
        assert_eq!(g.sans.len(), 1);
        assert_eq!(g.pos.turn(), Side::Black);
        g.swap_sides();
        assert!(g.two && g.hint.contains("AI"), "{}", g.hint);
        assert_eq!(g.sans.len(), 1, "nothing changed");
    }

    #[test]
    fn the_result_names_the_colour_that_won_and_the_game_is_saved_as_two_players() {
        home();
        let mut g = Game::new_two();
        for (a, b) in [("f2", "f3"), ("e7", "e5"), ("g2", "g4"), ("d8", "h4")] {
            play(&mut g, a, b);
        }
        assert_eq!(g.over().as_deref(), Some("Checkmate. Black wins."));
        assert_eq!(turn_text(&g), "game over: Checkmate. Black wins.");
        assert!(!g.person_to_move());
        let pgn = crate::save::pgn_text(&g, 0);
        assert!(pgn.contains("[White \"Player 1\"]") && pgn.contains("[Black \"Player 2\"]"));
        let parsed = crate::replay::parse_pgn(&pgn).unwrap();
        assert!(parsed.two(), "read back as an in-person game");
        assert_eq!(parsed.result, "0-1");
    }
}
