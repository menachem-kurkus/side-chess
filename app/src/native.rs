//! The native look: the colours the operating system itself uses, so the window looks
//! at home next to its other apps. Today that is macOS (its accent colour, light or dark
//! appearance, window background and text colours); elsewhere the `native` pack's own
//! plain system colours are used as they are.
use crate::canvas::Color;
use crate::pack::Theme;

/// What the system says its look is right now, as ordinary sRGB.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Native {
    pub dark: bool,
    pub background: [u8; 3],
    pub text: [u8; 3],
    pub muted: [u8; 3],
    pub accent: [u8; 3],
}

/// The system's colours right now, or None where they can't be read.
#[cfg(target_os = "macos")]
pub fn read() -> Option<Native> {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use std::ffi::{c_char, CStr};
    // SAFETY: plain property reads of AppKit objects, on the main thread (the app's
    // event loop), after the window system has started.
    unsafe {
        let app_cls = AnyClass::get(c"NSApplication")?;
        let app: Option<Retained<AnyObject>> = msg_send![app_cls, sharedApplication];
        let appearance: Option<Retained<AnyObject>> = msg_send![&*app?, effectiveAppearance];
        let appearance = appearance?;
        // dynamic colours follow the "current" appearance, so make it the app's
        let ap_cls = AnyClass::get(c"NSAppearance")?;
        let _: () = msg_send![ap_cls, setCurrentAppearance: &*appearance];
        let name: Option<Retained<AnyObject>> = msg_send![&*appearance, name];
        let utf8: *const c_char = msg_send![&*name?, UTF8String];
        let dark = !utf8.is_null() && CStr::from_ptr(utf8).to_string_lossy().contains("Dark");

        let color_cls = AnyClass::get(c"NSColor")?;
        let space_cls = AnyClass::get(c"NSColorSpace")?;
        let srgb: Option<Retained<AnyObject>> = msg_send![space_cls, sRGBColorSpace];
        let srgb = srgb?;
        let rgb = |sel_name: &str| -> Option<[u8; 3]> {
            let color: Option<Retained<AnyObject>> = match sel_name {
                "window" => msg_send![color_cls, windowBackgroundColor],
                "label" => msg_send![color_cls, labelColor],
                "secondary" => msg_send![color_cls, secondaryLabelColor],
                _ => msg_send![color_cls, controlAccentColor],
            };
            let converted: Option<Retained<AnyObject>> =
                msg_send![&*color?, colorUsingColorSpace: &*srgb];
            let c = converted?;
            let (mut r, mut g, mut b, mut a) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
            let _: () = msg_send![&*c, getRed: &mut r, green: &mut g, blue: &mut b, alpha: &mut a];
            let q = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            // a colour with some see-through (text colours are) is shown over the window colour
            Some([q(r), q(g), q(b)])
        };
        Some(Native {
            dark,
            background: rgb("window")?,
            text: rgb("label")?,
            muted: rgb("secondary")?,
            accent: rgb("accent")?,
        })
    }
}

#[cfg(not(target_os = "macos"))]
pub fn read() -> Option<Native> {
    None
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let m = |x: u8, y: u8| (x as f32 * t + y as f32 * (1.0 - t)).round() as u8;
    [m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2])]
}

fn col(c: [u8; 3], a: u8) -> Color {
    Color::rgba8(c[0], c[1], c[2], a)
}

/// Lays the system's colours over the native pack. Where they can't be read the
/// pack's own colours stay.
pub fn apply(th: &mut Theme) {
    let Some(n) = read() else {
        return;
    };
    let (white, black) = ([255u8; 3], [0u8; 3]);
    th.background = col(n.background, 255);
    th.text = col(n.text, 255);
    th.title = col(n.text, 255);
    th.muted = col(n.muted, 255);
    th.accent = col(n.accent, 255);
    th.status_your_move = col(n.accent, 255);
    th.status_waiting = col(n.muted, 255);
    th.status_over = col(n.accent, 255);
    th.banner = col(n.background, 255);
    th.banner_title = col(n.text, 255);
    th.banner_result = col(n.accent, 255);
    // the board: two tints of the accent, one light and one mid, readable under black and white pieces
    // The frame and header bands are a neutral grey, as in a Mac's own apps; the accent is the highlight.
    let border = mix(n.text, n.background, if n.dark { 0.22 } else { 0.7 });
    let (light, dark_sq) = if n.dark {
        (mix(n.accent, white, 0.30), mix(n.accent, white, 0.84))
    } else {
        (mix(n.accent, white, 0.13), mix(n.accent, white, 0.55))
    };
    th.light_square = col(light, 255);
    th.dark_square = col(dark_sq, 255);
    th.board_border = col(border, 255);
    th.last_from = col(n.accent, 0x33);
    th.last_to = col(n.accent, 0x66);
    th.last_outline = col(n.accent, 255);
    th.selected = col(n.accent, 0x55);
    th.target = col(n.accent, 0x77);
    th.target_outline = col(n.accent, 255);
    th.half_typed = col(n.accent, 0x33);
    th.move_dot = col(if n.dark { white } else { black }, 0x40);
    // the system's own red for a king in check
    let red = if n.dark { [255, 69, 58] } else { [215, 0, 21] };
    th.error = col(red, 255);
    th.check = col(red, 0x8c);
    th.check_outline = col(red, 255);
    th.shade = col(black, if n.dark { 0x99 } else { 0x59 });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixing_two_colours_goes_from_one_to_the_other() {
        assert_eq!(mix([0, 0, 0], [255, 255, 255], 1.0), [0, 0, 0]);
        assert_eq!(mix([0, 0, 0], [255, 255, 255], 0.0), [255, 255, 255]);
        assert_eq!(mix([0, 100, 200], [100, 200, 0], 0.5), [50, 150, 100]);
    }

    #[test]
    fn applying_the_native_look_never_breaks_the_theme() {
        let i = crate::pack::pack_index("native").expect("the native pack exists");
        let mut th = Theme::load(i).unwrap();
        apply(&mut th); // reads the system on a Mac, does nothing elsewhere
        assert_eq!(th.name, "Native");
        assert!(th.light_square.r > 0.0 || th.light_square.g > 0.0);
    }
}
