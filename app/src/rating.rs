//! Glicko-2 ratings for the person and for the AI, from game results. A game is one
//! rating period. The ratings live in ~/.side-chess/ratings.txt, with the names of the
//! games already counted so none is counted twice.
use crate::game::live_dir;
use std::{f64::consts::PI, fs, path::Path};

const SCALE: f64 = 173.7178; // Glicko-2's own scale against the usual rating numbers
const TAU: f64 = 0.5; // how fast volatility may change
const EPS: f64 = 1e-6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rating {
    pub rating: f64,
    pub rd: f64, // rating deviation: how unsure the rating is
    pub vol: f64,
    pub games: u32,
}

impl Default for Rating {
    fn default() -> Self {
        Rating {
            rating: 1500.0,
            rd: 350.0,
            vol: 0.06,
            games: 0,
        }
    }
}

fn g(phi: f64) -> f64 {
    1.0 / (1.0 + 3.0 * phi * phi / (PI * PI)).sqrt()
}

fn expect(mu: f64, mu_j: f64, phi_j: f64) -> f64 {
    1.0 / (1.0 + (-g(phi_j) * (mu - mu_j)).exp())
}

impl Rating {
    /// The new rating after playing `games`: (opponent, score) with 1, 0.5 or 0.
    pub fn after(self, games: &[(Rating, f64)]) -> Rating {
        let (mu, phi) = ((self.rating - 1500.0) / SCALE, self.rd / SCALE);
        if games.is_empty() {
            // no games: only the uncertainty grows
            let phi2 = (phi * phi + self.vol * self.vol).sqrt();
            return Rating {
                rd: phi2 * SCALE,
                ..self
            };
        }
        let opp = |o: &Rating| ((o.rating - 1500.0) / SCALE, o.rd / SCALE);
        let v = 1.0
            / games
                .iter()
                .map(|(o, _)| {
                    let (mj, pj) = opp(o);
                    let e = expect(mu, mj, pj);
                    g(pj).powi(2) * e * (1.0 - e)
                })
                .sum::<f64>();
        let sum: f64 = games
            .iter()
            .map(|(o, s)| {
                let (mj, pj) = opp(o);
                g(pj) * (s - expect(mu, mj, pj))
            })
            .sum();
        let delta = v * sum;
        // the new volatility, by the iterative (Illinois) method from the paper
        let a = (self.vol * self.vol).ln();
        let f = |x: f64| {
            let ex = x.exp();
            ex * (delta * delta - phi * phi - v - ex) / (2.0 * (phi * phi + v + ex).powi(2))
                - (x - a) / (TAU * TAU)
        };
        let mut big_a = a;
        let mut big_b = if delta * delta > phi * phi + v {
            (delta * delta - phi * phi - v).ln()
        } else {
            let mut k = 1.0;
            while f(a - k * TAU) < 0.0 {
                k += 1.0;
            }
            a - k * TAU
        };
        let (mut fa, mut fb) = (f(big_a), f(big_b));
        while (big_b - big_a).abs() > EPS {
            let c = big_a + (big_a - big_b) * fa / (fb - fa);
            let fc = f(c);
            if fc * fb <= 0.0 {
                big_a = big_b;
                fa = fb;
            } else {
                fa /= 2.0;
            }
            big_b = c;
            fb = fc;
        }
        let vol = (big_a / 2.0).exp();
        let phi_star = (phi * phi + vol * vol).sqrt();
        let phi_new = 1.0 / (1.0 / (phi_star * phi_star) + 1.0 / v).sqrt();
        let mu_new = mu + phi_new * phi_new * sum;
        Rating {
            rating: mu_new * SCALE + 1500.0,
            rd: phi_new * SCALE,
            vol,
            games: self.games + games.len() as u32,
        }
    }
}

/// Both ratings and the games already counted.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ratings {
    pub you: Rating,
    pub ai: Rating,
    pub counted: Vec<String>,
}

impl Ratings {
    pub fn to_text(&self) -> String {
        let line = |name: &str, r: &Rating| {
            format!(
                "{name} {:.2} {:.2} {:.6} {}\n",
                r.rating, r.rd, r.vol, r.games
            )
        };
        let mut s = line("you", &self.you) + &line("ai", &self.ai);
        for c in &self.counted {
            s.push_str(&format!("game {c}\n"));
        }
        s
    }

    pub fn from_text(text: &str) -> Ratings {
        let mut out = Ratings::default();
        for l in text.lines() {
            let w: Vec<&str> = l.split_whitespace().collect();
            let parse = |w: &[&str]| -> Option<Rating> {
                Some(Rating {
                    rating: w.get(1)?.parse().ok()?,
                    rd: w.get(2)?.parse().ok()?,
                    vol: w.get(3)?.parse().ok()?,
                    games: w.get(4)?.parse().ok()?,
                })
            };
            match w.first().copied() {
                Some("you") => out.you = parse(&w).unwrap_or_default(),
                Some("ai") => out.ai = parse(&w).unwrap_or_default(),
                Some("game") if w.len() == 2 => out.counted.push(w[1].to_string()),
                _ => {}
            }
        }
        out
    }

    /// Count one finished game, once. `score` is the person's result: 1, 0.5 or 0.
    /// Both sides are rated from their ratings before the game. Returns false if
    /// the game was already counted.
    pub fn record(&mut self, game: &str, score: f64) -> bool {
        if self.counted.iter().any(|c| c == game) {
            return false;
        }
        let (you, ai) = (self.you, self.ai);
        self.you = you.after(&[(ai, score)]);
        self.ai = ai.after(&[(you, 1.0 - score)]);
        self.counted.push(game.to_string());
        true
    }
}

fn path() -> std::path::PathBuf {
    live_dir().join("ratings.txt")
}

pub fn load() -> Ratings {
    fs::read_to_string(path())
        .map(|t| Ratings::from_text(&t))
        .unwrap_or_default()
}

/// Count a saved game's result, if it finished and has not been counted: the
/// person's side comes from the PGN's White and Black names.
pub fn record_game(pgn: &Path) {
    record_game_in(pgn, &path());
}

fn record_game_in(pgn: &Path, file: &Path) {
    let Ok(text) = fs::read_to_string(pgn) else {
        return;
    };
    let Ok(p) = crate::replay::parse_pgn(&text) else {
        return;
    };
    let me = p.player();
    let score = match (p.result.as_str(), me) {
        ("1-0", shakmaty::Color::White) | ("0-1", shakmaty::Color::Black) => 1.0,
        ("1-0", _) | ("0-1", _) => 0.0,
        ("1/2-1/2", _) => 0.5,
        _ => return, // not finished: no result to rate
    };
    let Some(name) = pgn.file_stem().map(|s| s.to_string_lossy().to_string()) else {
        return;
    };
    let mut r = fs::read_to_string(file)
        .map(|t| Ratings::from_text(&t))
        .unwrap_or_default();
    if r.record(&name, score) {
        if let Some(dir) = file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(file, r.to_text());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(rating: f64, rd: f64) -> Rating {
        Rating {
            rating,
            rd,
            vol: 0.06,
            games: 0,
        }
    }

    #[test]
    fn matches_the_worked_example_in_glickmans_paper() {
        let me = r(1500.0, 200.0);
        let n = me.after(&[
            (r(1400.0, 30.0), 1.0),
            (r(1550.0, 100.0), 0.0),
            (r(1700.0, 300.0), 0.0),
        ]);
        assert!((n.rating - 1464.06).abs() < 0.05, "{}", n.rating);
        assert!((n.rd - 151.52).abs() < 0.05, "{}", n.rd);
        assert!((n.vol - 0.05999).abs() < 0.0001, "{}", n.vol);
    }

    #[test]
    fn a_win_raises_you_and_lowers_the_other_and_certainty_grows() {
        let mut rs = Ratings::default();
        assert!(rs.record("a", 1.0));
        assert!(rs.you.rating > 1500.0 && rs.ai.rating < 1500.0);
        assert!(rs.you.rd < 350.0 && rs.ai.rd < 350.0);
        assert_eq!((rs.you.games, rs.ai.games), (1, 1));
        let after_win = rs.you.rating;
        rs.record("b", 0.0);
        assert!(rs.you.rating < after_win, "a loss lowers it");
    }

    #[test]
    fn a_game_counts_only_once_and_the_file_reads_back() {
        let mut rs = Ratings::default();
        assert!(rs.record("2026-11-23_09-10-11", 0.5));
        assert!(!rs.record("2026-11-23_09-10-11", 0.5));
        let back = Ratings::from_text(&rs.to_text());
        assert_eq!(back.counted, rs.counted);
        assert!((back.you.rating - rs.you.rating).abs() < 0.01);
        assert_eq!(back.ai.games, 1);
    }

    #[test]
    fn a_finished_pgn_is_counted_from_the_persons_side() {
        let dir = std::env::temp_dir().join(format!("side-chess-rate-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let f = dir.join("g1.pgn");
        fs::write(
            &f,
            "[White \"AI\"]\n[Black \"You\"]\n[Result \"0-1\"]\n\n1. e4 e5 0-1\n",
        )
        .unwrap();
        let file = dir.join("ratings.txt");
        record_game_in(&f, &file);
        record_game_in(&f, &file); // the same game again changes nothing
        let rs = Ratings::from_text(&fs::read_to_string(&file).unwrap());
        assert_eq!(rs.counted, ["g1"]);
        assert!(rs.you.rating > 1500.0, "the person (black) won");
        // an unfinished game is not rated
        let g2 = dir.join("g2.pgn");
        fs::write(&g2, "[White \"You\"]\n[Black \"AI\"]\n\n1. e4 e5 *\n").unwrap();
        record_game_in(&g2, &file);
        let rs = Ratings::from_text(&fs::read_to_string(&file).unwrap());
        assert_eq!(rs.counted, ["g1"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
