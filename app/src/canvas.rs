//! A drawing surface in plain memory: filled rectangles, outlines and images.
//! No graphics card is used, which keeps the app small on any Mac, Windows or Linux PC. The canvas
//! draws straight into the window's own pixel buffer (it keeps no copy of its
//! own), so all drawing happens while the window is being redrawn.
//! Coordinates are in window points; the canvas multiplies by the screen's
//! scale (2 on Retina screens, 1.25 or 1.5 on many Windows ones) itself.

/// A colour with straight (not premultiplied) alpha, each part 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// `t` of this colour and the rest of `other` (t = 1 is this colour), alpha kept from this one.
    pub fn mix(self, other: Color, t: f32) -> Color {
        let m = |a: f32, b: f32| a * t + b * (1.0 - t);
        Color {
            r: m(self.r, other.r),
            g: m(self.g, other.g),
            b: m(self.b, other.b),
            a: self.a,
        }
    }

    /// How light it looks, 0 (black) to 1 (white).
    pub fn luma(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    /// Whichever of `a` and `b` stands out more from this colour (for text on it).
    pub fn ink(self, a: Color, b: Color) -> Color {
        if (a.luma() - self.luma()).abs() >= (b.luma() - self.luma()).abs() {
            a
        } else {
            b
        }
    }

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

    /// The 128 px tiles of a pieces sheet, redrawn `tile` px wide and high (area
    /// average, in alpha-weighted colour so edges stay clean). Done once per screen
    /// scale, so drawing the pieces afterwards is a plain copy instead of resampling
    /// every pixel of every piece on every frame.
    pub fn with_tiles_of(&self, tile: usize) -> Img {
        const SRC: usize = 128;
        let (cols, rows) = (self.w / SRC, self.h / SRC);
        let mut out = Img {
            w: cols * tile,
            h: rows * tile,
            px: vec![[0; 4]; cols * tile * rows * tile],
        };
        let k = SRC as f32 / tile as f32; // source pixels per new pixel
        for ty in 0..rows {
            for tx in 0..cols {
                for y in 0..tile {
                    for x in 0..tile {
                        let (x0, x1) = (x as f32 * k, (x + 1) as f32 * k);
                        let (y0, y1) = (y as f32 * k, (y + 1) as f32 * k);
                        let mut acc = [0.0f32; 4]; // r, g, b weighted by alpha; alpha
                        let mut area = 0.0;
                        for sy in y0.floor() as usize..(y1.ceil() as usize).min(SRC) {
                            let wy = (y1.min(sy as f32 + 1.0) - y0.max(sy as f32)).max(0.0);
                            for sx in x0.floor() as usize..(x1.ceil() as usize).min(SRC) {
                                let wx = (x1.min(sx as f32 + 1.0) - x0.max(sx as f32)).max(0.0);
                                let p = self.px[(ty * SRC + sy) * self.w + tx * SRC + sx];
                                let a = p[3] as f32 / 255.0 * wx * wy;
                                for c in 0..3 {
                                    acc[c] += p[c] as f32 * a;
                                }
                                acc[3] += a;
                                area += wx * wy;
                            }
                        }
                        if acc[3] > 0.0 {
                            let q = |v: f32| v.round().clamp(0.0, 255.0) as u8;
                            out.px[(ty * tile + y) * out.w + tx * tile + x] = [
                                q(acc[0] / acc[3]),
                                q(acc[1] / acc[3]),
                                q(acc[2] / acc[3]),
                                q(acc[3] / area * 255.0),
                            ];
                        }
                    }
                }
            }
        }
        out
    }

    /// Just the coverage (alpha, 0..1) at a point, bilinear like `sample`. Text is a white
    /// mask drawn in a tint, so only this is needed for it: the same pixels for a fraction
    /// of the work (no colour is weighed, nothing is divided).
    fn coverage(&self, x: f32, y: f32) -> f32 {
        let x = x.clamp(0.0, self.w as f32 - 1.0);
        let y = y.clamp(0.0, self.h as f32 - 1.0);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let a = |px: usize, py: usize| self.px[py * self.w + px][3] as f32;
        let (a00, a10, a01, a11) = (a(x0, y0), a(x1, y0), a(x0, y1), a(x1, y1));
        if a00 + a10 + a01 + a11 == 0.0 {
            return 0.0; // most of a letter's box: nothing to blend
        }
        ((a00 * (1.0 - fx) + a10 * fx) * (1.0 - fy) + (a01 * (1.0 - fx) + a11 * fx) * fy) / 255.0
    }

    fn sample(&self, x: f32, y: f32) -> [f32; 4] {
        // bilinear, in straight alpha weighted by alpha so edges don't darken
        let x = x.clamp(0.0, self.w as f32 - 1.0);
        let y = y.clamp(0.0, self.h as f32 - 1.0);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let mut out = [0.0f32; 4];
        // all four neighbours fully clear (most of a piece tile): nothing to blend
        let w = self.w;
        if self.px[y0 * w + x0][3]
            | self.px[y0 * w + x1][3]
            | self.px[y1 * w + x0][3]
            | self.px[y1 * w + x1][3]
            == 0
        {
            return out;
        }
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

    /// A disc (`inner` 0) or a ring, centred at (cx, cy) points, radii in points,
    /// with soft edges. Used for the faded dots that show where a piece can go.
    pub fn round(&mut self, cx: f32, cy: f32, outer: f32, inner: f32, c: Color) {
        let s = self.scale;
        let (cx, cy, outer, inner) = (cx * s, cy * s, outer * s, inner * s);
        let (x0, x1) = (
            (cx - outer - 1.0).floor().max(0.0) as usize,
            ((cx + outer + 1.0).ceil() as usize).min(self.w),
        );
        let (y0, y1) = (
            (cy - outer - 1.0).floor().max(0.0) as usize,
            ((cy + outer + 1.0).ceil() as usize).min(self.h),
        );
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let cover = (outer - d + 0.5).clamp(0.0, 1.0) - (inner - d + 0.5).clamp(0.0, 1.0);
                if cover > 0.0 {
                    let i = y * self.w + x;
                    self.px[i] = blend(self.px[i], [c.r, c.g, c.b], c.a * cover);
                }
            }
        }
    }

    /// A filled rectangle with rounded corners of radius `r`. Use solid colours: where a
    /// see-through colour is used the corners are drawn twice and show darker.
    pub fn rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Color) {
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        self.rect(x + r, y, w - 2.0 * r, h, c);
        self.rect(x, y + r, r, h - 2.0 * r, c);
        self.rect(x + w - r, y + r, r, h - 2.0 * r, c);
        for (cx, cy) in [
            (x + r, y + r),
            (x + w - r, y + r),
            (x + r, y + h - r),
            (x + w - r, y + h - r),
        ] {
            self.round(cx, cy, r, 0.0, c);
        }
    }

    /// A top-to-bottom blend from `c0` to `c1`, drawn a pixel row at a time.
    pub fn vgrad(&mut self, x: f32, y: f32, w: f32, h: f32, c0: Color, c1: Color) {
        let rows = (h * self.scale).round().max(1.0) as i32;
        for i in 0..rows {
            let t = (i as f32 + 0.5) / rows as f32;
            let c = c1.mix(c0, t);
            self.rect(x, y + i as f32 / self.scale, w, 1.0 / self.scale + 0.01, c);
        }
    }

    /// A soft shadow under a rectangle: a few stacked, faint, growing layers, nudged down.
    /// Only the part outside the rectangle is drawn (the rectangle goes on top of it
    /// anyway), so it costs about as much as its edge, not its area.
    pub fn shadow(&mut self, x: f32, y: f32, w: f32, h: f32, layers: u32) {
        let c = Color::rgba8(0, 0, 0, 9);
        for i in 1..=layers {
            let g = i as f32;
            self.rect(x - g, y - g + 3.0, w + 2.0 * g, (g - 3.0).max(0.0), c); // above
            self.rect(x - g, y + h, w + 2.0 * g, g + 3.0, c); // below
            self.rect(x - g, y, g, h, c); // left
            self.rect(x + w, y, g, h, c); // right
        }
    }

    /// A straight line of thickness `t` with round ends, from (x0, y0) to (x1, y1).
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, t: f32, c: Color) {
        let n = ((x1 - x0).hypot(y1 - y0) * 2.0).ceil().max(1.0) as i32;
        for i in 0..=n {
            let f = i as f32 / n as f32;
            self.round(x0 + (x1 - x0) * f, y0 + (y1 - y0) * f, t / 2.0, 0.0, c);
        }
    }

    /// An arrow from (x0, y0) to its tip at (x1, y1), in points: a shaft `t` thick with
    /// a round start and a triangular head of length `head`. Drawn in a single pass so
    /// a see-through colour is blended once (no darker joins).
    pub fn arrow(&mut self, from: (f32, f32), to: (f32, f32), t: f32, head: f32, c: Color) {
        let (x0, y0, x1, y1) = (from.0, from.1, to.0, to.1);
        let s = self.scale;
        let (x0, y0, x1, y1, t, head) = (x0 * s, y0 * s, x1 * s, y1 * s, t * s, head * s);
        let len = (x1 - x0).hypot(y1 - y0);
        if len < 1.0 {
            return;
        }
        let (dx, dy) = ((x1 - x0) / len, (y1 - y0) / len);
        let head = head.min(len);
        let (bx, by) = (x1 - dx * head, y1 - dy * head); // where the head starts
        let hw = head * 0.6; // half the head's width
        let (nx, ny) = (-dy, dx);
        let (ax, ay, cx, cy) = (bx + nx * hw, by + ny * hw, bx - nx * hw, by - ny * hw);
        let pad = hw.max(t) + 2.0;
        let (lo_x, hi_x) = (x0.min(x1) - pad, x0.max(x1) + pad);
        let (lo_y, hi_y) = (y0.min(y1) - pad, y0.max(y1) + pad);
        let (px0, px1) = (
            lo_x.floor().max(0.0) as usize,
            (hi_x.ceil() as usize).min(self.w),
        );
        let (py0, py1) = (
            lo_y.floor().max(0.0) as usize,
            (hi_y.ceil() as usize).min(self.h),
        );
        // signed distance (positive inside) from the edge p->q of the head, which is wound
        // so the inside is on the left
        let edge = |px: f32, py: f32, (qx, qy): (f32, f32), (rx, ry): (f32, f32)| {
            let (ex, ey) = (rx - qx, ry - qy);
            ((px - qx) * -ey + (py - qy) * ex) / ex.hypot(ey)
        };
        let side = if edge(bx, by, (ax, ay), (x1, y1)) > 0.0 {
            1.0
        } else {
            -1.0
        };
        for y in py0..py1 {
            for x in px0..px1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                // the shaft: distance to the segment start..head base
                let (vx, vy) = (px - x0, py - y0);
                let along = (vx * dx + vy * dy).clamp(0.0, (len - head).max(0.0));
                let d = (vx - dx * along).hypot(vy - dy * along);
                let shaft = (t / 2.0 - d + 0.5).clamp(0.0, 1.0);
                // the head: inside all three edges
                // inside all three edges: the smallest signed distance is positive
                let m = side * edge(px, py, (ax, ay), (x1, y1));
                let m = m
                    .min(side * edge(px, py, (x1, y1), (cx, cy)))
                    .min(side * edge(px, py, (cx, cy), (ax, ay)));
                let cover = shaft.max((m + 0.5).clamp(0.0, 1.0));
                if cover > 0.0 {
                    let i = y * self.w + x;
                    self.px[i] = blend(self.px[i], [c.r, c.g, c.b], c.a * cover);
                }
            }
        }
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color) {
        if c.a <= 0.0 {
            return;
        }
        let (x0, y0, x1, y1) = self.bounds(x, y, w, h);
        let solid = (c.a >= 1.0).then(|| to_u32([c.r, c.g, c.b])); // worked out once, not per pixel
        for row in y0..y1 {
            let line = &mut self.px[row * self.w + x0..row * self.w + x1];
            match solid {
                Some(v) => line.fill(v),
                None => line
                    .iter_mut()
                    .for_each(|p| *p = blend(*p, [c.r, c.g, c.b], c.a)),
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

    /// Like `image`, but turned a quarter turn anticlockwise, so text reads from the
    /// bottom up. `dst` is the turned rectangle in points (its width is the source's
    /// height, its height the source's width).
    pub fn image_up(
        &mut self,
        img: &Img,
        src: (f32, f32, f32, f32),
        dst: (f32, f32, f32, f32),
        tint: Color,
    ) {
        let (sx, sy, sw, sh) = src;
        let (dx, dy, dw, dh) = dst;
        let (x0, y0, x1, y1) = self.bounds(dx, dy, dw, dh);
        let s = self.scale;
        for row in y0..y1 {
            for col in x0..x1 {
                // average four points inside the canvas pixel so the edges stay smooth
                let mut acc = [0.0f32; 4];
                for (ox, oy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                    let u = (col as f32 + ox - dx * s) / (dw * s); // across the turned box
                    let v = (row as f32 + oy - dy * s) / (dh * s); // down it
                    if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                        continue;
                    }
                    // the source's x runs up the box, its y runs across it
                    let q = img.sample(sx + (1.0 - v) * sw - 0.5, sy + u * sh - 0.5);
                    acc[3] += q[3];
                }
                let a = acc[3] / 4.0 * tint.a;
                if a > 0.0 {
                    let i = row * self.w + col;
                    self.px[i] = blend(self.px[i], [tint.r, tint.g, tint.b], a);
                }
            }
        }
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
                    if v[3] == 0 {
                        continue; // fully clear: nothing to draw
                    }
                    if v[3] == 255 && tint.is_none() {
                        // fully solid: copy the pixel, no blending needed
                        self.px[row * self.w + col] =
                            (v[0] as u32) << 16 | (v[1] as u32) << 8 | v[2] as u32;
                        continue;
                    }
                    [
                        v[0] as f32 / 255.0,
                        v[1] as f32 / 255.0,
                        v[2] as f32 / 255.0,
                        v[3] as f32 / 255.0,
                    ]
                } else if tint.is_some() && (kx > 1.5 || ky > 1.5) {
                    // tinted (text) and shrinking: the same average of a small block, of the
                    // coverage alone
                    let n = kx.max(ky).round().max(2.0) as i32;
                    let mut acc = 0.0f32;
                    for j in 0..n {
                        for i in 0..n {
                            acc += img.coverage(
                                ix - (n as f32 - 1.0) / 2.0 + i as f32,
                                iy - (n as f32 - 1.0) / 2.0 + j as f32,
                            );
                        }
                    }
                    if acc <= 0.0 {
                        continue;
                    }
                    [0.0, 0.0, 0.0, acc / (n * n) as f32]
                } else if tint.is_some() {
                    [0.0, 0.0, 0.0, img.coverage(ix, iy)]
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

    /// Saves the canvas as a PNG (the P key, or a `shot` file from an AI session),
    /// with the screen's colour profile (`icc`) so viewers show the right colours.
    pub fn save_png(&self, path: &std::path::Path, icc: Option<Vec<u8>>) -> std::io::Result<()> {
        let file = std::fs::File::create(path)?;
        let mut info = png::Info::with_size(self.w as u32, self.h as u32);
        info.color_type = png::ColorType::Rgb;
        info.bit_depth = png::BitDepth::Eight;
        info.icc_profile = icc.map(Into::into);
        let enc = png::Encoder::with_info(std::io::BufWriter::new(file), info)
            .map_err(std::io::Error::other)?;
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

#[cfg(test)]
mod arrow_tests {
    use super::*;

    #[test]
    fn an_arrow_covers_its_line_and_head_and_nothing_far_away() {
        let (w, h) = (100usize, 100usize);
        let mut px = vec![0u32; w * h];
        let mut cv = Canvas::new(&mut px, w, h, 1.0);
        cv.arrow(
            (10.0, 50.0),
            (90.0, 50.0),
            6.0,
            20.0,
            Color::rgba8(255, 255, 255, 255),
        );
        let at = |x: usize, y: usize| px[y * w + x] & 255;
        assert_eq!(at(40, 50), 255, "the shaft");
        assert_eq!(
            at(40, 53),
            0,
            "outside the shaft: 3 pt half-width, so 3 pt away is the edge"
        );
        assert!(at(75, 58) > 0, "the head is wider than the shaft");
        assert_eq!(at(75, 70), 0, "beyond the head");
        assert_eq!(at(95, 50), 0, "past the tip");
    }
}
