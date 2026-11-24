//! Text, drawn from an atlas that build.rs bakes from the computer's own Arial at
//! the few sizes the window uses ("b30" = bold 30 pt, "r14" = regular 14 pt...).
//! Loading a real font at run time would cost about 45 MB, so it is baked.
use crate::canvas::{Canvas, Color, Img};
use std::collections::HashMap;

pub struct Txt {
    img: Img,
    rows: HashMap<String, (f32, f32, f32)>, // style -> y, height, ascent (atlas pixels, 2x)
    chars: HashMap<(String, u8), (f32, f32, f32)>, // (style, byte) -> x, width, advance
    fit: Option<Fit>, // the atlas redrawn for the screen's own scale (see `fit`)
}

/// The atlas redrawn once at the exact scale of the screen, so a letter is copied to the
/// window pixel for pixel instead of being resampled on every frame. Same layout as the
/// atlas; only the sizes differ.
struct Fit {
    scale: f32,
    img: Img,
    rows: HashMap<String, (f32, f32)>, // style -> y, height (pixels)
    chars: HashMap<(String, u8), (f32, f32)>, // (style, byte) -> x, width (pixels)
}

impl Txt {
    pub fn load() -> Self {
        let img = Img::from_png(include_bytes!(concat!(env!("OUT_DIR"), "/text.png")));
        let (mut rows, mut chars) = (HashMap::new(), HashMap::new());
        for line in include_str!(concat!(env!("OUT_DIR"), "/text.txt")).lines() {
            let p: Vec<&str> = line.split(' ').collect();
            let n = |i: usize| p[i].parse::<f32>().unwrap();
            match p[0] {
                "row" => {
                    rows.insert(p[1].to_string(), (n(2), n(3), n(4)));
                }
                "ch" => {
                    chars.insert(
                        (p[1].to_string(), p[2].parse::<u8>().unwrap()),
                        (n(3), n(4), n(5)),
                    );
                }
                _ => {}
            }
        }
        Txt {
            img,
            rows,
            chars,
            fit: None,
        }
    }

    /// Redraw the atlas for a screen at `scale` (window pixels per point), unless that is
    /// already done. The atlas is baked at 2 pixels a point, so a 2x screen needs nothing.
    /// Costs a little over a megabyte on a 125% screen and a couple of milliseconds, once.
    pub fn fit(&mut self, scale: f64) {
        let s = scale as f32;
        if (s - 2.0).abs() < 1e-3 {
            self.fit = None;
            return;
        }
        if self
            .fit
            .as_ref()
            .is_some_and(|f| (f.scale - s).abs() < 1e-3)
        {
            return;
        }
        let f = s / 2.0; // new pixels per atlas pixel
        let mut styles: Vec<(&String, f32, f32)> =
            self.rows.iter().map(|(k, v)| (k, v.0, v.1)).collect();
        styles.sort_by(|a, b| a.1.total_cmp(&b.1));
        let cell = |v: f32| (v * f).ceil();
        let width = styles
            .iter()
            .map(|(k, ..)| {
                (32u8..127)
                    .filter_map(|b| self.chars.get(&((*k).clone(), b)))
                    .map(|c| cell(c.1))
                    .sum::<f32>()
            })
            .fold(0.0f32, f32::max) as usize;
        let height: usize = styles.iter().map(|s| cell(s.2) as usize).sum();
        let mut img = Img {
            w: width,
            h: height,
            px: vec![[0; 4]; width * height],
        };
        let (mut rows, mut chars) = (HashMap::new(), HashMap::new());
        let mut y_at = 0usize;
        for (style, ry, rh) in styles {
            let h2 = cell(rh) as usize;
            rows.insert(style.clone(), (y_at as f32, h2 as f32));
            let mut x_at = 0usize;
            for b in 32u8..127 {
                let Some(&(cx, cw, _)) = self.chars.get(&(style.clone(), b)) else {
                    continue;
                };
                let w2 = cell(cw) as usize;
                chars.insert((style.clone(), b), (x_at as f32, w2 as f32));
                // each new pixel is the average coverage of the atlas pixels under it
                for y in 0..h2 {
                    let (y0, y1) = (y as f32 / f, ((y + 1) as f32 / f).min(rh));
                    for x in 0..w2 {
                        let (x0, x1) = (x as f32 / f, ((x + 1) as f32 / f).min(cw));
                        let (mut acc, mut area) = (0.0f32, 0.0f32);
                        for sy in y0.floor() as usize..(y1.ceil() as usize) {
                            let wy = (y1.min(sy as f32 + 1.0) - y0.max(sy as f32)).max(0.0);
                            for sx in x0.floor() as usize..(x1.ceil() as usize) {
                                let wx = (x1.min(sx as f32 + 1.0) - x0.max(sx as f32)).max(0.0);
                                let a = self.img.px
                                    [(ry as usize + sy) * self.img.w + cx as usize + sx][3];
                                acc += a as f32 * wx * wy;
                                area += wx * wy;
                            }
                        }
                        if acc > 0.0 && area > 0.0 {
                            img.px[(y_at + y) * width + x_at + x] =
                                [255, 255, 255, (acc / area).round().min(255.0) as u8];
                        }
                    }
                }
                x_at += w2;
            }
            y_at += h2;
        }
        self.fit = Some(Fit {
            scale: s,
            img,
            rows,
            chars,
        });
    }

    /// Width in points.
    pub fn width(&self, style: &str, s: &str) -> f32 {
        s.bytes()
            .filter_map(|b| self.chars.get(&(style.to_string(), b)))
            .map(|c| c.2 / 2.0)
            .sum()
    }

    /// Draws `s` reading upwards (a quarter turn anticlockwise), centred on the
    /// vertical line `cx`, with its first letter at `bottom` and the rest above it.
    pub fn draw_up(
        &self,
        cv: &mut Canvas<'_>,
        style: &str,
        s: &str,
        cx: f32,
        bottom: f32,
        col: Color,
    ) {
        let Some(&(ry, h, _)) = self.rows.get(style) else {
            return;
        };
        let mut pen = bottom;
        for b in s.bytes() {
            let Some(&(gx, w, adv)) = self.chars.get(&(style.to_string(), b)) else {
                continue;
            };
            cv.image_up(
                &self.img,
                (gx, ry, w, h),
                (
                    (cx - h / 4.0).round(),
                    (pen - w / 2.0).round(),
                    h / 2.0,
                    w / 2.0,
                ),
                col,
            );
            pen -= adv / 2.0;
        }
    }

    /// Draws `s` with its baseline at `baseline` (points).
    pub fn draw(
        &self,
        cv: &mut Canvas<'_>,
        style: &str,
        s: &str,
        x: f32,
        baseline: f32,
        col: Color,
    ) {
        self.draw_scaled(cv, style, s, x, baseline, col, 1.0);
    }

    /// Like `width`, for text drawn `k` times as big with `draw_scaled`.
    pub fn width_scaled(&self, style: &str, s: &str, k: f32) -> f32 {
        self.width(style, s) * k
    }

    /// Draws `s` `k` times as big as its style (the home screen's big numbers and header
    /// use the 30 pt bold style at 1.5, so no extra atlas is baked and no memory is spent).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_scaled(
        &self,
        cv: &mut Canvas<'_>,
        style: &str,
        s: &str,
        x: f32,
        baseline: f32,
        col: Color,
        k: f32,
    ) {
        let Some(&(ry, h, asc)) = self.rows.get(style) else {
            return;
        };
        // at the screen's own scale (and normal size) use the atlas redrawn for it
        let fit = self
            .fit
            .as_ref()
            .filter(|f| k == 1.0 && (f.scale - cv.scale).abs() < 1e-3);
        let mut pen = x;
        for b in s.bytes() {
            let Some(&(cx, w, adv)) = self.chars.get(&(style.to_string(), b)) else {
                continue;
            };
            let top = baseline - (asc + 1.0) / 2.0 * k;
            let fitted = fit.and_then(|f| {
                let (fy, fh) = *f.rows.get(style)?;
                let (fx, fw) = *f.chars.get(&(style.to_string(), b))?;
                Some((f, (fx, fy, fw, fh)))
            });
            match fitted {
                Some((f, src)) => cv.image(
                    &f.img,
                    src,
                    (
                        pen.round() - 0.5,
                        top.round(),
                        src.2 / cv.scale,
                        src.3 / cv.scale,
                    ),
                    Some(col),
                ),
                None => cv.image(
                    &self.img,
                    (cx, ry, w, h),
                    (pen.round() - 0.5, top.round(), w / 2.0 * k, h / 2.0 * k),
                    Some(col),
                ),
            }
            pen += adv / 2.0 * k;
        }
    }
}
