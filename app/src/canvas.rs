//! A drawing surface in plain memory: filled rectangles, outlines and images.
//! No graphics card is used, which keeps the app small on any Mac. The canvas
//! draws straight into the window's own pixel buffer (it keeps no copy of its
//! own), so all drawing happens while the window is being redrawn.
//! Coordinates are in window points; the canvas multiplies by the screen's
//! scale (2 on Retina screens) itself.

/// A colour with straight (not premultiplied) alpha, each part 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        let f = |v: u8| v as f32 / 255.0;
        Color {
            r: f(r),
            g: f(g),
            b: f(b),
            a: f(a),
        }
    }
}

/// An RGBA image (straight alpha), decoded from a PNG built into the app.
pub struct Img {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[u8; 4]>,
}

impl Img {
    pub fn from_png(bytes: &[u8]) -> Img {
        let mut dec = png::Decoder::new(bytes);
        dec.set_transformations(png::Transformations::EXPAND);
        let mut reader = dec.read_info().expect("a PNG");
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).expect("a PNG frame");
        let (w, h) = (info.width as usize, info.height as usize);
        let px = match info.color_type {
            png::ColorType::Rgba => buf
                .chunks(4)
                .take(w * h)
                .map(|c| [c[0], c[1], c[2], c[3]])
                .collect(),
            png::ColorType::Rgb => buf
                .chunks(3)
                .take(w * h)
                .map(|c| [c[0], c[1], c[2], 255])
                .collect(),
            png::ColorType::GrayscaleAlpha => buf
                .chunks(2)
                .take(w * h)
                .map(|c| [c[0], c[0], c[0], c[1]])
                .collect(),
            _ => buf.iter().take(w * h).map(|&v| [v, v, v, 255]).collect(),
        };
        Img { w, h, px }
    }

    fn sample(&self, x: f32, y: f32) -> [f32; 4] {
        // bilinear, in straight alpha weighted by alpha so edges don't darken
        let x = x.clamp(0.0, self.w as f32 - 1.0);
        let y = y.clamp(0.0, self.h as f32 - 1.0);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let mut out = [0.0f32; 4];
        for (px, py, wgt) in [
            (x0, y0, (1.0 - fx) * (1.0 - fy)),
            (x1, y0, fx * (1.0 - fy)),
            (x0, y1, (1.0 - fx) * fy),
            (x1, y1, fx * fy),
        ] {
            let p = self.px[py * self.w + px];
            let a = p[3] as f32 / 255.0 * wgt;
            out[0] += p[0] as f32 / 255.0 * a;
            out[1] += p[1] as f32 / 255.0 * a;
            out[2] += p[2] as f32 / 255.0 * a;
            out[3] += a;
        }
        if out[3] > 0.0 {
            let a = out[3];
            for v in &mut out[..3] {
                *v /= a;
            }
        }
        out
    }
}

/// The window's pixels: 0x00RRGGBB, `w` x `h` physical pixels at `scale`,
/// borrowed from the window for the length of one redraw.
pub struct Canvas<'a> {
    pub w: usize,
    pub h: usize,
    pub scale: f32,
    pub px: &'a mut [u32],
}

fn blend(dst: u32, c: [f32; 3], a: f32) -> u32 {
    if a >= 1.0 {
        return to_u32(c);
    }
    let d = [
        ((dst >> 16) & 255) as f32 / 255.0,
        ((dst >> 8) & 255) as f32 / 255.0,
        (dst & 255) as f32 / 255.0,
    ];
    to_u32([
        c[0] * a + d[0] * (1.0 - a),
        c[1] * a + d[1] * (1.0 - a),
        c[2] * a + d[2] * (1.0 - a),
    ])
}

fn to_u32(c: [f32; 3]) -> u32 {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    (q(c[0]) << 16) | (q(c[1]) << 8) | q(c[2])
}

impl<'a> Canvas<'a> {
    pub fn new(px: &'a mut [u32], w: usize, h: usize, scale: f32) -> Self {
        assert_eq!(px.len(), w * h, "the buffer matches the window size");
        Canvas { w, h, scale, px }
    }

    /// Physical pixel bounds of a rectangle in points, clipped to the canvas.
    fn bounds(&self, x: f32, y: f32, w: f32, h: f32) -> (usize, usize, usize, usize) {
        let s = self.scale;
        let clip = |v: f32, max: usize| (v.round().max(0.0) as usize).min(max);
        (
            clip(x * s, self.w),
            clip(y * s, self.h),
            clip((x + w) * s, self.w),
            clip((y + h) * s, self.h),
        )
    }

    pub fn clear(&mut self, c: Color) {
        let v = to_u32([c.r, c.g, c.b]);
        self.px.iter_mut().for_each(|p| *p = v);
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color) {
        if c.a <= 0.0 {
            return;
        }
        let (x0, y0, x1, y1) = self.bounds(x, y, w, h);
        for row in y0..y1 {
            for p in &mut self.px[row * self.w + x0..row * self.w + x1] {
                *p = blend(*p, [c.r, c.g, c.b], c.a);
            }
        }
    }

    /// An outline `t` points thick, drawn inside the rectangle.
    pub fn rect_lines(&mut self, x: f32, y: f32, w: f32, h: f32, t: f32, c: Color) {
        self.rect(x, y, w, t, c);
        self.rect(x, y + h - t, w, t, c);
        self.rect(x, y + t, t, h - 2.0 * t, c);
        self.rect(x + w - t, y + t, t, h - 2.0 * t, c);
    }

    /// Draws part of an image (`src` in image pixels: x, y, w, h) into a
    /// rectangle in points. With `tint`, the image's colour is replaced by the
    /// tint (used for text, which is baked white).
    pub fn image(
        &mut self,
        img: &Img,
        src: (f32, f32, f32, f32),
        dst: (f32, f32, f32, f32),
        tint: Option<Color>,
    ) {
        let (sx, sy, sw, sh) = src;
        let (dx, dy, dw, dh) = dst;
        let (x0, y0, x1, y1) = self.bounds(dx, dy, dw, dh);
        let s = self.scale;
        let (kx, ky) = (sw / (dw * s), sh / (dh * s)); // image pixels per canvas pixel
        let one_to_one = (kx - 1.0).abs() < 1e-3 && (ky - 1.0).abs() < 1e-3;
        for row in y0..y1 {
            for col in x0..x1 {
                // centre of this canvas pixel, in image pixels
                let ix = sx + ((col as f32 + 0.5) - dx * s) * kx - 0.5;
                let iy = sy + ((row as f32 + 0.5) - dy * s) * ky - 0.5;
                let p = if one_to_one {
                    let (rx, ry) = (ix.round() as isize, iy.round() as isize);
                    if rx < 0 || ry < 0 || rx as usize >= img.w || ry as usize >= img.h {
                        continue;
                    }
                    let v = img.px[ry as usize * img.w + rx as usize];
                    [
                        v[0] as f32 / 255.0,
                        v[1] as f32 / 255.0,
                        v[2] as f32 / 255.0,
                        v[3] as f32 / 255.0,
                    ]
                } else if kx > 1.5 || ky > 1.5 {
                    // shrinking: average a small block so edges stay smooth
                    let n = kx.max(ky).round().max(2.0) as i32;
                    let mut acc = [0.0f32; 4];
                    for j in 0..n {
                        for i in 0..n {
                            let q = img.sample(
                                ix - (n as f32 - 1.0) / 2.0 + i as f32,
                                iy - (n as f32 - 1.0) / 2.0 + j as f32,
                            );
                            for c in 0..3 {
                                acc[c] += q[c] * q[3];
                            }
                            acc[3] += q[3];
                        }
                    }
                    let a = acc[3] / (n * n) as f32;
                    if acc[3] > 0.0 {
                        [acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3], a]
                    } else {
                        continue;
                    }
                } else {
                    img.sample(ix, iy)
                };
                let (rgb, a) = match tint {
                    Some(t) => ([t.r, t.g, t.b], p[3] * t.a),
                    None => ([p[0], p[1], p[2]], p[3]),
                };
                if a > 0.0 {
                    let i = row * self.w + col;
                    self.px[i] = blend(self.px[i], rgb, a);
                }
            }
        }
    }

    /// Saves the canvas as a PNG (the P key, or a `shot` file from Claude).
    pub fn save_png(&self, path: &std::path::Path) -> std::io::Result<()> {
        let file = std::fs::File::create(path)?;
        let mut enc =
            png::Encoder::new(std::io::BufWriter::new(file), self.w as u32, self.h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let data: Vec<u8> = self
            .px
            .iter()
            .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
            .collect();
        enc.write_header()
            .and_then(|mut w| w.write_image_data(&data))
            .map_err(std::io::Error::other)
    }
}
