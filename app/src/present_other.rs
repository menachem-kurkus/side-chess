//! Showing frames on Windows (and anywhere that isn't a Mac): the frame goes into
//! the window through softbuffer. Windows converts nothing, so a `Screen` has
//! no colours to match; the same interface as the macOS `present.rs`.
use std::{cell::RefCell, num::NonZeroU32, rc::Rc};
use winit::window::Window;

pub struct Presenter {
    surface: RefCell<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
}

/// Nothing to match on this platform: pack colours are used as written.
#[derive(PartialEq)]
pub struct Screen;

impl Presenter {
    pub fn new(window: &Rc<Window>) -> Presenter {
        let ctx = softbuffer::Context::new(window.clone()).expect("a drawing context");
        let surface = softbuffer::Surface::new(&ctx, window.clone()).expect("a surface");
        Presenter {
            surface: RefCell::new(surface),
        }
    }

    pub fn screen(&self) -> Screen {
        Screen
    }

    /// Draws a frame with `paint` straight into the window's own buffer (0x00RRGGBB):
    /// no second copy of the screen.
    pub fn frame(
        &self,
        w: usize,
        h: usize,
        _scale: f64,
        _screen: &Screen,
        paint: impl FnOnce(&mut [u32]),
    ) {
        let (Some(nw), Some(nh)) = (NonZeroU32::new(w as u32), NonZeroU32::new(h as u32)) else {
            return;
        };
        let mut surface = self.surface.borrow_mut();
        if surface.resize(nw, nh).is_err() {
            return;
        }
        if let Ok(mut buf) = surface.buffer_mut() {
            paint(&mut buf);
            let _ = buf.present();
        };
    }
}

impl Screen {
    pub fn icc(&self) -> Option<Vec<u8>> {
        None
    }

    pub fn convert(&self, _rgba: &mut [[u8; 4]]) {}
}
