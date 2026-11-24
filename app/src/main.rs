#![cfg_attr(windows, windows_subsystem = "windows")]
//! Side Chess: you play white in this window (S swaps sides); an AI session plays
//! the other side by reading ~/.side-chess/state.txt and writing its move to
//! ~/.side-chess/opponent_move.txt or the legacy claude_move.txt.
//!
//! Where things live:
//!   game.rs   rules, typed and clicked moves, the files shared with the AI
//!   home.rs   the home screen and its Keybinds and Look pages
//!   keys.rs   which key does what (the person can change them)
//!   resources.rs what the app is using, for the home screen
//!   draw.rs   what the window shows (layout)
//!   pack.rs   the look: colours and pieces from app/packs/<pack>/
//!   text.rs   text from a baked atlas
//!   canvas.rs drawing in plain memory (no graphics card: stays small)
//!   present.rs showing frames in the screen's own colours
//!   main.rs   the window, keys and mouse (this file)
mod canvas;
mod draw;
mod engine;
mod game;
mod home;
mod keys;
mod native;
mod pack;
#[cfg_attr(not(target_os = "macos"), path = "present_other.rs")]
mod present;
mod rails;
mod rating;
mod replay;
mod resources;
mod save;
mod score;
mod text;
mod threats;

use canvas::{Canvas, Color, Img};
use draw::{menu_button_at, promotion_at, square_at, View, H, SQ, W};
use game::{check_opponent_move, live_dir, pick_square, sq, write_state, Game, MoveStart};
use home::{Info, Item, KeysHit, PacksHit, Page};
use keys::{Action, Keymap, ACTIONS};
use pack::{
    load_look_for, load_pack, piece_sets, save_pieces, saved_pack, saved_pieces, Theme, PACKS,
};
use present::{Presenter, Screen};
use rails::{Hit, Mode, Panel, Rails, LEN, TOP};
use shakmaty::{fen::Fen, Chess, Color as Side, Position, Role};
use std::{
    fs,
    rc::Rc,
    time::{Duration, Instant},
};
use text::Txt;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

/// How often the app looks for the opponent's move and a screenshot request.
const POLL: Duration = Duration::from_millis(250);

/// How long a moved piece glides to its square.
const SLIDE: Duration = Duration::from_millis(150);

/// One of the thin side windows (see rails.rs): its own window, docked beside the board.
struct RailWin {
    panel: Panel,
    window: Rc<Window>,
    presenter: Presenter,
    mouse: (f32, f32),  // the pointer in this window
    hover: Option<Hit>, // the button under it
}

struct App {
    window: Option<Rc<Window>>,
    presenter: Option<Presenter>,
    screen: Option<Screen>, // the screen the pack's colours were converted for
    g: Game,
    pack: usize,
    th: Theme,
    pieces: Img,
    sized: Option<(f64, usize, Img)>, // the pieces redrawn for the screen scale: (scale, tile px, image)
    txt: Txt,
    kb_file: Option<u32>,
    kb_rank: Option<u32>,
    mouse: (f32, f32),
    mods: ModifiersState,
    start: Instant,
    pack_shown_at: Option<Instant>,
    dots: usize,
    dirty: bool,
    shot: bool,
    replay: replay::State,
    arrows: bool, // X: threat arrows are on
    rails: Rails,
    rail_wins: Vec<RailWin>,
    preview: Option<usize>, // a look the pointer is on in the Look list, shown until it leaves
    rails_away: bool,       // the rails are out of sight because the board is minimised
    prev_moves: usize,      // how many moves the last frame had
    slide_from: Option<Instant>, // when the last move's slide began
    native_seen: Option<native::Native>, // the system colours the native look was last built from
    pack_names: Vec<String>,
    piece_names: Vec<String>,     // the piece sets in the Pieces list
    pieces_choice: Option<usize>, // a piece set picked on its own; None follows the colours
    preview_pieces: Option<Option<usize>>, // a pieces row the pointer is on (None inside: "same as colours")
    page: Page,        // the home screen, the board, or one of the home screen's pages
    info: Info,        // what the home screen shows (score, memory...)
    home_focus: usize, // the thing to do the keyboard or pointer is on
    keys: Keymap,      // which key does what
    keys_wait: Option<usize>, // the Keybinds row waiting for its new key
    keys_msg: String,  // why the last key was refused
    keys_hover: Option<KeysHit>,
    packs_hover: Option<PacksHit>,
    swatches: Vec<[Color; 3]>, // each look's light square, dark square and accent, for the packs page
    redraw_ms: f32,            // how long the last frame took to draw
    live_at: Instant,          // when the home screen's live numbers were last read
    replay_from_home: bool, // the replay list was opened from the home screen: Esc goes back to it
    ai_side: Side,          // the side the person last played against the AI
}

impl App {
    fn new() -> Self {
        let pack = saved_pack();
        let (th, pieces) = load_pack(pack);
        let mut g = Game::new();
        // testing: CHESS_FEN starts from a given position
        if let Ok(fen) = std::env::var("CHESS_FEN") {
            if let Ok(Ok(p)) = fen
                .parse::<Fen>()
                .map(|f| f.into_position::<Chess>(shakmaty::CastlingMode::Standard))
            {
                g.pos = p;
            }
        }
        // testing: CHESS_SELECT=e2 starts with that piece selected (to see its move dots)
        if let Ok(sqr) = std::env::var("CHESS_SELECT") {
            g.selected = sqr.parse().ok();
        }
        let _ = fs::remove_file(live_dir().join("claude_move.txt"));
        let _ = fs::remove_file(live_dir().join("opponent_move.txt"));
        write_state(&g);
        let (mut pack_names, mut swatches) = (vec![], vec![]);
        for i in 0..PACKS.len() {
            match Theme::load(i) {
                Ok(t) => {
                    pack_names.push(t.name.clone());
                    swatches.push([t.light_square, t.dark_square, t.accent]);
                }
                Err(_) => {
                    pack_names.push(PACKS[i].0.to_string());
                    swatches.push([Color::rgba8(160, 160, 160, 255); 3]);
                }
            }
        }
        // testing: CHESS_PAGE=game|keys|packs starts there instead of on the home screen
        let page = match std::env::var("CHESS_PAGE").as_deref() {
            Ok("game") => Page::Game,
            Ok("keys") => Page::Keys,
            Ok("packs") => Page::Packs,
            _ => Page::Home,
        };
        App {
            window: None,
            presenter: None,
            screen: None,
            g,
            pack,
            th,
            pieces,
            sized: None,
            txt: Txt::load(),
            kb_file: None,
            kb_rank: None,
            mouse: (0.0, 0.0),
            mods: ModifiersState::empty(),
            start: Instant::now(),
            pack_shown_at: None,
            dots: 1,
            dirty: true,
            shot: false,
            replay: replay::State::default(),
            arrows: false,
            rails: Rails::load(),
            rail_wins: vec![],
            native_seen: None,
            preview: None,
            rails_away: false,
            prev_moves: 0,
            slide_from: None,
            pack_names,
            piece_names: piece_sets()
                .iter()
                .map(|&i| {
                    if PACKS[i].0 == pack::DEFAULT_PACK {
                        "Classic".to_string()
                    } else {
                        Theme::load(i)
                            .map(|t| t.name)
                            .unwrap_or_else(|_| PACKS[i].0.to_string())
                    }
                })
                .collect(),
            pieces_choice: saved_pieces(),
            preview_pieces: None,
            page,
            info: Info::load(),
            home_focus: 0,
            keys: Keymap::load(),
            keys_wait: None,
            keys_msg: String::new(),
            keys_hover: None,
            packs_hover: None,
            swatches,
            redraw_ms: 0.0,
            live_at: Instant::now(),
            replay_from_home: false,
            ai_side: Side::White,
        }
    }

    /// The small print under each thing to do on the home screen.
    fn home_subs(&self) -> [String; 5] {
        let moves = self.g.sans.len().div_ceil(2);
        let ai_playing = !self.g.two && !self.g.sans.is_empty() && self.g.over().is_none();
        let (me, other) = if self.g.player == Side::White {
            ("white", "black")
        } else {
            ("black", "white")
        };
        let ai = if self.g.two {
            "Start a game against Claude or Codex".to_string()
        } else if ai_playing {
            format!("Continue your game, move {moves}")
        } else if self.g.over().is_some() {
            "Your last game is over: open the board for a new one".to_string()
        } else {
            format!("You play {me}; Claude or Codex plays {other}")
        };
        let two = if self.g.two && self.g.over().is_none() && !self.g.sans.is_empty() {
            format!("Continue, move {moves}")
        } else if ai_playing {
            "Two people, one screen. Your AI game is saved".to_string()
        } else {
            "Two people, one screen: take turns on the board".to_string()
        };
        let plural = |n: u32| if n == 1 { "" } else { "s" };
        let replays = match (self.info.saved, self.info.stockfish) {
            (0, _) => "No saved games yet".to_string(),
            (n, true) => format!("{n} saved game{}, rated by Stockfish", plural(n)),
            (n, false) => format!("{n} saved game{}", plural(n)),
        };
        let keys = if self.keys.is_default() {
            format!(
                "New game {}, swap {}, take back {}, and more",
                self.keys.label_of(Action::New),
                self.keys.label_of(Action::Swap),
                self.keys.label_of(Action::Undo)
            )
        } else {
            "Your own shortcuts are set".to_string()
        };
        let packs = format!(
            "Now: {} ({} looks)",
            self.pack_names.get(self.pack).map_or("", |s| s.as_str()),
            self.pack_names.len()
        );
        [ai, two, replays, keys, packs]
    }

    /// Go to the home screen, reading the score and the rest again.
    fn go_home(&mut self) {
        self.replay.leave();
        self.replay_from_home = false;
        let redraw_ms = self.redraw_ms;
        self.info = Info::load();
        self.info.redraw_ms = redraw_ms;
        self.live_at = Instant::now();
        self.page = Page::Home;
        self.keys_wait = None;
        self.keys_msg.clear();
        self.dirty = true;
    }

    /// Do one of the five things on the home screen.
    fn activate(&mut self, item: Item) {
        match item {
            Item::PlayAi => {
                if self.g.two {
                    // the in-person game is kept (saved if long enough); a new one against the AI
                    save::save(&mut self.g);
                    self.g = Game::new_as(self.ai_side);
                    let _ = fs::remove_file(live_dir().join("claude_move.txt"));
                    let _ = fs::remove_file(live_dir().join("opponent_move.txt"));
                    write_state(&self.g);
                }
                self.page = Page::Game;
            }
            Item::InPerson => {
                if !self.g.two {
                    self.autosave();
                    save::save(&mut self.g);
                    self.ai_side = self.g.player;
                    self.g = Game::new_two();
                    write_state(&self.g);
                }
                self.page = Page::Game;
            }
            Item::Replays => {
                self.page = Page::Game;
                self.replay.open_list();
                self.replay_from_home = true;
            }
            Item::Keybinds => {
                self.keys_wait = None;
                self.keys_msg.clear();
                self.page = Page::Keys;
            }
            Item::Packs => self.page = Page::Packs,
        }
        self.dirty = true;
    }

    /// A click on the home screen or one of its pages.
    fn page_click(&mut self) {
        let (x, y) = self.mouse;
        match self.page {
            Page::Home => {
                if let Some(i) = home::item_at(x, y) {
                    self.home_focus = i;
                    self.activate(home::ITEMS[i]);
                }
            }
            Page::Keys => match home::keys_hit(x, y) {
                Some(KeysHit::Back) => self.go_home(),
                Some(KeysHit::Reset) => {
                    self.keys = Keymap::defaults();
                    self.keys.save();
                    self.keys_wait = None;
                    self.keys_msg.clear();
                }
                Some(KeysHit::Row(i)) => {
                    self.keys_wait = if self.keys_wait == Some(i) {
                        None
                    } else {
                        Some(i)
                    };
                    self.keys_msg.clear();
                }
                None => {
                    self.keys_wait = None;
                    self.keys_msg.clear();
                }
            },
            Page::Packs => {
                match home::packs_hit(x, y, self.pack_names.len(), self.piece_names.len() + 1) {
                    Some(PacksHit::Back) => self.go_home(),
                    Some(PacksHit::Colours(i)) => self.set_pack(i),
                    Some(PacksHit::Pieces(row)) => self.set_pieces(Self::row_choice(row)),
                    None => {}
                }
            }
            Page::Game => {}
        }
        self.dirty = true;
    }

    /// The pointer moved over the home screen or one of its pages: light up what it is on.
    fn page_hover(&mut self) {
        let (x, y) = self.mouse;
        match self.page {
            Page::Home => {
                if let Some(i) = home::item_at(x, y) {
                    if i != self.home_focus {
                        self.home_focus = i;
                        self.dirty = true;
                    }
                }
            }
            Page::Keys => {
                let hover = home::keys_hit(x, y);
                if hover != self.keys_hover {
                    self.keys_hover = hover;
                    self.dirty = true;
                }
            }
            Page::Packs => {
                let hover =
                    home::packs_hit(x, y, self.pack_names.len(), self.piece_names.len() + 1);
                if hover != self.packs_hover {
                    self.packs_hover = hover;
                    self.dirty = true;
                }
            }
            Page::Game => {}
        }
    }

    /// Keys on the home screen or one of its pages.
    fn page_key(&mut self, key: &Key, el: &ActiveEventLoop) {
        let quit =
            self.mods.super_key() || (cfg!(not(target_os = "macos")) && self.mods.control_key());
        if let Key::Character(s) = key {
            if quit && matches!(s.to_lowercase().as_str(), "q" | "w") {
                el.exit();
                return;
            }
        }
        let n = home::ITEMS.len();
        match self.page {
            Page::Home => match key {
                Key::Named(NamedKey::ArrowDown) => self.home_focus = (self.home_focus + 1) % n,
                Key::Named(NamedKey::ArrowUp) => self.home_focus = (self.home_focus + n - 1) % n,
                Key::Named(NamedKey::Enter) => self.activate(home::ITEMS[self.home_focus]),
                // back to the board, where you left it
                Key::Named(NamedKey::Escape) => self.page = Page::Game,
                Key::Character(s) if !quit => {
                    if s.chars().next().and_then(|c| self.keys.action(c)) == Some(Action::Menu) {
                        self.page = Page::Game;
                    }
                }
                _ => {}
            },
            Page::Keys => match (key, self.keys_wait) {
                (Key::Named(NamedKey::Escape), Some(_)) => {
                    self.keys_wait = None;
                    self.keys_msg.clear();
                }
                (Key::Named(NamedKey::Escape), None) => self.go_home(),
                (Key::Character(s), Some(row)) if !quit => {
                    if let Some(c) = s.chars().next() {
                        match self.keys.set(ACTIONS[row], c) {
                            Ok(()) => {
                                self.keys.save();
                                self.keys_wait = None;
                                self.keys_msg.clear();
                            }
                            Err(why) => self.keys_msg = why,
                        }
                    }
                }
                _ => {}
            },
            Page::Packs => {
                if matches!(key, Key::Named(NamedKey::Escape)) {
                    self.go_home();
                }
            }
            Page::Game => {}
        }
        self.dirty = true;
    }

    /// Do what a shortcut key stands for.
    fn do_action(&mut self, a: Action, el: &ActiveEventLoop) {
        match a {
            Action::New => {
                save::save(&mut self.g);
                self.g = if self.g.two {
                    Game::new_two()
                } else {
                    Game::new_as(self.g.player)
                };
                write_state(&self.g);
            }
            // swap sides: only between games (refused mid-game: press New game first)
            Action::Swap => {
                self.autosave();
                self.g.swap_sides();
            }
            Action::Undo => self.g.undo(),
            Action::Look if PACKS.len() > 1 => self.set_pack(self.pack + 1),
            Action::Look => {}
            Action::Shot => self.shot = true,
            // the list of saved games
            Action::Replays => {
                self.replay.open_list();
                self.replay_from_home = false;
            }
            // arrows from every piece to what it attacks
            Action::Arrows => self.arrows = !self.arrows,
            Action::KeysPanel => {
                self.rails.press_key(Panel::Keys);
                self.rails_changed(el);
            }
            Action::LookPanel => {
                self.rails.press_key(Panel::Look);
                self.rails_changed(el);
            }
            Action::Menu => self.go_home(),
        }
    }

    /// The look on screen: the one under the pointer in the Look list, else the chosen one.
    fn shown_pack(&self) -> usize {
        self.preview.unwrap_or(self.pack)
    }

    /// The piece set on screen: the one the pointer is on in the Pieces list, else the one
    /// picked on its own, else the colours' own.
    fn shown_pieces(&self) -> usize {
        self.preview_pieces
            .unwrap_or(self.pieces_choice)
            .unwrap_or_else(|| self.shown_pack())
    }

    /// The row of the Pieces list that is in use (0 is "same as colours").
    fn pieces_row(&self) -> usize {
        self.pieces_choice
            .and_then(|c| piece_sets().iter().position(|&s| s == c))
            .map_or(0, |k| k + 1)
    }

    /// What a row of the Pieces list stands for.
    fn row_choice(row: usize) -> Option<usize> {
        row.checked_sub(1)
            .and_then(|k| piece_sets().get(k).copied())
    }

    /// Pick a piece set on its own (None: follow the colours again) and remember it.
    fn set_pieces(&mut self, choice: Option<usize>) {
        self.pieces_choice = choice;
        self.preview_pieces = None;
        self.screen = None;
        save_pieces(choice);
        self.dirty = true;
    }

    /// Show a piece set while the pointer is on its row in the list (None when it leaves).
    fn preview_pieces_row(&mut self, row: Option<usize>) {
        let want = row
            .map(Self::row_choice)
            .filter(|c| *c != self.pieces_choice);
        if want != self.preview_pieces {
            self.preview_pieces = want;
            self.screen = None;
            self.dirty = true;
        }
    }

    /// Choose a look (T, or a click in the Look list) and remember it.
    fn set_pack(&mut self, i: usize) {
        self.pack = i % PACKS.len();
        self.preview = None;
        self.screen = None; // the next redraw loads it for this screen
        let _ = fs::create_dir_all(live_dir());
        let _ = fs::write(live_dir().join("pack.txt"), PACKS[self.pack].0);
        self.pack_shown_at = Some(Instant::now());
        self.dirty = true;
    }

    /// Show a look while the pointer is on it in the list (None when it leaves).
    fn preview_pack(&mut self, i: Option<usize>) {
        let i = i.filter(|i| *i != self.pack);
        if i != self.preview {
            self.preview = i;
            self.screen = None;
            self.dirty = true;
        }
    }

    /// A rail was opened, closed or hidden: save it and make the windows match.
    fn rails_changed(&mut self, el: &ActiveEventLoop) {
        self.rails.save();
        self.sync_rails(el);
        self.dirty = true;
    }

    /// Open a window for each rail on show, close the hidden ones, size the rest and
    /// put them all in a row beside the board.
    fn sync_rails(&mut self, el: &ActiveEventLoop) {
        if self.window.is_none() {
            return;
        }
        for p in [Panel::Keys, Panel::Look] {
            let mode = self.rails.mode(p);
            let have = self.rail_wins.iter().position(|r| r.panel == p);
            let w = Rails::width_of(p, mode) as f64;
            match (mode, have) {
                (Mode::Hidden, Some(i)) => {
                    self.rail_wins.remove(i);
                }
                (Mode::Hidden, None) => {}
                (_, Some(i)) => {
                    let _ = self.rail_wins[i]
                        .window
                        .request_inner_size(LogicalSize::new(w, LEN as f64));
                }
                (_, None) => {
                    let attrs = Window::default_attributes()
                        .with_title("Side Chess")
                        .with_decorations(false)
                        .with_resizable(false)
                        .with_active(false)
                        .with_inner_size(LogicalSize::new(w, LEN as f64));
                    // a rail is part of the board's window group: no taskbar entry of its own
                    #[cfg(windows)]
                    let attrs = {
                        use winit::platform::windows::WindowAttributesExtWindows;
                        attrs.with_skip_taskbar(true)
                    };
                    // Not a child of the board window: macOS moves a whole group of windows
                    // together to keep a child on screen, and a rail must never move the board.
                    // The rails follow the board instead (see place_rails).
                    if let Ok(window) = el.create_window(attrs) {
                        let window = Rc::new(window);
                        let presenter = Presenter::new(&window);
                        window.request_redraw();
                        self.rail_wins.push(RailWin {
                            panel: p,
                            window,
                            presenter,
                            mouse: (0.0, 0.0),
                            hover: None,
                        });
                    }
                }
            }
        }
        for r in &mut self.rail_wins {
            r.hover = None;
        }
        self.preview_pack(None); // a closed or folded list can't keep a look on show
        self.preview_pieces_row(None);
        self.place_rails();
    }

    /// Line the rail windows up beside the board window.
    fn place_rails(&self) {
        let Some(main) = &self.window else {
            return;
        };
        let Ok(at) = main.inner_position() else {
            return;
        };
        // In screen points, which mean the same on every display (pixels do not: the
        // rails may be on a screen with a different scale from the board's).
        let at: LogicalPosition<f64> = at.to_logical(main.scale_factor());
        for (p, x, _) in self.rails.layout() {
            if let Some(r) = self.rail_wins.iter().find(|r| r.panel == p) {
                r.window
                    .set_outer_position(LogicalPosition::new(at.x + x as f64, at.y + TOP as f64));
            }
        }
    }

    fn request_redraws(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
        for r in &self.rail_wins {
            r.window.request_redraw();
        }
    }

    /// Draw one rail window.
    fn redraw_rail(&self, i: usize) {
        let Some(screen) = &self.screen else {
            return; // the board window has not worked out the screen's colours yet
        };
        let rw = &self.rail_wins[i];
        let size = rw.window.inner_size();
        let (w, h) = (size.width as usize, size.height as usize);
        if w == 0 || h == 0 {
            return;
        }
        let scale = rw.window.scale_factor();
        let view = View {
            th: &self.th,
            pieces: self.sized.as_ref().map_or(&self.pieces, |s| &s.2),
            tile: self.sized.as_ref().map_or(128.0, |s| s.1 as f32),
            txt: &self.txt,
            kb_file: None,
            kb_rank: None,
            thinking_dots: self.dots,
            show_pack_name: false,
            rails: &self.rails,
            pack: self.pack,
            pack_names: &self.pack_names,
            piece_names: &self.piece_names,
            pieces_row: self.pieces_row(),
            slide: None,
            arrows: false,
            replay: None,
            keys: &self.keys,
        };
        rw.presenter.frame(w, h, scale, screen, |px| {
            let mut cv = Canvas::new(px, w, h, scale as f32);
            rails::draw_panel(&mut cv, &view, rw.panel, rw.hover);
        });
    }

    /// Keep the game on disk once it is over (see save.rs for the rules).
    fn autosave(&mut self) {
        if self.g.over().is_some() {
            let saved = save::save(&mut self.g);
            // the result moves both ratings, once per game
            if let (Some(path), false) = (saved, self.g.rated) {
                rating::record_game(&path);
                self.g.rated = true;
            }
        }
    }

    fn my_turn(&self) -> bool {
        self.g.person_to_move()
    }

    fn click(&mut self) {
        if self.page != Page::Game {
            self.page_click();
            return;
        }
        if self.replay.active() {
            return;
        }
        if menu_button_at(self.mouse.0, self.mouse.1) {
            self.go_home();
            return;
        }
        if self.g.promotion.is_some() {
            // the box asking which piece the pawn becomes: pick one, or click away to cancel
            match promotion_at(self.mouse.0, self.mouse.1) {
                Some(role) => {
                    self.g.promote(role);
                }
                None => self.g.cancel_promotion(),
            }
            return;
        }
        if !self.my_turn() {
            return;
        }
        // the board turns round for black against the AI; in person white stays at the bottom
        let flip = !self.g.two && self.g.player == Side::Black;
        let Some(clicked) = square_at(self.mouse.0, self.mouse.1, flip) else {
            return;
        };
        let g = &mut self.g;
        let started = g.selected.map(|from| g.try_move(from, clicked));
        if matches!(started, Some(MoveStart::Played | MoveStart::NeedPromotion)) {
            // played, or waiting for the piece to be picked
        } else if g.pos.board().piece_at(clicked).map(|p| p.color) == Some(g.mover()) {
            g.target = None;
            g.selected = if g.selected == Some(clicked) {
                None
            } else {
                Some(clicked)
            };
        } else {
            g.selected = None;
            g.target = None;
        }
    }

    fn key(&mut self, key: &Key, el: &ActiveEventLoop) {
        if self.page != Page::Game {
            self.page_key(key, el);
            return;
        }
        if self.replay.active() {
            self.replay_key(key, el);
            // a replay opened from the home screen goes back there when it is left
            if self.replay_from_home && !self.replay.active() {
                self.go_home();
            }
            self.dirty = true;
            return;
        }
        // While the box asks which piece a pawn becomes, Q R B N pick, Esc cancels
        if self.g.promotion.is_some() && !self.mods.super_key() && !self.mods.control_key() {
            match key {
                Key::Character(s) => {
                    let role = match s.chars().next().map(|c| c.to_ascii_lowercase()) {
                        Some('q') => Some(Role::Queen),
                        Some('r') => Some(Role::Rook),
                        Some('b') => Some(Role::Bishop),
                        Some('n') => Some(Role::Knight),
                        _ => None,
                    };
                    if let Some(role) = role {
                        self.g.promote(role);
                    }
                }
                Key::Named(NamedKey::Escape) => self.g.cancel_promotion(),
                _ => {}
            }
            self.dirty = true;
            return;
        }
        match key {
            Key::Named(NamedKey::Enter) => {
                if let (true, Some(from), Some(to)) =
                    (self.my_turn(), self.g.selected, self.g.target)
                {
                    self.g.try_move(from, to);
                }
            }
            Key::Named(NamedKey::Escape) => {
                self.g.selected = None;
                self.g.target = None;
                self.kb_file = None;
                self.kb_rank = None;
            }
            Key::Character(s) => {
                for ch in s.chars().map(|c| c.to_ascii_lowercase()) {
                    // Cmd+Q / Cmd+W on Mac, Ctrl+Q / Ctrl+W on Windows
                    if self.mods.super_key()
                        || (cfg!(not(target_os = "macos")) && self.mods.control_key())
                    {
                        if ch == 'q' || ch == 'w' {
                            el.exit();
                        }
                        continue;
                    }
                    match ch {
                        // a letter and a number, in either order, name a square
                        f @ 'a'..='h' => self.kb_file = Some(f as u32 - 'a' as u32),
                        r @ '1'..='8' => self.kb_rank = Some(r as u32 - '1' as u32),
                        // everything else is a shortcut the person can change (keys.rs)
                        other => {
                            if let Some(a) = self.keys.action(other) {
                                self.do_action(a, el);
                            }
                        }
                    }
                }
                if let (Some(f), Some(r)) = (self.kb_file, self.kb_rank) {
                    self.kb_file = None;
                    self.kb_rank = None;
                    if self.my_turn() {
                        pick_square(&mut self.g, sq(f, r));
                    }
                }
            }
            _ => return,
        }
        self.dirty = true;
    }

    /// Keys while the list of saved games or a replay is open. Nothing here reaches
    /// the live game or the AI's files.
    fn replay_key(&mut self, key: &Key, el: &ActiveEventLoop) {
        let quit =
            self.mods.super_key() || (cfg!(not(target_os = "macos")) && self.mods.control_key());
        if let Key::Character(s) = key {
            if quit && matches!(s.to_lowercase().as_str(), "q" | "w") {
                el.exit();
            }
        }
        if matches!(key, Key::Character(s) if s.eq_ignore_ascii_case("x")) && !quit {
            self.arrows = !self.arrows;
        }
        if let Some(run) = &mut self.replay.run {
            match key {
                Key::Named(NamedKey::ArrowLeft) => run.step(-1),
                Key::Named(NamedKey::ArrowRight) => run.step(1),
                Key::Named(NamedKey::Home) => run.goto(0),
                Key::Named(NamedKey::End) => run.goto(run.total()),
                // Up and Down: the previous and the next move the engine marked
                Key::Named(NamedKey::ArrowUp) => {
                    if let Some(i) = run.next_mark(-1) {
                        run.goto(i);
                    }
                }
                Key::Named(NamedKey::ArrowDown) => {
                    if let Some(i) = run.next_mark(1) {
                        run.goto(i);
                    }
                }
                Key::Named(NamedKey::Escape) => self.replay.leave(),
                _ => {}
            }
        } else {
            match key {
                Key::Named(NamedKey::ArrowUp) => {
                    self.replay.picker.iter_mut().for_each(|p| p.pick(-1))
                }
                Key::Named(NamedKey::ArrowDown) => {
                    self.replay.picker.iter_mut().for_each(|p| p.pick(1))
                }
                Key::Named(NamedKey::Enter) => self.replay.open_selected(),
                Key::Named(NamedKey::Escape) => self.replay.leave(),
                _ => {}
            }
        }
    }

    fn redraw(&mut self) {
        let (Some(window), Some(presenter)) = (&self.window, &self.presenter) else {
            return;
        };
        let size = window.inner_size();
        let (w, h) = (size.width as usize, size.height as usize);
        if w == 0 || h == 0 {
            return;
        }
        let scale = window.scale_factor();
        // New screen (or new pack): convert the pack's colours for this screen.
        let screen = presenter.screen();
        let screen_changed = self.screen.as_ref() != Some(&screen);
        if screen_changed {
            (self.th, self.pieces) = load_look_for(self.shown_pack(), self.shown_pieces(), &screen);
            self.screen = Some(screen);
            self.sized = None;
            if PACKS[self.shown_pack()].0 == "native" {
                self.native_seen = native::read();
            }
        }
        // The text too: redrawn once for this screen's scale, so a letter is a plain copy
        self.txt.fit(scale);
        // Windows screens often run at 125% or 150%: redraw the pieces once at the
        // exact size they are shown, so a frame copies pixels instead of resampling.
        // (A Mac's 2x screen is close enough to the 128 px tiles; left as it was.)
        if cfg!(not(target_os = "macos")) && self.sized.as_ref().map(|s| s.0) != Some(scale) {
            let tile = ((SQ - 4.0) as f64 * scale).round().max(1.0) as usize;
            self.sized = Some((scale, tile, self.pieces.with_tiles_of(tile)));
        }
        // A move just played slides its piece across for a moment
        let moves = self.g.sans.len();
        if moves == self.prev_moves + 1 {
            self.slide_from = Some(Instant::now());
        }
        self.prev_moves = moves;
        let slide = self
            .slide_from
            .map(|t| t.elapsed().as_secs_f32() / SLIDE.as_secs_f32())
            .filter(|t| *t < 1.0);
        if slide.is_none() {
            self.slide_from = None;
        }
        let view = View {
            th: &self.th,
            pieces: self.sized.as_ref().map_or(&self.pieces, |s| &s.2),
            tile: self.sized.as_ref().map_or(128.0, |s| s.1 as f32),
            txt: &self.txt,
            kb_file: self.kb_file,
            kb_rank: self.kb_rank,
            thinking_dots: self.dots,
            show_pack_name: self
                .pack_shown_at
                .is_some_and(|t| t.elapsed() < Duration::from_secs(2)),
            rails: &self.rails,
            pack: self.pack,
            pack_names: &self.pack_names,
            piece_names: &self.piece_names,
            pieces_row: self.pieces_row(),
            slide,
            arrows: self.arrows,
            replay: self.replay.active().then_some(&self.replay),
            keys: &self.keys,
        };
        let Some(screen) = &self.screen else {
            return;
        };
        let g = &self.g;
        let (page, subs) = (self.page, self.home_subs());
        let (info, keys) = (&self.info, &self.keys);
        let (th, txt) = (&self.th, &self.txt);
        let (names, swatches, piece_names) = (&self.pack_names, &self.swatches, &self.piece_names);
        let (pack, pieces_row, focus) = (self.pack, self.pieces_row(), self.home_focus);
        let (keys_wait, keys_msg) = (self.keys_wait, self.keys_msg.as_str());
        let (keys_hover, packs_hover) = (self.keys_hover, self.packs_hover);
        let shot = &mut self.shot;
        let drawn = Instant::now();
        // The presenter hands the closure the pixels to draw into: the window's own
        // buffer where the system allows (Windows), else a frame it then hands over (Mac).
        presenter.frame(w, h, scale, screen, |px| {
            let mut cv = Canvas::new(px, w, h, scale as f32);
            match page {
                Page::Game => draw::frame(&mut cv, g, &view),
                Page::Home => home::draw_home(&mut cv, th, txt, info, &subs, focus),
                Page::Keys => {
                    home::draw_keys(&mut cv, th, txt, keys, keys_wait, keys_msg, keys_hover)
                }
                Page::Packs => home::draw_packs(
                    &mut cv,
                    th,
                    txt,
                    names,
                    swatches,
                    piece_names,
                    pack,
                    pieces_row,
                    packs_hover,
                ),
            }
            if *shot {
                *shot = false;
                let _ = fs::create_dir_all(live_dir());
                let _ = cv.save_png(&live_dir().join("screen.png"), screen.icc());
            }
        });
        // the home screen shows how long a frame takes: the last one measured
        let ms = drawn.elapsed().as_secs_f32() * 1000.0;
        self.redraw_ms = ms;
        self.info.redraw_ms = ms;
        // the rails wait for the board to work out the screen's colours: draw them now
        if screen_changed {
            for r in &self.rail_wins {
                r.window.request_redraw();
            }
        }
        // while a piece is sliding, ask for the next frame straight away
        if slide.is_some() {
            window.request_redraw();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let mut attrs = Window::default_attributes()
            .with_title("Side Chess")
            .with_inner_size(LogicalSize::new(W as f64, H as f64))
            .with_resizable(false);
        // The taskbar / title bar icon (a Mac gets its Dock icon from the .app bundle).
        if cfg!(not(target_os = "macos")) {
            let icon = Img::from_png(include_bytes!("../icon/icon-128.png"));
            let rgba: Vec<u8> = icon.px.iter().flatten().copied().collect();
            if let Ok(icon) = winit::window::Icon::from_rgba(rgba, icon.w as u32, icon.h as u32) {
                #[cfg(windows)]
                {
                    use winit::platform::windows::WindowAttributesExtWindows;
                    attrs = attrs.with_taskbar_icon(Some(icon.clone()));
                }
                attrs = attrs.with_window_icon(Some(icon));
            }
        }
        let window = Rc::new(el.create_window(attrs).expect("a window"));
        self.presenter = Some(Presenter::new(&window));
        window.request_redraw();
        self.window = Some(window);
        self.sync_rails(el);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, ev: WindowEvent) {
        // which rail window this is for, if it is not the board window
        let rail = self.rail_wins.iter().position(|r| r.window.id() == id);
        match ev {
            WindowEvent::CloseRequested if rail.is_none() => el.exit(),
            WindowEvent::RedrawRequested => match rail {
                Some(i) => self.redraw_rail(i),
                None => self.redraw(),
            },
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::CursorMoved { position, .. } => match rail {
                Some(i) => {
                    let r = &mut self.rail_wins[i];
                    let s = r.window.scale_factor();
                    r.mouse = ((position.x / s) as f32, (position.y / s) as f32);
                    let hover = self.rails.hit(r.panel, r.mouse.0, r.mouse.1);
                    if hover != r.hover {
                        r.hover = hover;
                        self.dirty = true;
                    }
                    // pointing at a row shows it; leaving the lists puts the chosen look back
                    let (colours, pieces) = match hover {
                        Some(Hit::Colours(i)) => (Some(i), None),
                        Some(Hit::Pieces(i)) => (None, Some(i)),
                        _ => (None, None),
                    };
                    self.preview_pack(colours);
                    self.preview_pieces_row(pieces);
                }
                None => {
                    let s = self.window.as_ref().map_or(1.0, |w| w.scale_factor());
                    self.mouse = ((position.x / s) as f32, (position.y / s) as f32);
                    self.page_hover();
                }
            },
            WindowEvent::CursorLeft { .. } => {
                if let Some(i) = rail {
                    if self.rail_wins[i].hover.take().is_some() {
                        self.dirty = true;
                    }
                    self.preview_pack(None);
                    self.preview_pieces_row(None);
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                match rail {
                    Some(i) => {
                        let (p, (x, y)) = (self.rail_wins[i].panel, self.rail_wins[i].mouse);
                        let hit = self.rails.hit(p, x, y);
                        if let Some(Hit::Colours(k)) = hit {
                            self.set_pack(k); // a click keeps the colours the pointer is on
                        } else if let Some(Hit::Pieces(row)) = hit {
                            self.set_pieces(Self::row_choice(row)); // ... or the pieces
                        } else if self.rails.click(p, x, y) {
                            self.rails_changed(el);
                        }
                    }
                    None => self.click(),
                }
                self.dirty = true;
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && (!event.repeat || self.replay.active()) =>
            {
                self.key(&event.logical_key, el);
            }
            WindowEvent::Resized(_) if rail.is_some() => {
                self.place_rails(); // a rail changed width: keep it against the board
                self.dirty = true;
            }
            WindowEvent::Moved(_) if rail.is_none() => {
                self.place_rails(); // the board window moved (maybe onto another screen)
                self.dirty = true;
            }
            WindowEvent::Resized(_)
            | WindowEvent::Moved(_)
            | WindowEvent::ScaleFactorChanged { .. } => {
                self.dirty = true;
            }
            WindowEvent::Focused(focused) => {
                // the native look follows the system: light or dark, or a new accent colour
                if focused
                    && PACKS[self.shown_pack()].0 == "native"
                    && native::read() != self.native_seen
                {
                    self.screen = None;
                }
                self.dirty = true;
            }
            _ => {}
        }
        if self.dirty {
            self.dirty = false;
            self.request_redraws();
        }
    }

    // Quitting: keep the game being played
    fn exiting(&mut self, _el: &ActiveEventLoop) {
        save::save(&mut self.g);
    }

    // Between events: look for the opponent's move, a screenshot request, and time-based changes.
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.replay.run.as_mut().is_some_and(|r| r.poll()) {
            self.dirty = true; // the engine finished another position
        }
        let before = (self.g.sans.len(), self.g.note.clone());
        check_opponent_move(&mut self.g);
        self.autosave();
        if (self.g.sans.len(), self.g.note.clone()) != before {
            self.dirty = true;
        }
        // The rails go away with the board when it is minimised, and come back with it
        let minimised = self
            .window
            .as_ref()
            .and_then(|w| w.is_minimized())
            .unwrap_or(false);
        if minimised != self.rails_away {
            self.rails_away = minimised;
            for r in &self.rail_wins {
                r.window.set_visible(!minimised);
            }
        }
        if self.page == Page::Home && self.live_at.elapsed() >= Duration::from_secs(1) {
            self.live_at = Instant::now();
            self.info.refresh();
            self.dirty = true;
        }
        let shot_req = live_dir().join("shot");
        if shot_req.exists() {
            let _ = fs::remove_file(&shot_req);
            self.shot = true;
            self.dirty = true;
        }
        if !self.g.two && self.g.pos.turn() != self.g.player && self.g.over().is_none() {
            let dots = (self.start.elapsed().as_millis() / 500 % 3) as usize + 1;
            if dots != self.dots {
                self.dots = dots;
                self.dirty = true;
            }
        }
        if self
            .pack_shown_at
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(2))
        {
            self.pack_shown_at = None;
            self.dirty = true;
        }
        if self.dirty {
            self.dirty = false;
            self.request_redraws();
        }
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + POLL));
    }
}

/// On Linux the window system loads the keyboard library (libxkbcommon) at run
/// time and crashes without a clear message if it's missing, so check first.
#[cfg(target_os = "linux")]
fn check_linux_libraries() {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let (lib, pkg) = if wayland {
        (
            "libxkbcommon.so.0",
            "libxkbcommon0 (Debian, Ubuntu) or libxkbcommon (Fedora, Arch)",
        )
    } else {
        (
            "libxkbcommon-x11.so.0",
            "libxkbcommon-x11-0 (Debian, Ubuntu) or libxkbcommon-x11 (Fedora, Arch)",
        )
    };
    let mut dirs: Vec<std::path::PathBuf> = std::env::var_os("LD_LIBRARY_PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for d in [
        "/lib",
        "/lib64",
        "/usr/lib",
        "/usr/lib64",
        "/usr/local/lib",
        "/lib/x86_64-linux-gnu",
        "/usr/lib/x86_64-linux-gnu",
        "/lib/aarch64-linux-gnu",
        "/usr/lib/aarch64-linux-gnu",
    ] {
        dirs.push(d.into());
    }
    if dirs.iter().any(|d| d.join(lib).exists()) {
        return;
    }
    // Not in the usual folders: ask the loader's cache. If that can't be read
    // either, carry on rather than refuse a setup we don't know.
    let Ok(out) = std::process::Command::new("ldconfig").arg("-p").output() else {
        return;
    };
    if !out.status.success() || String::from_utf8_lossy(&out.stdout).contains(lib) {
        return;
    }
    eprintln!("Side Chess can't start: {lib} is missing. Install the package {pkg}, then open Side Chess again.");
    std::process::exit(1);
}

fn main() {
    #[cfg(target_os = "linux")]
    check_linux_libraries();
    let el = EventLoop::new().expect("an event loop");
    let mut app = App::new();
    el.run_app(&mut app).expect("the app runs");
}
