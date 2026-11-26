//! Pure image comparison: changed-pixel count, max channel delta and a heat map.
//! No GPUI; runs on a background thread for large images.

/// Decoded RGBA8 pixels, row-major.
#[derive(Clone, PartialEq, Eq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pixels({}x{})", self.width, self.height)
    }
}

impl Pixels {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Self> {
        (rgba.len() as u64 == u64::from(width) * u64::from(height) * 4).then_some(Self {
            width,
            height,
            rgba,
        })
    }
    pub fn filled(width: u32, height: u32, color: [u8; 4]) -> Self {
        let rgba = color.repeat(width as usize * height as usize);
        Self {
            width,
            height,
            rgba,
        }
    }
    /// The RGBA value at `(x, y)`, if inside.
    pub fn get(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let p = self.rgba.get(i..i + 4)?;
        Some([p[0], p[1], p[2], p[3]])
    }
    pub fn set(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if x < self.width && y < self.height {
            let i = (y as usize * self.width as usize + x as usize) * 4;
            if let Some(p) = self.rgba.get_mut(i..i + 4) {
                p.copy_from_slice(&color);
            }
        }
    }
}

/// What changed between two images.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiffSummary {
    /// Pixels whose RGBA differs, including pixels outside the other image's bounds.
    pub changed: u64,
    /// Pixels compared: the union of both sizes.
    pub total: u64,
    /// The largest per-channel absolute difference (255 for pixels only one image has).
    pub max_delta: u8,
    pub before: (u32, u32),
    pub after: (u32, u32),
}

impl DiffSummary {
    pub fn size_changed(&self) -> bool {
        self.before != self.after
    }
    /// Changed pixels as a fraction of compared pixels.
    pub fn ratio(&self) -> f64 {
        if self.total == 0 {
            0.
        } else {
            self.changed as f64 / self.total as f64
        }
    }
}

/// Compare two images over the union of their sizes. Returns the summary and a heat map:
/// unchanged pixels are a dimmed grey of the after image, changed pixels red with an
/// intensity that grows with the channel delta. Either side may be absent (an added or a
/// deleted golden): every pixel of the other side counts as changed.
pub fn diff(before: Option<&Pixels>, after: Option<&Pixels>) -> (DiffSummary, Pixels) {
    let size = |p: Option<&Pixels>| p.map_or((0, 0), |p| (p.width, p.height));
    let (bw, bh) = size(before);
    let (aw, ah) = size(after);
    let (w, h) = (bw.max(aw), bh.max(ah));
    let mut heat = Pixels::filled(w, h, [0, 0, 0, 255]);
    let mut summary = DiffSummary {
        before: (bw, bh),
        after: (aw, ah),
        total: u64::from(w) * u64::from(h),
        ..Default::default()
    };
    for y in 0..h {
        for x in 0..w {
            let b = before.and_then(|p| p.get(x, y));
            let a = after.and_then(|p| p.get(x, y));
            let delta = match (b, a) {
                (Some(b), Some(a)) => b
                    .iter()
                    .zip(a)
                    .map(|(b, a)| b.abs_diff(a))
                    .max()
                    .unwrap_or(0),
                (None, None) => 0,
                _ => 255,
            };
            if delta > 0 {
                summary.changed += 1;
                summary.max_delta = summary.max_delta.max(delta);
                // Visible even for a one-step delta.
                let red = 96u16 + u16::from(delta) * 159 / 255;
                heat.set(x, y, [red.min(255) as u8, 0, 0, 255]);
            } else if let Some(a) = a {
                let grey =
                    ((u16::from(a[0]) + u16::from(a[1]) + u16::from(a[2])) / 3 / 4 + 32) as u8;
                heat.set(x, y, [grey, grey, grey, 255]);
            }
        }
    }
    (summary, heat)
}

pub fn decode_png(bytes: &[u8]) -> anyhow::Result<Pixels> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)?.to_rgba8();
    let (width, height) = image.dimensions();
    Ok(Pixels {
        width,
        height,
        rgba: image.into_raw(),
    })
}

pub fn encode_png(pixels: &Pixels) -> anyhow::Result<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::write_buffer_with_format(
        &mut out,
        &pixels.rgba,
        pixels.width,
        pixels.height,
        image::ExtendedColorType::Rgba8,
        image::ImageFormat::Png,
    )?;
    Ok(out.into_inner())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test assertions")]
mod tests {
    use super::*;

    #[test]
    fn identical_images_have_no_changes() {
        let a = Pixels::filled(4, 3, [10, 20, 30, 255]);
        let (summary, heat) = diff(Some(&a), Some(&a.clone()));
        assert_eq!(summary.changed, 0);
        assert_eq!(summary.max_delta, 0);
        assert_eq!(summary.total, 12);
        assert!(!summary.size_changed());
        assert_eq!((heat.width, heat.height), (4, 3));
        assert!(
            heat.get(0, 0).unwrap()[0] < 96,
            "unchanged pixels are dim grey"
        );
    }

    #[test]
    fn counts_changed_pixels_and_the_largest_channel_delta() {
        let a = Pixels::filled(4, 4, [100, 100, 100, 255]);
        let mut b = a.clone();
        b.set(1, 1, [101, 100, 100, 255]);
        b.set(2, 3, [100, 40, 100, 255]);
        let (summary, heat) = diff(Some(&a), Some(&b));
        assert_eq!(summary.changed, 2);
        assert_eq!(summary.max_delta, 60);
        assert!((summary.ratio() - 2. / 16.).abs() < 1e-9);
        let one_step = heat.get(1, 1).unwrap();
        let big = heat.get(2, 3).unwrap();
        assert!(
            one_step[0] >= 96 && one_step[1] == 0,
            "a one-step change is visible"
        );
        assert!(big[0] > one_step[0], "larger deltas are hotter");
    }

    #[test]
    fn size_changes_and_missing_sides_count_every_uncovered_pixel() {
        let small = Pixels::filled(2, 2, [0, 0, 0, 255]);
        let large = Pixels::filled(3, 2, [0, 0, 0, 255]);
        let (summary, _) = diff(Some(&small), Some(&large));
        assert!(summary.size_changed());
        assert_eq!(
            (summary.changed, summary.total, summary.max_delta),
            (2, 6, 255)
        );
        let (added, _) = diff(None, Some(&small));
        assert_eq!((added.changed, added.total), (4, 4));
        let (none, heat) = diff(None, None);
        assert_eq!((none.changed, none.total, heat.width), (0, 0, 0));
    }

    #[test]
    fn png_round_trip() {
        let mut p = Pixels::filled(3, 2, [1, 2, 3, 255]);
        p.set(2, 1, [200, 100, 50, 128]);
        let back = decode_png(&encode_png(&p).unwrap()).unwrap();
        assert_eq!(back, p);
        assert!(decode_png(b"not a png").is_err());
        assert!(Pixels::new(2, 2, vec![0; 15]).is_none());
    }
}
