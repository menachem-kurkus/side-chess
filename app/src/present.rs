//! Shows finished frames in the window, labelled with the window's own colour
//! profile. macOS then has nothing to convert: handing over a frame takes well
//! under a millisecond, and macOS keeps no converted copy of the window
//! (about 20 MB on a Retina screen).
//!
//! Because the frame is in the screen's colours, the pack's colours and piece
//! images are converted to that screen once (see `Screen::convert`), not every
//! frame. Pack makers still write ordinary #RRGGBB colours.
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGColorSpaceModel,
    CGContext, CGDataProvider, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo,
};
use objc2_quartz_core::{kCAGravityTopLeft, CALayer, CATransaction};
use std::ffi::c_void;
use std::ptr::{self, NonNull};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// The window's drawing layer.
pub struct Presenter {
    view: Retained<AnyObject>, // the window's NSView
    layer: Retained<CALayer>,
}

/// A screen's colour profile, compared to notice when the window moves.
pub struct Screen(CFRetained<CGColorSpace>);

impl PartialEq for Screen {
    fn eq(&self, other: &Screen) -> bool {
        *self.0 == *other.0 // same profile contents (CFEqual)
    }
}

impl Presenter {
    pub fn new(window: &Window) -> Presenter {
        let RawWindowHandle::AppKit(h) = window.window_handle().expect("a window").as_raw() else {
            panic!("Side Chess runs on macOS");
        };
        // SAFETY: winit gives a valid NSView, and this runs on the main thread.
        unsafe {
            let view: Retained<AnyObject> =
                Retained::retain(h.ns_view.as_ptr().cast()).expect("an NSView");
            let _: () = msg_send![&*view, setWantsLayer: Bool::YES];
            let root: Option<Retained<CALayer>> = msg_send![&*view, layer];
            let root = root.expect("a layer-backed view");
            let layer = CALayer::new();
            layer.setAnchorPoint(CGPoint::new(0.0, 0.0));
            layer.setGeometryFlipped(true);
            layer.setContentsGravity(kCAGravityTopLeft);
            root.addSublayer(&layer);
            Presenter { view, layer }
        }
    }

    /// The colour profile of the screen the window is on now.
    pub fn screen(&self) -> Screen {
        // SAFETY: plain property reads on the main thread.
        unsafe {
            let win: Option<Retained<AnyObject>> = msg_send![&*self.view, window];
            let ns: Option<Retained<AnyObject>> = win.and_then(|w| msg_send![&*w, colorSpace]);
            let cg: *mut CGColorSpace = match &ns {
                Some(ns) => msg_send![&**ns, CGColorSpace],
                None => ptr::null_mut(),
            };
            let cg = NonNull::new(cg).map(|cg| CFRetained::retain(cg));
            match cg {
                // only an ordinary RGB profile can hold the frame's pixels
                Some(cg)
                    if CGColorSpace::model(Some(&cg)) == CGColorSpaceModel::RGB
                        && CGColorSpace::number_of_components(Some(&cg)) == 3 =>
                {
                    Screen(cg)
                }
                _ => Screen(CGColorSpace::new_device_rgb().expect("a colour space")),
            }
        }
    }

    /// Draws a frame with `paint` (0x00RRGGBB, in `screen`'s colours) and puts it in the window.
    pub fn frame(
        &self,
        w: usize,
        h: usize,
        scale: f64,
        screen: &Screen,
        paint: impl FnOnce(&mut [u32]),
    ) {
        let mut px = vec![0u32; w * h];
        paint(&mut px);
        self.present(px, w, h, scale, screen);
    }

    /// Puts a finished frame (0x00RRGGBB, already in `screen`'s colours) in the window.
    pub fn present(&self, px: Vec<u32>, w: usize, h: usize, scale: f64, screen: &Screen) {
        unsafe extern "C-unwind" fn release(_: *mut c_void, data: NonNull<c_void>, size: usize) {
            // SAFETY: the same boxed slice handed over below, now no longer used by macOS
            let s = ptr::slice_from_raw_parts_mut(data.as_ptr().cast::<u32>(), size / 4);
            drop(unsafe { Box::from_raw(s) });
        }
        let len = px.len() * 4;
        let data = Box::into_raw(px.into_boxed_slice()).cast::<c_void>();
        // SAFETY: valid pointer and length; macOS frees it through `release`.
        let image = unsafe {
            let provider =
                CGDataProvider::with_data(ptr::null_mut(), data, len, Some(release)).unwrap();
            CGImage::new(
                w,
                h,
                8,
                32,
                w * 4,
                Some(&screen.0),
                pixel_format(),
                Some(&provider),
                ptr::null(),
                false,
                CGColorRenderingIntent::RenderingIntentDefault,
            )
            .unwrap()
        };
        CATransaction::begin();
        CATransaction::setDisableActions(true); // no fade between frames
        self.layer.setContentsScale(scale);
        self.layer.setFrame(CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(w as f64 / scale, h as f64 / scale),
        ));
        // SAFETY: a CGImage is valid layer contents.
        unsafe { self.layer.setContents(Some(image.as_ref())) };
        CATransaction::commit();
    }
}

/// 0x00RRGGBB in a u32, as the canvas draws it.
fn pixel_format() -> CGBitmapInfo {
    CGBitmapInfo(CGImageAlphaInfo::NoneSkipFirst.0 | CGImageByteOrderInfo::Order32Little.0)
}

impl Screen {
    /// The profile's ICC data, to embed in screenshots.
    pub fn icc(&self) -> Option<Vec<u8>> {
        CGColorSpace::icc_data(Some(&self.0)).map(|d| d.to_vec())
    }

    /// Converts RGBA pixels from the colours the app is designed in (what the
    /// window showed before this change: macOS's device RGB) to this screen's
    /// colours, with macOS's own colour matching. Alpha is left as it is.
    pub fn convert(&self, rgba: &mut [[u8; 4]]) {
        if rgba.is_empty() {
            return;
        }
        let n = rgba.len();
        let mut src: Vec<u32> = rgba
            .iter()
            .map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32)
            .collect();
        let mut dst = vec![0u32; n];
        // SAFETY: both buffers outlive the context and image that point at them.
        unsafe {
            let from = CGColorSpace::new_device_rgb().unwrap();
            let provider =
                CGDataProvider::with_data(ptr::null_mut(), src.as_mut_ptr().cast(), n * 4, None)
                    .unwrap();
            let image = CGImage::new(
                n,
                1,
                8,
                32,
                n * 4,
                Some(&from),
                pixel_format(),
                Some(&provider),
                ptr::null(),
                false,
                CGColorRenderingIntent::RenderingIntentDefault,
            )
            .unwrap();
            let ctx = CGBitmapContextCreate(
                dst.as_mut_ptr().cast(),
                n,
                1,
                8,
                n * 4,
                Some(&self.0),
                pixel_format().0,
            )
            .unwrap();
            CGContext::draw_image(
                Some(&ctx),
                CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(n as f64, 1.0)),
                Some(&image),
            );
        }
        for (p, d) in rgba.iter_mut().zip(dst) {
            p[0] = (d >> 16) as u8;
            p[1] = (d >> 8) as u8;
            p[2] = d as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::{load_pack, load_pack_for, PACKS};

    fn device_rgb() -> Screen {
        Screen(CGColorSpace::new_device_rgb().unwrap())
    }

    #[test]
    fn converting_to_the_colours_it_is_already_in_changes_nothing() {
        let mut px = vec![[10, 200, 30, 77], [255, 255, 255, 0], [0, 0, 0, 255]];
        let before = px.clone();
        device_rgb().convert(&mut px);
        assert_eq!(px, before);
    }

    #[test]
    fn every_pack_converts_for_a_wide_colour_screen_and_keeps_its_alpha() {
        // Display P3, as on recent Mac screens
        let p3 = Screen(
            CGColorSpace::with_name(Some(unsafe { objc2_core_graphics::kCGColorSpaceDisplayP3 }))
                .unwrap(),
        );
        for i in 0..PACKS.len() {
            let (mut plain, plain_px) = load_pack(i);
            let (mut conv, conv_px) = load_pack_for(i, &p3);
            for (a, b) in plain.colors_mut().iter().zip(conv.colors_mut().iter()) {
                assert_eq!(a.a, b.a, "pack {}: alpha must not change", PACKS[i].0);
            }
            assert_eq!((plain_px.w, plain_px.h), (conv_px.w, conv_px.h));
            assert!(plain_px
                .px
                .iter()
                .zip(&conv_px.px)
                .all(|(a, b)| a[3] == b[3]));
            // a saturated colour really is converted (P3 can show more than sRGB)
            let mut red = [[255u8, 0, 0, 255]];
            p3.convert(&mut red);
            assert!(
                red[0][0] < 255 && red[0][1] > 0,
                "sRGB red in P3 is {:?}",
                red[0]
            );
        }
    }
}
