#![cfg_attr(windows, windows_subsystem = "windows")]
//! Side Chess: you play white in this window (S swaps sides); an AI session plays
//! the other side by reading ~/.side-chess/state.txt and writing its move to
//! ~/.side-chess/opponent_move.txt or the legacy claude_move.txt.
//!
//! Where things live:
//!   game.rs   rules, typed and clicked moves, the files shared with the AI
//!   draw.rs   what the window shows (layout)
//!   pack.rs   the look: colours and pieces from app/packs/<pack>/
//!   text.rs   text from a baked atlas
//!   canvas.rs drawing in plain memory (no graphics card: stays small)
//!   present.rs showing frames in the screen's own colours
//!   main.rs   the window, keys and mouse (this file)
mod canvas;
mod draw;
mod game;
mod native;
mod pack;
#[cfg_attr(not(target_os = "macos"), path = "present_other.rs")]
mod present;
mod rails;
mod save;
mod text;

use canvas::{Canvas, Img};
use draw::{promotion_at, square_at, View, H, SQ, W};
use game::{
    check_opponent_move, game_over, live_dir, pick_square, sq, write_state, Game, MoveStart,
};
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
            rails: Rails::load(),
            rail_wins: vec![],
            native_seen: None,
            preview: None,
            rails_away: false,
            prev_moves: 0,
            slide_from: None,
            pack_names: (0..PACKS.len())
                .map(|i| {
                    Theme::load(i)
                        .map(|t| t.name)
                        .unwrap_or_else(|_| PACKS[i].0.to_string())
                })
                .collect(),
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
        };
        rw.presenter.frame(w, h, scale, screen, |px| {
            let mut cv = Canvas::new(px, w, h, scale as f32);
            rails::draw_panel(&mut cv, &view, rw.panel, rw.hover);
        });
    }

    /// Keep the game on disk once it is over (see save.rs for the rules).
    fn autosave(&mut self) {
        if game_over(&self.g.pos, self.g.player).is_some() {
            save::save(&mut self.g);
        }
    }

    fn my_turn(&self) -> bool {
        self.g.pos.turn() == self.g.player && game_over(&self.g.pos, self.g.player).is_none()
    }

    fn click(&mut self) {
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
        let flip = self.g.player == Side::Black;
        let Some(clicked) = square_at(self.mouse.0, self.mouse.1, flip) else {
            return;
        };
        let g = &mut self.g;
        let started = g.selected.map(|from| g.try_move(from, clicked));
        if matches!(started, Some(MoveStart::Played | MoveStart::NeedPromotion)) {
            // played, or waiting for the piece to be picked
        } else if g.pos.board().piece_at(clicked).map(|p| p.color) == Some(g.player) {
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
                        'n' => {
                            save::save(&mut self.g);
                            self.g = Game::new_as(self.g.player);
                            write_state(&self.g);
                        }
                        // swap sides: only between games (refused mid-game: press N first)
                        's' => {
                            self.autosave();
                            self.g.swap_sides();
                        }
                        'u' => self.g.undo(),
                        't' if PACKS.len() > 1 => self.set_pack(self.pack + 1),
                        'p' => self.shot = true,
                        // K and L open, close or bring back the keys and look rails
                        'k' => {
                            self.rails.press_key(Panel::Keys);
                            self.rails_changed(el);
                        }
                        'l' => {
                            self.rails.press_key(Panel::Look);
                            self.rails_changed(el);
                        }
                        _ => {}
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
        };
        let Some(screen) = &self.screen else {
            return;
        };
        let (g, shot) = (&self.g, &mut self.shot);
        // The presenter hands the closure the pixels to draw into: the window's own
        // buffer where the system allows (Windows), else a frame it then hands over (Mac).
        presenter.frame(w, h, scale, screen, |px| {
            let mut cv = Canvas::new(px, w, h, scale as f32);
            draw::frame(&mut cv, g, &view);
            if *shot {
                *shot = false;
                let _ = fs::create_dir_all(live_dir());
                let _ = cv.save_png(&live_dir().join("screen.png"), screen.icc());
            }
        });
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
                if event.state == ElementState::Pressed && !event.repeat =>
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
        let shot_req = live_dir().join("shot");
        if shot_req.exists() {
            let _ = fs::remove_file(&shot_req);
            self.shot = true;
            self.dirty = true;
        }
        if self.g.pos.turn() != self.g.player && game_over(&self.g.pos, self.g.player).is_none() {
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
