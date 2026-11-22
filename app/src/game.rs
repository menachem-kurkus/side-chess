//! The game: rules, moves typed or clicked by the player, and the two files that
//! connect the window to Claude (~/.side-chess/state.txt and claude_move.txt).
use shakmaty::{
    fen::Fen, san::San, Chess, Color as Side, EnPassantMode, File, Move, Position, Rank, Role,
    Square,
};
use std::{fs, path::PathBuf, str::FromStr};

pub struct Game {
    pub pos: Chess,
    pub history: Vec<(Chess, Option<(Square, Square)>)>,
    pub sans: Vec<String>,
    pub selected: Option<Square>,
    pub target: Option<Square>, // keyboard: square offered for the selected piece, played on Enter
    pub last: Option<(Square, Square)>,
    pub note: String,
}

impl Game {
    pub fn new() -> Self {
        Game {
            pos: Chess::default(),
            history: vec![],
            sans: vec![],
            selected: None,
            target: None,
            last: None,
            note: String::new(),
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
        write_state(self);
    }

    // Take back to the last time it was the player's move
    pub fn undo(&mut self) {
        while let Some((p, l)) = self.history.pop() {
            self.pos = p;
            self.last = l;
            self.sans.pop();
            if self.pos.turn() == Side::White {
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
    if g.pos.board().piece_at(s).map(|p| p.color) == Some(Side::White) {
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
    let mine: Vec<Square> = g.pos.board().by_color(Side::White).into_iter().collect();
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

pub fn game_over(pos: &Chess) -> Option<String> {
    if pos.is_checkmate() {
        Some(if pos.turn() == Side::White {
            "Checkmate. Claude wins.".into()
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

// The file Claude reads every turn
pub fn write_state(g: &Game) {
    let pos = &g.pos;
    let fen = Fen::from_position(pos.clone(), EnPassantMode::Legal).to_string();
    let turn = match (game_over(pos), pos.turn()) {
        (Some(s), _) => format!("game over: {s}"),
        (None, Side::White) => "white (player)".into(),
        (None, Side::Black) => "black (Claude)".into(),
    };
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

// Claude's reply: SAN (Nf6) or from-to (g8f6)
pub fn parse_move(pos: &Chess, s: &str) -> Option<Move> {
    if let Ok(san) = San::from_str(s) {
        if let Ok(m) = san.to_move(pos) {
            return Some(m);
        }
    }
    let s = s.to_lowercase();
    if s.len() >= 4 {
        let (from, to) = (&s[0..2], &s[2..4]);
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

pub fn check_claude_move(g: &mut Game) {
    let path = live_dir().join("claude_move.txt");
    let Ok(raw) = fs::read_to_string(&path) else {
        return;
    };
    let _ = fs::remove_file(&path);
    let s = raw.trim();
    if s.is_empty() || g.pos.turn() != Side::Black || game_over(&g.pos).is_some() {
        return;
    }
    match parse_move(&g.pos, s) {
        Some(m) => g.play(&m),
        None => {
            g.note =
                format!("Claude sent '{s}', which isn't legal here. Waiting for another move.");
            write_state(g);
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
    }

    #[test]
    fn game_over_detects_mate_stalemate_and_draws() {
        let mate = from_fen("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3");
        assert_eq!(
            game_over(&mate.pos).as_deref(),
            Some("Checkmate. Claude wins.")
        );
        let stale = from_fen("k7/2Q5/1K6/8/8/8/8/8 b - - 0 1");
        assert_eq!(
            game_over(&stale.pos).as_deref(),
            Some("Stalemate. It's a draw.")
        );
        let bare = from_fen("k7/8/1K6/8/8/8/8/8 w - - 0 1");
        assert!(game_over(&bare.pos).unwrap().starts_with("Draw."));
        assert!(game_over(&Chess::default()).is_none());
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
    fn pawns_promote_to_a_queen() {
        let g = from_fen("8/4P1k1/8/8/8/8/8/4K3 w - - 0 1");
        let m = legal_to(&g.pos, s("e7"), s("e8")).unwrap();
        assert_eq!(m.promotion(), Some(Role::Queen));
    }
}
