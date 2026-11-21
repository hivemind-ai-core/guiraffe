//! Tray icon images and badges. Pure pixel work, no platform code.
use crate::TrayError;

/// A straight (not premultiplied) RGBA8 image, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaIcon {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl RgbaIcon {
    /// Fails unless `rgba` holds exactly `width * height * 4` bytes and the size is 1..=1024.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, TrayError> {
        let ok = (1..=1024).contains(&width)
            && (1..=1024).contains(&height)
            && rgba.len() as u64 == u64::from(width) * u64::from(height) * 4;
        if !ok {
            return Err(TrayError::InvalidIcon(format!(
                "{width}x{height} needs {} bytes, got {}",
                u64::from(width) * u64::from(height) * 4,
                rgba.len()
            )));
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
    /// The pixel at (x, y), or `None` outside.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = ((y * self.width + x) * 4) as usize;
        self.rgba.get(i..i + 4).and_then(|p| p.try_into().ok())
    }

    /// A filled anti-aliased disc of `color` on transparency: a simple generated icon.
    pub fn disc(size: u32, color: [u8; 4]) -> Self {
        let size = size.clamp(1, 1024);
        let mut icon = Self {
            width: size,
            height: size,
            rgba: vec![0; (size * size * 4) as usize],
        };
        let r = size as f32 / 2.0;
        icon.blend_disc(r, r, r - 0.5, color);
        icon
    }

    /// ARGB32 in network byte order: the StatusNotifierItem pixmap format.
    pub fn to_argb32_be(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.rgba.len());
        for p in self.rgba.as_chunks::<4>().0 {
            out.extend_from_slice(&[p[3], p[0], p[1], p[2]]);
        }
        out
    }

    /// A copy with `badge` drawn in the top-right corner in `color`.
    pub fn with_badge(&self, badge: Badge, color: [u8; 4]) -> Self {
        let mut out = self.clone();
        out.draw_badge(badge, color);
        out
    }

    /// Only the badge, on a transparent icon-sized canvas (for overlay-icon hosts).
    pub fn badge_overlay(width: u32, height: u32, badge: Badge, color: [u8; 4]) -> Self {
        let (width, height) = (width.clamp(1, 1024), height.clamp(1, 1024));
        let mut out = Self {
            width,
            height,
            rgba: vec![0; (width * height * 4) as usize],
        };
        out.draw_badge(badge, color);
        out
    }

    fn draw_badge(&mut self, badge: Badge, color: [u8; 4]) {
        let label = match badge {
            Badge::None => return,
            Badge::Dot => None,
            Badge::Count(0) => return,
            Badge::Count(n) if n > 9 => Some("9+".to_string()),
            Badge::Count(n) => Some(n.to_string()),
        };
        let size = self.width.min(self.height) as f32;
        let r = match label {
            Some(_) => (size * 0.34).max(4.0),
            None => (size * 0.22).max(2.0),
        };
        let (cx, cy) = (self.width as f32 - r, r);
        // A ring in the icon's transparent cut-out keeps the badge readable on any icon.
        self.cut_disc(cx, cy, r + (size * 0.06).max(1.0));
        self.blend_disc(cx, cy, r, color);
        if let Some(text) = label {
            self.draw_text(&text, cx, cy, r, [255, 255, 255, 255]);
        }
    }

    /// Source-over a disc with anti-aliased edges.
    fn blend_disc(&mut self, cx: f32, cy: f32, r: f32, color: [u8; 4]) {
        self.for_disc(cx, cy, r, |dst, coverage| {
            blend(dst, color, coverage);
        });
    }

    /// Clear alpha inside a disc (destination-out).
    fn cut_disc(&mut self, cx: f32, cy: f32, r: f32) {
        self.for_disc(cx, cy, r, |dst, coverage| {
            dst[3] = (f32::from(dst[3]) * (1.0 - coverage)).round() as u8;
        });
    }

    fn for_disc(&mut self, cx: f32, cy: f32, r: f32, mut f: impl FnMut(&mut [u8], f32)) {
        let (w, h) = (self.width as i64, self.height as i64);
        let x0 = ((cx - r - 1.0).floor() as i64).clamp(0, w);
        let x1 = ((cx + r + 1.0).ceil() as i64).clamp(0, w);
        let y0 = ((cy - r - 1.0).floor() as i64).clamp(0, h);
        let y1 = ((cy + r + 1.0).ceil() as i64).clamp(0, h);
        for y in y0..y1 {
            for x in x0..x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let coverage = (r - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    let i = ((y * w + x) * 4) as usize;
                    if let Some(px) = self.rgba.get_mut(i..i + 4) {
                        f(px, coverage);
                    }
                }
            }
        }
    }

    /// Draw `text` (digits and '+') centered at (cx, cy) with a 3×5 pixel font scaled to fit.
    fn draw_text(&mut self, text: &str, cx: f32, cy: f32, r: f32, color: [u8; 4]) {
        let glyphs: Vec<[u8; 5]> = text.chars().filter_map(glyph).collect();
        if glyphs.is_empty() {
            return;
        }
        let cols = glyphs.len() as f32 * 4.0 - 1.0;
        let scale = ((r * 1.3) / 5.0).min((r * 1.6) / cols).floor().max(1.0);
        let width = cols * scale;
        let height = 5.0 * scale;
        let left = (cx - width / 2.0).round();
        let top = (cy - height / 2.0).round();
        for (g, rows) in glyphs.iter().enumerate() {
            for (row, bits) in rows.iter().enumerate() {
                for col in 0..3u8 {
                    if bits & (0b100 >> col) == 0 {
                        continue;
                    }
                    let x = left + (g as f32 * 4.0 + f32::from(col)) * scale;
                    let y = top + row as f32 * scale;
                    self.fill_rect(x as i64, y as i64, scale as i64, scale as i64, color);
                }
            }
        }
    }

    fn fill_rect(&mut self, x: i64, y: i64, w: i64, h: i64, color: [u8; 4]) {
        let (iw, ih) = (self.width as i64, self.height as i64);
        for yy in y.max(0)..(y + h).min(ih) {
            for xx in x.max(0)..(x + w).min(iw) {
                let i = ((yy * iw + xx) * 4) as usize;
                if let Some(px) = self.rgba.get_mut(i..i + 4) {
                    blend(px, color, 1.0);
                }
            }
        }
    }
}

fn blend(dst: &mut [u8], src: [u8; 4], coverage: f32) {
    let sa = f32::from(src[3]) / 255.0 * coverage;
    let da = f32::from(dst[3]) / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        dst.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }
    for c in 0..3 {
        let s = f32::from(src[c]);
        let d = f32::from(dst[c]);
        dst[c] = ((s * sa + d * da * (1.0 - sa)) / out_a)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
    dst[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// 3×5 bitmap glyphs, top row first, MSB = left column.
fn glyph(c: char) -> Option<[u8; 5]> {
    Some(match c {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b011, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '+' => [0b000, 0b010, 0b111, 0b010, 0b000],
        _ => return None,
    })
}

/// A count or dot drawn over the tray icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Badge {
    #[default]
    None,
    Dot,
    /// `0` hides the badge; above 9 shows "9+".
    Count(u32),
}

/// The tray icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayIcon {
    Rgba(RgbaIcon),
    /// A freedesktop icon-theme name (Linux only; other platforms show nothing).
    Named(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [u8; 4] = [224, 49, 49, 255];
    const BLUE: [u8; 4] = [34, 139, 230, 255];

    #[test]
    fn rejects_bad_buffers() {
        assert!(RgbaIcon::new(2, 2, vec![0; 16]).is_ok());
        assert!(RgbaIcon::new(2, 2, vec![0; 15]).is_err());
        assert!(RgbaIcon::new(0, 2, vec![]).is_err());
        assert!(RgbaIcon::new(2000, 1, vec![0; 8000]).is_err());
    }

    #[test]
    fn argb_is_network_order() {
        let icon = RgbaIcon::new(1, 1, vec![1, 2, 3, 4]).expect("icon");
        assert_eq!(icon.to_argb32_be(), [4, 1, 2, 3]);
    }

    #[test]
    fn disc_is_opaque_in_the_middle_and_clear_in_the_corner() {
        let icon = RgbaIcon::disc(32, BLUE);
        assert_eq!(icon.pixel(16, 16), Some(BLUE));
        assert_eq!(icon.pixel(0, 0).map(|p| p[3]), Some(0));
        assert_eq!(icon.pixel(32, 0), None);
    }

    #[test]
    fn badge_draws_top_right_and_leaves_the_rest() {
        let base = RgbaIcon::disc(32, BLUE);
        let dot = base.with_badge(Badge::Dot, RED);
        let r = 32.0 * 0.22;
        let (bx, by) = ((32.0 - r) as u32, r as u32);
        assert_eq!(dot.pixel(bx, by), Some(RED));
        assert_eq!(dot.pixel(4, 28), base.pixel(4, 28), "bottom-left untouched");
        assert_eq!(dot.pixel(16, 16), base.pixel(16, 16));
        assert_eq!(base.with_badge(Badge::None, RED), base);
        assert_eq!(base.with_badge(Badge::Count(0), RED), base);
    }

    #[test]
    fn count_badge_has_white_digits() {
        let base = RgbaIcon::disc(64, BLUE);
        let three = base.with_badge(Badge::Count(3), RED);
        let many = base.with_badge(Badge::Count(42), RED);
        assert_ne!(three, many, "9+ differs from 3");
        let white = |icon: &RgbaIcon| {
            (0..64)
                .flat_map(|y| (0..64).map(move |x| (x, y)))
                .filter(|&(x, y)| icon.pixel(x, y) == Some([255, 255, 255, 255]))
                .count()
        };
        assert!(white(&three) > 0);
        assert!(white(&many) > white(&three), "two glyphs paint more");
    }

    #[test]
    fn overlay_is_transparent_except_the_badge() {
        let o = RgbaIcon::badge_overlay(32, 32, Badge::Dot, RED);
        assert_eq!(o.pixel(2, 30).map(|p| p[3]), Some(0));
        assert_eq!(o.pixel(27, 5).map(|p| p[3]), Some(255));
    }
}
