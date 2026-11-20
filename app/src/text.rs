//! Text, drawn from an atlas that build.rs bakes from the Mac's own Arial at
//! the few sizes the window uses ("b30" = bold 30 pt, "r14" = regular 14 pt...).
//! Loading a real font at run time would cost about 45 MB, so it is baked.
use crate::canvas::{Canvas, Color, Img};
use std::collections::HashMap;

pub struct Txt {
    img: Img,
    rows: HashMap<String, (f32, f32, f32)>, // style -> y, height, ascent (atlas pixels, 2x)
    chars: HashMap<(String, u8), (f32, f32, f32)>, // (style, byte) -> x, width, advance
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
        Txt { img, rows, chars }
    }

    /// Width in points.
    pub fn width(&self, style: &str, s: &str) -> f32 {
        s.bytes()
            .filter_map(|b| self.chars.get(&(style.to_string(), b)))
            .map(|c| c.2 / 2.0)
            .sum()
    }

    /// Draws `s` with its baseline at `baseline` (points).
    pub fn draw(&self, cv: &mut Canvas, style: &str, s: &str, x: f32, baseline: f32, col: Color) {
        let Some(&(ry, h, asc)) = self.rows.get(style) else {
            return;
        };
        let mut pen = x;
        for b in s.bytes() {
            let Some(&(cx, w, adv)) = self.chars.get(&(style.to_string(), b)) else {
                continue;
            };
            let top = baseline - (asc + 1.0) / 2.0;
            cv.image(
                &self.img,
                (cx, ry, w, h),
                (pen.round() - 0.5, top.round(), w / 2.0, h / 2.0),
                Some(col),
            );
            pen += adv / 2.0;
        }
    }
}
