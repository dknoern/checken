//! The app icon, drawn in code: a white hen sitting in a straw nest on a
//! money-green tile, with a golden "nest egg" marked with a dollar sign in
//! front of her. Shapes are signed distance functions, sampled several
//! times per pixel for smooth edges.

type Rgba = [f32; 4];

fn hex(h: u32) -> Rgba {
    [((h >> 16) & 255) as f32 / 255.0, ((h >> 8) & 255) as f32 / 255.0, (h & 255) as f32 / 255.0, 1.0]
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]
}

/// Paints `src` with coverage `a` over `dst` (straight alpha).
fn over(dst: &mut Rgba, src: Rgba, a: f32) {
    let a = (src[3] * a).clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let out_a = a + dst[3] * (1.0 - a);
    for i in 0..3 {
        dst[i] = (src[i] * a + dst[i] * dst[3] * (1.0 - a)) / out_a;
    }
    dst[3] = out_a;
}

/// Distance to a rounded box centred at (cx, cy) with half size (hx, hy).
fn rbox(x: f32, y: f32, cx: f32, cy: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let qx = (x - cx).abs() - hx + r;
    let qy = (y - cy).abs() - hy + r;
    let ox = qx.max(0.0);
    let oy = qy.max(0.0);
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r
}

/// Inside an ellipse grown by `grow` on every side.
fn ellipse(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32, grow: f32) -> bool {
    let (dx, dy) = ((x - cx) / (rx + grow), (y - cy) / (ry + grow));
    dx * dx + dy * dy <= 1.0
}

/// Inside an ellipse turned by `angle` (radians), grown by `grow`.
fn ellipse_rot(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32, angle: f32, grow: f32) -> bool {
    let (s, c) = angle.sin_cos();
    let (px, py) = (x - cx, y - cy);
    ellipse(px * c + py * s, -px * s + py * c, 0.0, 0.0, rx, ry, grow)
}

fn inside_triangle(x: f32, y: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let s = |p: (f32, f32), q: (f32, f32)| (x - q.0) * (p.1 - q.1) - (p.0 - q.0) * (y - q.1);
    let (d1, d2, d3) = (s(a, b), s(b, c), s(c, a));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

const OUTLINE: u32 = 0x3a3128;
const LINE: f32 = 0.009;

/// The hen's feathered parts: tail, body and head, each with a dark outline.
fn hen_white(x: f32, y: f32, grow: f32) -> bool {
    let tail = ellipse_rot(x, y, 0.255, 0.40, 0.055, 0.11, -0.5, grow)
        || ellipse_rot(x, y, 0.215, 0.46, 0.05, 0.10, -0.95, grow)
        || ellipse_rot(x, y, 0.30, 0.37, 0.045, 0.09, -0.15, grow);
    let body = ellipse(x, y, 0.45, 0.53, 0.215, 0.165, grow);
    let neck = ellipse_rot(x, y, 0.585, 0.43, 0.075, 0.11, 0.35, grow);
    let head = ellipse(x, y, 0.625, 0.335, 0.088, 0.088, grow);
    tail || body || neck || head
}

/// The colour of the icon at one point of the unit square (y grows down).
fn sample(x: f32, y: f32) -> Rgba {
    let mut c: Rgba = [0.0, 0.0, 0.0, 0.0];
    // Tile: soft drop shadow, then a money-green gradient with a sheen.
    let tile = rbox(x, y, 0.5, 0.5, 0.42, 0.42, 0.095);
    let shadow = rbox(x, y, 0.5, 0.515, 0.42, 0.42, 0.095);
    if shadow < 0.035 {
        over(&mut c, [0.0, 0.0, 0.0, 0.28], (1.0 - (shadow.max(0.0) / 0.035)).powi(2));
    }
    if tile >= 0.0 {
        return c;
    }
    let mut bg = mix(hex(0x2fa86e), hex(0x0f6e5e), (x * 0.35 + y * 0.9 - 0.1) / 1.0);
    bg = mix(bg, [1.0, 1.0, 1.0, 1.0], (0.45 - y).max(0.0) * 0.25);
    over(&mut c, bg, 1.0);
    if tile > -0.008 {
        over(&mut c, [1.0, 1.0, 1.0, 0.18], 1.0);
    }
    // A faint ring of light behind the hen.
    let glow = ((x - 0.47).powi(2) + (y - 0.48).powi(2)).sqrt();
    if glow < 0.33 {
        over(&mut c, [1.0, 1.0, 0.85, 0.10 * (1.0 - glow / 0.33)], 1.0);
    }

    // Comb, behind the head.
    let comb = [(0.575, 0.258, 0.034), (0.618, 0.240, 0.040), (0.662, 0.255, 0.034)];
    if comb.iter().any(|(cx, cy, r)| ellipse(x, y, *cx, *cy, *r, *r, LINE)) {
        over(&mut c, hex(OUTLINE), 1.0);
    }
    if comb.iter().any(|(cx, cy, r)| ellipse(x, y, *cx, *cy, *r, *r, 0.0)) {
        over(&mut c, hex(0xe2402f), 1.0);
    }

    // Feathers: outline, then white shaded toward the bottom.
    if hen_white(x, y, LINE) {
        over(&mut c, hex(OUTLINE), 1.0);
    }
    if hen_white(x, y, 0.0) {
        over(&mut c, mix(hex(0xfffdf7), hex(0xe9e1d0), (y - 0.30) / 0.4), 1.0);
    }
    // Wing, a soft cream feather with an outline.
    if ellipse_rot(x, y, 0.415, 0.53, 0.125, 0.07, 0.18, LINE * 0.8) {
        over(&mut c, hex(0x8c7b63), 1.0);
    }
    if ellipse_rot(x, y, 0.415, 0.53, 0.125, 0.07, 0.18, 0.0) {
        over(&mut c, hex(0xefe5cf), 1.0);
    }
    // Beak and wattle.
    let beak = ((0.700, 0.318), (0.785, 0.345), (0.700, 0.372));
    if inside_triangle(x, y, beak.0, beak.1, beak.2) {
        over(&mut c, hex(0xf3a712), 1.0);
    }
    if ellipse(x, y, 0.692, 0.402, 0.020, 0.032, LINE * 0.7) {
        over(&mut c, hex(OUTLINE), 1.0);
    }
    if ellipse(x, y, 0.692, 0.402, 0.020, 0.032, 0.0) {
        over(&mut c, hex(0xe2402f), 1.0);
    }
    // Eye with a glint.
    if ellipse(x, y, 0.640, 0.322, 0.016, 0.018, 0.0) {
        over(&mut c, hex(0x1d1a17), 1.0);
    }
    if ellipse(x, y, 0.645, 0.316, 0.005, 0.005, 0.0) {
        over(&mut c, hex(0xffffff), 1.0);
    }

    // Straw nest around her lower half.
    if ellipse(x, y, 0.47, 0.705, 0.30, 0.105, LINE) {
        over(&mut c, hex(0x5a3a18), 1.0);
    }
    if ellipse(x, y, 0.47, 0.705, 0.30, 0.105, 0.0) {
        let shade = mix(hex(0xc98a43), hex(0x9a6328), (y - 0.62) / 0.18);
        over(&mut c, shade, 1.0);
        // Wavy strands of straw running around the nest.
        let strand = (y * 150.0 + (x * 21.0).sin() * 4.0 + (x * 47.0).sin() * 1.2).sin();
        if strand > 0.72 {
            over(&mut c, hex(0xeec27c), 0.85);
        } else if strand < -0.8 {
            over(&mut c, hex(0x7a4c1c), 0.55);
        }
        // A few loose strands crossing the other way.
        let cross = (x * 95.0 - y * 210.0 + (y * 30.0).sin() * 3.0).sin();
        if cross > 0.93 {
            over(&mut c, hex(0xdba862), 0.7);
        }
    }

    // The golden nest egg, with its shadow and a dollar sign.
    let (ex, ey, erx, ery) = (0.665, 0.700, 0.088, 0.112);
    if ellipse(x, y, ex + 0.012, ey + 0.016, erx, ery, 0.0) {
        over(&mut c, [0.15, 0.08, 0.0, 0.35], 1.0);
    }
    if ellipse(x, y, ex, ey, erx, ery, LINE) {
        over(&mut c, hex(0x8a5a06), 1.0);
    }
    if ellipse(x, y, ex, ey, erx, ery, 0.0) {
        let gold = mix(hex(0xffe27a), hex(0xe09b12), (y - (ey - ery)) / (2.0 * ery));
        over(&mut c, gold, 1.0);
        if ellipse(x, y, ex - 0.03, ey - 0.05, 0.022, 0.032, 0.0) {
            over(&mut c, [1.0, 1.0, 0.95, 0.55], 1.0);
        }
        // "$": two open rings making an S, and a bar through them.
        let (rr, t, dd) = (0.024, 0.0105, 0.024);
        let ring = |ox: f32, oy: f32| (((x - ox).powi(2) + (y - oy).powi(2)).sqrt() - rr).abs() < t / 2.0;
        let (cx, cy) = (ex, ey + 0.008);
        let top = ring(cx, cy - dd) && !(x > cx && y > cy - dd);
        let bottom = ring(cx, cy + dd) && !(x < cx && y < cy + dd);
        let bar = (x - cx).abs() < t * 0.42 && (y - cy).abs() < dd + rr + t * 1.2;
        if top || bottom || bar {
            over(&mut c, hex(0x8f5a03), 1.0);
        }
    }
    c
}

/// The app icon picture, app/assets/icon.png, built into the program. Replace
/// that file and build again to change the icon everywhere (Dock, window,
/// About box).
const ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

/// The icon as straight-alpha RGBA pixels, `size` × `size`: assets/icon.png
/// made into a macOS-style icon (see `mac_icon`), or the drawn icon if that
/// file can't be read.
pub fn pixels(size: u32) -> Vec<u8> {
    static PREPARED: std::sync::OnceLock<Option<image::RgbaImage>> = std::sync::OnceLock::new();
    let prepared = PREPARED.get_or_init(|| image::load_from_memory(ICON_PNG).ok().map(|img| mac_icon(&img.to_rgba8())));
    match prepared {
        Some(img) if size == img.width() => img.as_raw().clone(),
        Some(img) => image::imageops::resize(img, size, size, image::imageops::FilterType::Lanczos3).into_raw(),
        None => drawn_pixels(size),
    }
}

/// macOS icon grid: on a 1024-pixel canvas the rounded tile is 824 pixels
/// wide with 100 pixels of clear space around it (Apple's app icon template).
const CANVAS: u32 = 1024;
const TILE: f32 = 824.0;

/// Turns a picture into a Dock-ready icon:
/// 1. The plain background around the tile (white, light gray, a soft drop
///    shadow, or already transparent) is cleared. It's found by spreading in
///    from the picture's edges through pale, colourless pixels, so it stops at
///    the tile's colour and at the artwork's outlines: parts of the drawing
///    that hang over the tile (a head, claws) stay.
/// 2. The cut edge is smoothed so no pale fringe is left.
/// 3. The tile is found (the rows and columns that are mostly kept) and the
///    picture is scaled and placed so the tile sits on the macOS icon grid.
pub fn mac_icon(src: &image::RgbaImage) -> image::RgbaImage {
    let (w, h) = src.dimensions();
    let idx = |x: u32, y: u32| (y * w + x) as usize;
    let is_bg = |p: &image::Rgba<u8>| {
        let [r, g, b, a] = p.0;
        let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
        a < 16 || (mn >= 140 && mx - mn <= 35)
    };
    // 1. Spread in from the edges.
    let mut removed = vec![false; (w * h) as usize];
    let mut stack: Vec<(u32, u32)> = Vec::new();
    for x in 0..w {
        stack.push((x, 0));
        stack.push((x, h - 1));
    }
    for y in 0..h {
        stack.push((0, y));
        stack.push((w - 1, y));
    }
    while let Some((x, y)) = stack.pop() {
        let i = idx(x, y);
        if removed[i] || !is_bg(src.get_pixel(x, y)) {
            continue;
        }
        removed[i] = true;
        if x > 0 {
            stack.push((x - 1, y));
        }
        if x + 1 < w {
            stack.push((x + 1, y));
        }
        if y > 0 {
            stack.push((x, y - 1));
        }
        if y + 1 < h {
            stack.push((x, y + 1));
        }
    }
    // 2. Smooth the cut: an edge pixel's opacity is the share of kept pixels
    // around it, and it takes the colour of its most colourful kept neighbour
    // so the pale blend from the old background doesn't show.
    let mut out = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            if removed[idx(x, y)] {
                continue;
            }
            let mut kept = 0u32;
            let mut total = 0u32;
            let mut edge = false;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    total += 1;
                    if removed[idx(nx as u32, ny as u32)] {
                        edge = true;
                    } else {
                        kept += 1;
                    }
                }
            }
            let mut p = *src.get_pixel(x, y);
            if edge {
                let mut best = p;
                let mut best_sat = 0i32;
                for dy in -2i32..=2 {
                    for dx in -2i32..=2 {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 || removed[idx(nx as u32, ny as u32)] {
                            continue;
                        }
                        let q = src.get_pixel(nx as u32, ny as u32);
                        let [r, g, b, _] = q.0;
                        let sat = r.max(g).max(b) as i32 - r.min(g).min(b) as i32;
                        if sat > best_sat {
                            best_sat = sat;
                            best = *q;
                        }
                    }
                }
                p = best;
                p.0[3] = ((p.0[3] as u32 * kept) / total.max(1)) as u8;
            }
            out.put_pixel(x, y, p);
        }
    }
    // 3. Find the tile: rows and columns that are mostly kept.
    let row_counts: Vec<u32> = (0..h).map(|y| (0..w).filter(|&x| !removed[idx(x, y)]).count() as u32).collect();
    let col_counts: Vec<u32> = (0..w).map(|x| (0..h).filter(|&y| !removed[idx(x, y)]).count() as u32).collect();
    let span = |counts: &[u32]| -> Option<(u32, u32)> {
        let max = *counts.iter().max()?;
        let lo = counts.iter().position(|&c| c * 10 >= max * 6)? as u32;
        let hi = counts.iter().rposition(|&c| c * 10 >= max * 6)? as u32;
        (hi > lo).then_some((lo, hi + 1))
    };
    let (Some((top, bottom)), Some((left, right))) = (span(&row_counts), span(&col_counts)) else {
        return image::imageops::resize(&out, CANVAS, CANVAS, image::imageops::FilterType::Lanczos3);
    };
    let tile = (right - left).max(bottom - top) as f32;
    let scale = TILE / tile;
    let (nw, nh) = (((w as f32 * scale).round() as u32).max(1), ((h as f32 * scale).round() as u32).max(1));
    let scaled = image::imageops::resize(&out, nw, nh, image::imageops::FilterType::Lanczos3);
    // Centre the tile on the canvas; overhanging art may use the margin.
    let cx = (left + right) as f32 / 2.0 * scale;
    let cy = (top + bottom) as f32 / 2.0 * scale;
    let ox = (CANVAS as f32 / 2.0 - cx).round() as i64;
    let oy = (CANVAS as f32 / 2.0 - cy).round() as i64;
    let mut canvas = image::RgbaImage::new(CANVAS, CANVAS);
    image::imageops::overlay(&mut canvas, &scaled, ox, oy);
    canvas
}

/// The drawn hen-in-a-nest icon, used when assets/icon.png can't be read.
fn drawn_pixels(size: u32) -> Vec<u8> {
    const SS: u32 = 4;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut acc = [0.0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    let s = sample(x, y);
                    // Average premultiplied colour so edges don't darken.
                    for i in 0..3 {
                        acc[i] += s[i] * s[3];
                    }
                    acc[3] += s[3];
                }
            }
            let n = (SS * SS) as f32;
            let a = acc[3] / n;
            for i in 0..3 {
                let v = if a > 0.0 { acc[i] / n / a } else { 0.0 };
                out.push((v * 255.0).round().clamp(0.0, 255.0) as u8);
            }
            out.push((a * 255.0).round() as u8);
        }
    }
    out
}

pub fn icon_data() -> eframe::egui::IconData {
    let size = 256;
    eframe::egui::IconData { rgba: pixels(size), width: size, height: size }
}

#[cfg(test)]
mod tests {
    #[test]
    fn white_surround_is_cleared_and_the_tile_sits_on_the_mac_grid() {
        // A white picture with a coloured tile from 100 to 900 and a gray shadow under it.
        let mut img = image::RgbaImage::from_pixel(1000, 1000, image::Rgba([255, 255, 255, 255]));
        for y in 905..920 {
            for x in 100..900 {
                img.put_pixel(x, y, image::Rgba([190, 190, 190, 255]));
            }
        }
        for y in 100..900 {
            for x in 100..900 {
                img.put_pixel(x, y, image::Rgba([250, 140, 0, 255]));
            }
        }
        // Art hanging over the top edge, with a dark outline.
        for y in 60..100 {
            for x in 480..520 {
                img.put_pixel(x, y, image::Rgba([30, 20, 10, 255]));
            }
        }
        let out = super::mac_icon(&img);
        assert_eq!(out.dimensions(), (1024, 1024));
        assert_eq!(out.get_pixel(5, 5).0[3], 0, "corner is clear");
        assert_eq!(out.get_pixel(512, 990).0[3], 0, "shadow is cleared");
        assert_eq!(out.get_pixel(512, 512).0, [250, 140, 0, 255]);
        // The tile spans about 100..924.
        assert!(out.get_pixel(104, 512).0[3] > 200 && out.get_pixel(95, 512).0[3] < 30);
        // The overhanging art is kept above the tile.
        assert!(out.get_pixel(512, 80).0[3] > 200);
    }
}
