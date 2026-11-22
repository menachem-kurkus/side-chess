#![cfg_attr(windows, windows_subsystem = "windows")]
//! Side Chess: you play white in this window; a Claude Code session plays black
//! by reading ~/.side-chess/state.txt and writing its move to
//! ~/.side-chess/claude_move.txt.
//!
//! Where things live:
//!   game.rs   rules, typed and clicked moves, the files shared with Claude
//!   draw.rs   what the window shows (layout)
//!   pack.rs   the look: colours and pieces from app/packs/<pack>/
//!   text.rs   text from a baked atlas
//!   canvas.rs drawing in plain memory (no graphics card: stays small)
//!   present.rs showing frames in the screen's own colours
//!   main.rs   the window, keys and mouse (this file)
mod canvas;
mod draw;
mod game;
mod pack;
#[cfg_attr(not(target_os = "macos"), path = "present_other.rs")]
mod present;
mod text;

use canvas::{Canvas, Img};
use draw::{square_at, View, H, SQ, W};
use game::{check_claude_move, game_over, legal_to, live_dir, pick_square, sq, write_state, Game};
use pack::{load_pack, load_pack_for, saved_pack, Theme, PACKS};
use present::{Presenter, Screen};
use shakmaty::{fen::Fen, Chess, Color as Side, Position};
use std::{
    fs,
    rc::Rc,
    time::{Duration, Instant},
};
use text::Txt;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

/// How often the app looks for Claude's move and a screenshot request.
const POLL: Duration = Duration::from_millis(250);

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
        }
    }

    fn my_turn(&self) -> bool {
        self.g.pos.turn() == Side::White && game_over(&self.g.pos).is_none()
    }

    fn click(&mut self) {
        if !self.my_turn() {
            return;
        }
        let Some(clicked) = square_at(self.mouse.0, self.mouse.1) else {
            return;
        };
        let g = &mut self.g;
        if let Some(m) = g.selected.and_then(|from| legal_to(&g.pos, from, clicked)) {
            g.play(&m);
        } else if g.pos.board().piece_at(clicked).map(|p| p.color) == Some(Side::White) {
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
        match key {
            Key::Named(NamedKey::Enter) => {
                if let (true, Some(from), Some(to)) =
                    (self.my_turn(), self.g.selected, self.g.target)
                {
                    if let Some(m) = legal_to(&self.g.pos, from, to) {
                        self.g.play(&m);
                    }
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
                            self.g = Game::new();
                            write_state(&self.g);
                        }
                        'u' => self.g.undo(),
                        't' if PACKS.len() > 1 => {
                            self.pack = (self.pack + 1) % PACKS.len();
                            self.screen = None; // the next redraw loads it for this screen
                            let _ = fs::create_dir_all(live_dir());
                            let _ = fs::write(live_dir().join("pack.txt"), PACKS[self.pack].0);
                            self.pack_shown_at = Some(Instant::now());
                        }
                        'p' => self.shot = true,
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
        if self.screen.as_ref() != Some(&screen) {
            (self.th, self.pieces) = load_pack_for(self.pack, &screen);
            self.screen = Some(screen);
            self.sized = None;
        }
        // Windows screens often run at 125% or 150%: redraw the pieces once at the
        // exact size they are shown, so a frame copies pixels instead of resampling.
        // (A Mac's 2x screen is close enough to the 128 px tiles; left as it was.)
        if cfg!(not(target_os = "macos")) && self.sized.as_ref().map(|s| s.0) != Some(scale) {
            let tile = ((SQ - 4.0) as f64 * scale).round().max(1.0) as usize;
            self.sized = Some((scale, tile, self.pieces.with_tiles_of(tile)));
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
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                let s = self.window.as_ref().map_or(1.0, |w| w.scale_factor());
                self.mouse = ((position.x / s) as f32, (position.y / s) as f32);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                self.click();
                self.dirty = true;
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                self.key(&event.logical_key, el);
            }
            WindowEvent::Resized(_)
            | WindowEvent::Moved(_) // maybe onto another screen
            | WindowEvent::ScaleFactorChanged { .. }
            | WindowEvent::Focused(_) => {
                self.dirty = true;
            }
            _ => {}
        }
        if self.dirty {
            self.dirty = false;
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    // Between events: look for Claude's move, a screenshot request, and time-based changes.
    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let before = (self.g.sans.len(), self.g.note.clone());
        check_claude_move(&mut self.g);
        if (self.g.sans.len(), self.g.note.clone()) != before {
            self.dirty = true;
        }
        let shot_req = live_dir().join("shot");
        if shot_req.exists() {
            let _ = fs::remove_file(&shot_req);
            self.shot = true;
            self.dirty = true;
        }
        if self.g.pos.turn() == Side::Black && game_over(&self.g.pos).is_none() {
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
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
        el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + POLL));
    }
}

fn main() {
    let el = EventLoop::new().expect("an event loop");
    let mut app = App::new();
    el.run_app(&mut app).expect("the app runs");
}
