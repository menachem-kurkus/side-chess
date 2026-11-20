// Draws the chess pieces and the few text sizes the app uses into small
// images at build time, from fonts every Mac already has. Shipping images
// made from Apple's fonts isn't allowed, and loading the fonts at run time
// would cost about 45 MB of memory, so they're baked here instead.
use fontdue::{Font, FontSettings};
use std::{env, fs, path::Path};

const ARIAL: &str = "/System/Library/Fonts/Supplemental/Arial.ttf";
const ARIAL_BOLD: &str = "/System/Library/Fonts/Supplemental/Arial Bold.ttf";

fn font(path: &str) -> Font {
    let bytes = fs::read(path)
        .unwrap_or_else(|_| panic!("Side Chess builds on macOS only: {path} not found"));
    Font::from_bytes(bytes, FontSettings::default()).unwrap()
}

struct Img {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
} // straight-alpha RGBA, 0..1

impl Img {
    fn new(w: usize, h: usize) -> Self {
        Img {
            w,
            h,
            px: vec![[0.0; 4]; w * h],
        }
    }
    // paint `color` through a coverage mask placed at (x0, y0)
    fn paint(&mut self, mask: &[f32], mw: usize, mh: usize, x0: i32, y0: i32, color: [f32; 4]) {
        for y in 0..mh {
            for x in 0..mw {
                let (tx, ty) = (x0 + x as i32, y0 + y as i32);
                if tx < 0 || ty < 0 || tx >= self.w as i32 || ty >= self.h as i32 {
                    continue;
                }
                let a = mask[y * mw + x] * color[3];
                if a <= 0.0 {
                    continue;
                }
                let d = &mut self.px[ty as usize * self.w + tx as usize];
                let out_a = a + d[3] * (1.0 - a);
                for c in 0..3 {
                    d[c] = if out_a > 0.0 {
                        (color[c] * a + d[c] * d[3] * (1.0 - a)) / out_a
                    } else {
                        0.0
                    };
                }
                d[3] = out_a;
            }
        }
    }
    fn save(&self, path: &Path) {
        let data: Vec<u8> = self
            .px
            .iter()
            .flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
            .collect();
        let file = fs::File::create(path).unwrap();
        let mut enc =
            png::Encoder::new(std::io::BufWriter::new(file), self.w as u32, self.h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&data).unwrap();
    }
}

// grayscale morphology and blur on a mask
fn filter(m: &[f32], w: usize, h: usize, r: i32, f: fn(f32, f32) -> f32, init: f32) -> Vec<f32> {
    let mut out = vec![init; w * h];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut v = init;
            for dy in -r..=r {
                for dx in -r..=r {
                    let (sx, sy) = (x + dx, y + dy);
                    let s = if sx < 0 || sy < 0 || sx >= w as i32 || sy >= h as i32 {
                        0.0
                    } else {
                        m[sy as usize * w + sx as usize]
                    };
                    v = f(v, s);
                }
            }
            out[y as usize * w + x as usize] = v;
        }
    }
    out
}
fn blur(m: &[f32], w: usize, h: usize, r: i32) -> Vec<f32> {
    let n = ((2 * r + 1) * (2 * r + 1)) as f32;
    let sum = filter(m, w, h, r, |a, b| a + b, 0.0);
    sum.into_iter().map(|v| v / n).collect()
}

fn to_mask(bytes: &[u8]) -> Vec<f32> {
    bytes.iter().map(|&b| b as f32 / 255.0).collect()
}

fn pieces(dest: &Path, font_path: &str, white_rgb: [f32; 3], black_rgb: [f32; 3]) {
    const S: usize = 128;
    let f = font(font_path);
    let px = S as f32 * 0.86;
    let filled = [
        '\u{265A}', '\u{265B}', '\u{265C}', '\u{265D}', '\u{265E}', '\u{265F}',
    ];
    let hollow = [
        '\u{2654}', '\u{2655}', '\u{2656}', '\u{2657}', '\u{2658}', '\u{2659}',
    ];
    let dark = [black_rgb[0], black_rgb[1], black_rgb[2], 1.0];
    let mut sheet = Img::new(S * 6, S * 2);
    for row in 0..2 {
        let white = row == 0;
        for i in 0..6 {
            let (mf, bf) = f.rasterize(filled[i], px);
            let (mh, bh) = f.rasterize(hollow[i], px);
            // centre the filled glyph's box in the tile; the outline shares its pen position
            let left = (S as i32 - mf.width as i32) / 2;
            let top = (S as i32 - mf.height as i32) / 2;
            let (ox, oy) = ((i * S) as i32, (row * S) as i32);
            let fm = to_mask(&bf);
            // soft drop shadow
            let pad = 6usize;
            let (sw, sh) = (mf.width + pad * 2, mf.height + pad * 2);
            let mut sm = vec![0.0; sw * sh];
            for y in 0..mf.height {
                for x in 0..mf.width {
                    sm[(y + pad) * sw + x + pad] = fm[y * mf.width + x];
                }
            }
            let sm = blur(&blur(&sm, sw, sh, 1), sw, sh, 1);
            sheet.paint(
                &sm,
                sw,
                sh,
                ox + left + 2 - pad as i32,
                oy + top + 4 - pad as i32,
                [0.0, 0.0, 0.0, 70.0 / 255.0],
            );
            if white {
                // white body: the filled shape with its inner gaps closed
                let (cw, ch) = (mf.width + 8, mf.height + 8);
                let mut cm = vec![0.0; cw * ch];
                for y in 0..mf.height {
                    for x in 0..mf.width {
                        cm[(y + 4) * cw + x + 4] = fm[y * mf.width + x];
                    }
                }
                let cm = filter(
                    &filter(&cm, cw, ch, 3, f32::max, 0.0),
                    cw,
                    ch,
                    3,
                    f32::min,
                    1.0,
                );
                sheet.paint(
                    &cm,
                    cw,
                    ch,
                    ox + left - 4,
                    oy + top - 4,
                    [white_rgb[0], white_rgb[1], white_rgb[2], 1.0],
                );
                // the outline and the filled glyph share their outer shape: line up their boxes
                let hx = left + (mf.width as i32 - mh.width as i32) / 2;
                let hy = top + (mf.height as i32 - mh.height as i32) / 2;
                sheet.paint(&to_mask(&bh), mh.width, mh.height, ox + hx, oy + hy, dark);
            } else {
                sheet.paint(&fm, mf.width, mf.height, ox + left, oy + top, dark);
            }
        }
    }
    sheet.save(dest);
}

fn text(out: &Path) {
    let styles = [
        ("b30", ARIAL_BOLD, 30.0),
        ("b18", ARIAL_BOLD, 18.0),
        ("b11", ARIAL_BOLD, 11.0),
        ("r16", ARIAL, 16.0),
        ("r14", ARIAL, 14.0),
    ];
    let mut rows = vec![];
    let mut lines = vec![];
    let (mut y, mut width) = (0usize, 0usize);
    for (key, path, size) in styles {
        let f = font(path);
        let px = size * 2.0; // 2x for retina screens
        let lm = f.horizontal_line_metrics(px).unwrap();
        let asc = lm.ascent.ceil() as i32;
        let h = (asc + (-lm.descent).ceil() as i32 + 2) as usize;
        let mut x = 0usize;
        let mut cells = vec![];
        for code in 32u8..127 {
            let (m, bmp) = f.rasterize(code as char, px);
            let w = m.advance_width.ceil() as usize + 2;
            cells.push((code, x, w, m, bmp));
            lines.push(format!("ch {key} {code} {x} {w} {:.2}", m.advance_width));
            x += w;
        }
        width = width.max(x);
        lines.push(format!("row {key} {y} {h} {asc}"));
        rows.push((y, h, asc, cells));
        y += h;
    }
    let mut img = Img::new(width, y);
    for (ry, _h, asc, cells) in rows {
        for (_code, cx, _w, m, bmp) in cells {
            let gx = cx as i32 + 1 + m.xmin;
            let gy = ry as i32 + asc + 1 - (m.ymin + m.height as i32);
            img.paint(
                &to_mask(&bmp),
                m.width,
                m.height,
                gx,
                gy,
                [1.0, 1.0, 1.0, 1.0],
            );
        }
    }
    img.save(&out.join("text.png"));
    fs::write(out.join("text.txt"), lines.join("\n") + "\n").unwrap();
}

// key = value lines of a pack.txt (a line starting with # is a comment)
fn pack_keys(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

fn rgb(hex: &str) -> [f32; 3] {
    let h = hex.trim_start_matches('#');
    let n = |i: usize| {
        u8::from_str_radix(&h[i..i + 2], 16).expect("colour like #RRGGBB") as f32 / 255.0
    };
    [n(0), n(2), n(4)]
}

// Every folder in packs/ is a look. Its pieces come from its own pieces.png,
// or are drawn here from its piece_font. The list is built into the app.
fn packs(out: &Path) {
    println!("cargo:rerun-if-changed=packs");
    let root = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("packs");
    let base = pack_keys(
        &fs::read_to_string(root.join("blue-gold/pack.txt")).expect("packs/blue-gold/pack.txt"),
    );
    let mut ids: Vec<String> = fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("pack.txt").exists())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    ids.sort();
    let mut list = String::from("pub const PACKS: &[(&str, &str, &[u8])] = &[\n");
    for id in ids {
        let dir = root.join(&id);
        let mut keys = base.clone();
        keys.extend(pack_keys(
            &fs::read_to_string(dir.join("pack.txt")).unwrap(),
        ));
        let get = |k: &str| {
            keys.iter()
                .rev()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        let png = out.join(format!("pack-{id}.png"));
        if dir.join("pieces.png").exists() {
            fs::copy(dir.join("pieces.png"), &png).unwrap();
        } else {
            pieces(
                &png,
                &get("piece_font"),
                rgb(&get("piece_white")),
                rgb(&get("piece_black")),
            );
        }
        list += &format!(
            "    ({id:?}, include_str!({:?}), include_bytes!({:?})),\n",
            dir.join("pack.txt").display().to_string(),
            png.display().to_string()
        );
    }
    list += "];\n";
    fs::write(out.join("packs.rs"), list).unwrap();
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let out = env::var("OUT_DIR").unwrap();
    let out = Path::new(&out);
    packs(out);
    text(out);
}
