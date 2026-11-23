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
    pub hint: String, // shown in the window only (never in state.txt), e.g. a refused key
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
            hint: String::new(),
        }
    }

    /// S swaps sides, but only between games: a new game that hasn't started or
    /// one that is over. Mid-game it is refused with a hint and nothing changes.
    pub fn swap_sides(&mut self) {
        let in_progress = !self.sans.is_empty() && game_over(&self.pos, self.player).is_none();
        if in_progress {
            self.hint = "Game in progress. Press N for a new game, then S to swap sides.".into();
        } else {
            *self = Game::new_as(self.player.other());
            write_state(self);
        }
    }

    pub fn play(&mut self, m: &Move) {
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
        while let Some((p, l)) = self.history.pop() {
            self.pos = p;
            self.last = l;
            self.sans.pop();
            if self.pos.turn() == self.player {
                break;
            }
        }
        self.selected = None;
        self.target = None;
        write_state(self);
    }
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
    // One of my pieces: select it
    if g.pos.board().piece_at(s).map(|p| p.color) == Some(g.player) {
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
    let mine: Vec<Square> = g.pos.board().by_color(g.player).into_iter().collect();
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
    match game_over(pos, g.player) {
        Some(s) => format!("game over: {s}"),
        None => format!(
            "{} ({})",
            if pos.turn() == Side::White {
                "white"
            } else {
                "black"
            },
            if pos.turn() == g.player {
                "player"
            } else {
                "AI"
            }
        ),
    }
}

// The file an AI session reads every turn
pub fn write_state(g: &Game) {
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
    let text = format!(
        "turn: {turn}\nfen: {fen}\ncheck: {}\nmoves: {}\nnote: {}\nlegal: {}\n\n{board}",
        pos.is_check(),
        move_list(&g.sans),
        g.note,
        legal.join(" "),
    );
    let dir = live_dir();
    let _ = fs::create_dir_all(&dir);
    let tmp = dir.join("state.tmp");
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(tmp, dir.join("state.txt"));
    }
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
    fn s_mid_game_is_refused_and_only_works_between_games() {
        setup();
        // a game in progress: refused, nothing changes
        let mut g = from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        g.sans.push("e4".into());
        g.swap_sides();
        assert_eq!(g.player, Side::White);
        assert_eq!(g.sans.len(), 1, "the game is not restarted");
        assert!(g.hint.contains("Press N"), "{}", g.hint);
        // a fresh game: swaps, and the AI (white) opens
        let mut fresh = Game::new();
        fresh.swap_sides();
        assert_eq!(fresh.player, Side::Black);
        assert!(fresh.hint.is_empty());
        // a finished game: swaps
        let mut over = from_fen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3");
        over.sans.push("Qh4#".into());
        over.swap_sides();
        assert_eq!(over.player, Side::Black);
        assert!(over.sans.is_empty());
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
}
