//! Move ratings from Stockfish, if it is installed. Stockfish is a separate program
//! (free, open source, GPL-3.0) that Side Chess talks to over UCI, in a background
//! thread, so the window never waits for it and uses no memory for it while no game
//! is being replayed. The engine is stopped when the replay is left.
use shakmaty::{Color as Side, Square};
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
    str::FromStr,
    sync::mpsc::{channel, Receiver, Sender},
    thread::{self, JoinHandle},
};

/// How deep the engine looks at each position (a few tenths of a second each).
pub const DEPTH: u32 = 14;

/// Scores are clamped to this many hundredths of a pawn, so a mate counts as
/// "winning" without making every later swing look enormous.
pub const CAP: i32 = 1000;

/// What the engine thinks of one position.
#[derive(Clone, Debug, PartialEq)]
pub struct Eval {
    pub cp: i32,           // hundredths of a pawn, from white's side, clamped to +-CAP
    pub mate: Option<i32>, // moves to mate (positive: white mates), when there is one
    pub best: Option<(Square, Square)>, // the engine's move for the side to move
}

impl Eval {
    /// The score as shown: +0.3, -1.2, or #3 for mate in three.
    pub fn text(&self) -> String {
        match self.mate {
            Some(m) => format!("#{}", m.abs()),
            None => format!("{:+.1}", self.cp as f32 / 100.0),
        }
    }
}

/// How a move ranks by what it cost the player who made it (centipawns lost).
/// Thresholds: 50 an inaccuracy, 100 a mistake, 200 a blunder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grade {
    Fine,
    Inaccuracy,
    Mistake,
    Blunder,
}

pub const INACCURACY: i32 = 50;
pub const MISTAKE: i32 = 100;
pub const BLUNDER: i32 = 200;

/// What a move cost the side that made it: the score before it and after it,
/// both from white's side.
pub fn loss(before: &Eval, after: &Eval, mover: Side) -> i32 {
    let d = before.cp - after.cp;
    (if mover == Side::White { d } else { -d }).max(0)
}

pub fn grade(loss: i32) -> Grade {
    match loss {
        l if l >= BLUNDER => Grade::Blunder,
        l if l >= MISTAKE => Grade::Mistake,
        l if l >= INACCURACY => Grade::Inaccuracy,
        _ => Grade::Fine,
    }
}

impl Grade {
    pub fn mark(self) -> &'static str {
        match self {
            Grade::Fine => "",
            Grade::Inaccuracy => "?!",
            Grade::Mistake => "?",
            Grade::Blunder => "??",
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Grade::Fine => "fine",
            Grade::Inaccuracy => "inaccuracy",
            Grade::Mistake => "mistake",
            Grade::Blunder => "blunder",
        }
    }
}

/// Where a Stockfish can be found: next to the app's own files, then the PATH, then the
/// usual install folders (a window opened from the Dock has a short PATH).
/// `SIDE_CHESS_ENGINE` names one explicitly (used by the tests).
pub fn find_engine() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SIDE_CHESS_ENGINE") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let exe = if cfg!(windows) {
        "stockfish.exe"
    } else {
        "stockfish"
    };
    let mut dirs = vec![crate::game::live_dir().join("engine")];
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    for d in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/games",
        "/usr/bin",
    ] {
        dirs.push(d.into());
    }
    dirs.into_iter().map(|d| d.join(exe)).find(|p| p.is_file())
}

/// Read one engine move such as "e2e4" or "e7e8q" as its two squares.
fn uci_squares(s: &str) -> Option<(Square, Square)> {
    Some((
        Square::from_str(s.get(0..2)?).ok()?,
        Square::from_str(s.get(2..4)?).ok()?,
    ))
}

/// Tracks one search: the last score seen, then the move it ended on.
#[derive(Default)]
pub struct Search {
    cp: Option<i32>,
    mate: Option<i32>,
}

impl Search {
    /// Feed one line of engine output. `white_to_move` says whose side the engine
    /// scores from. Returns the finished Eval at the `bestmove` line.
    pub fn line(&mut self, line: &str, white_to_move: bool) -> Option<Eval> {
        let sign = if white_to_move { 1 } else { -1 };
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.first() {
            Some(&"info") => {
                if let Some(i) = words.iter().position(|w| *w == "score") {
                    let n = words.get(i + 2).and_then(|v| v.parse::<i32>().ok());
                    match (words.get(i + 1), n) {
                        (Some(&"cp"), Some(n)) => {
                            self.cp = Some((n * sign).clamp(-CAP, CAP));
                            self.mate = None;
                        }
                        (Some(&"mate"), Some(n)) => {
                            self.cp = Some(if n * sign > 0 { CAP } else { -CAP });
                            self.mate = Some(n * sign);
                        }
                        _ => {}
                    }
                }
                None
            }
            Some(&"bestmove") => {
                let eval = Eval {
                    cp: self.cp.unwrap_or(0),
                    mate: self.mate,
                    best: words.get(1).and_then(|m| uci_squares(m)),
                };
                *self = Search::default();
                Some(eval)
            }
            _ => None,
        }
    }
}

/// An engine running in the background, asked about a list of positions in order.
pub struct Engine {
    jobs: Option<Sender<Vec<String>>>,
    pub results: Receiver<(usize, Eval)>,
    handle: Option<JoinHandle<()>>,
}

impl Engine {
    /// Start the program at `path` and ask about every position (as FEN) in `fens`.
    /// Results arrive on `results` as (index, eval), in order.
    pub fn start(path: PathBuf, fens: Vec<String>) -> Engine {
        let (jobs, job_rx) = channel::<Vec<String>>();
        let (out, results) = channel();
        let handle = thread::spawn(move || run(path, job_rx, out));
        let _ = jobs.send(fens);
        Engine {
            jobs: Some(jobs),
            results,
            handle: Some(handle),
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.jobs = None; // the worker sees the channel close, stops, and kills the engine
                          // the worker may be in the middle of a position; it ends when that search does
        let _ = self.handle.take();
    }
}

fn run(path: PathBuf, jobs: Receiver<Vec<String>>, out: Sender<(usize, Eval)>) {
    let mut cmd = Command::new(&path);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // the app is a Windows GUI program: without this a console window flashes up
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let Ok(mut child) = cmd.spawn() else {
        return;
    };
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        return;
    };
    let mut lines = BufReader::new(stdout).lines();
    let mut send = |s: &str| writeln!(stdin, "{s}").and_then(|_| stdin.flush()).is_ok();
    let ready = send("uci")
        && send("setoption name Threads value 1")
        && send("setoption name Hash value 16")
        && send("isready");
    if !ready {
        return;
    }
    // wait until it says it is ready
    loop {
        match lines.next() {
            Some(Ok(l)) if l.trim() == "readyok" => break,
            Some(Ok(_)) => {}
            _ => return,
        }
    }
    'jobs: while let Ok(fens) = jobs.recv() {
        for (i, fen) in fens.iter().enumerate() {
            // a closed channel (the replay was left) ends the work between positions
            if matches!(
                jobs.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Disconnected)
            ) {
                break 'jobs;
            }
            let white = fen.split(' ').nth(1) != Some("b");
            if !(send(&format!("position fen {fen}")) && send(&format!("go depth {DEPTH}"))) {
                break 'jobs;
            }
            let mut search = Search::default();
            loop {
                match lines.next() {
                    Some(Ok(l)) => {
                        if let Some(e) = search.line(&l, white) {
                            if out.send((i, e)).is_err() {
                                break 'jobs;
                            }
                            break;
                        }
                    }
                    _ => break 'jobs,
                }
            }
        }
    }
    let _ = send("quit");
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(cp: i32) -> Eval {
        Eval {
            cp,
            mate: None,
            best: None,
        }
    }

    #[test]
    fn scores_are_turned_to_white_and_the_best_move_is_read() {
        let mut s = Search::default();
        assert!(s.line("info depth 3 score cp 10 pv e7e5", false).is_none());
        assert!(s
            .line("info depth 14 score cp 40 nodes 9 pv e7e5 g1f3", false)
            .is_none());
        let r = s.line("bestmove e7e5 ponder g1f3", false).unwrap();
        assert_eq!(r.cp, -40, "black to move: +40 for black is -40 for white");
        assert_eq!(r.best, Some(("e7".parse().unwrap(), "e5".parse().unwrap())));
        assert_eq!(r.text(), "-0.4");
    }

    #[test]
    fn mate_scores_count_as_winning_and_are_named() {
        let mut s = Search::default();
        s.line("info depth 9 score mate 3 pv d1h5", true);
        let r = s.line("bestmove d1h5", true).unwrap();
        assert_eq!((r.cp, r.mate, r.text()), (CAP, Some(3), "#3".to_string()));
        s.line("info depth 9 score mate -2 pv a1a2", true);
        let r = s.line("bestmove a1a2", true).unwrap();
        assert_eq!((r.cp, r.mate), (-CAP, Some(-2)));
    }

    #[test]
    fn a_move_is_graded_by_what_it_cost_the_player() {
        // white moves and the score falls from +0.30 to -0.90: a 120 centipawn loss
        let l = loss(&e(30), &e(-90), Side::White);
        assert_eq!((l, grade(l)), (120, Grade::Mistake));
        // black moves and the score rises (good for white): the same swing costs black
        assert_eq!(loss(&e(-90), &e(30), Side::Black), 120);
        // a move that improves the player's score costs nothing
        assert_eq!(loss(&e(30), &e(80), Side::White), 0);
        assert_eq!(grade(49), Grade::Fine);
        assert_eq!(grade(50), Grade::Inaccuracy);
        assert_eq!(grade(100), Grade::Mistake);
        assert_eq!(grade(200), Grade::Blunder);
        assert_eq!(Grade::Blunder.mark(), "??");
    }

    #[cfg(unix)]
    #[test]
    fn a_stand_in_engine_answers_for_every_position() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("side-chess-engine-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let script = dir.join("fake-engine");
        std::fs::write(
            &script,
            "#!/bin/sh\nwhile read cmd rest; do\ncase \"$cmd\" in\nuci) echo 'id name Fake'; echo uciok;;\nisready) echo readyok;;\ngo) echo 'info depth 14 score cp 25 pv e2e4'; echo 'bestmove e2e4';;\nquit) exit 0;;\nesac\ndone\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let fens = vec![
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".to_string(),
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1".to_string(),
        ];
        let engine = Engine::start(script, fens);
        let first = engine
            .results
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let second = engine
            .results
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert_eq!((first.0, first.1.cp), (0, 25));
        assert_eq!(
            (second.0, second.1.cp),
            (1, -25),
            "black to move: turned to white"
        );
        drop(engine);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
