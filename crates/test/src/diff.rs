//! Pure image comparison for visual snapshots (no GPU needed to test it).

/// How different two images may be and still match. Software and hardware renderers differ
/// by tiny anti-aliasing deltas, so goldens are compared with a small tolerance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tolerance {
    /// Per-channel difference ignored entirely.
    pub channel: u8,
    /// Pixels allowed to exceed `channel` before the images count as different.
    pub pixels: usize,
}

impl Tolerance {
    pub const EXACT: Tolerance = Tolerance {
        channel: 0,
        pixels: 0,
    };
    /// Default for goldens: absorbs adapter AA noise (spike measured max delta 1).
    pub const DEFAULT: Tolerance = Tolerance {
        channel: 2,
        pixels: 0,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffReport {
    pub size_mismatch: Option<((u32, u32), (u32, u32))>,
    pub differing_pixels: usize,
    pub max_channel_delta: u8,
    /// Bounding box `(x, y, w, h)` of differing pixels.
    pub bounds: Option<(u32, u32, u32, u32)>,
}

impl DiffReport {
    pub fn matches(&self, t: Tolerance) -> bool {
        self.size_mismatch.is_none() && self.differing_pixels <= t.pixels
    }
}

/// Compare RGBA8 buffers of `(w, h)`. Pixels whose every channel differs by ≤ `t.channel`
/// are equal. `mask` (same layout) marks pixels to ignore where its byte is non-zero.
pub fn diff(a: (&[u8], u32, u32), b: (&[u8], u32, u32), t: Tolerance) -> DiffReport {
    let ((ab, aw, ah), (bb, bw, bh)) = (a, b);
    if (aw, ah) != (bw, bh) || ab.len() != bb.len() || ab.len() != (aw as usize) * (ah as usize) * 4
    {
        return DiffReport {
            size_mismatch: Some(((aw, ah), (bw, bh))),
            differing_pixels: 0,
            max_channel_delta: 0,
            bounds: None,
        };
    }
    let mut differing = 0;
    let mut max = 0u8;
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (i, (pa, pb)) in ab
        .as_chunks::<4>()
        .0
        .iter()
        .zip(bb.as_chunks::<4>().0.iter())
        .enumerate()
    {
        let delta = pa
            .iter()
            .zip(pb)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0);
        max = max.max(delta);
        if delta > t.channel {
            differing += 1;
            let (x, y) = ((i as u32) % aw, (i as u32) / aw);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    let bounds = (differing > 0).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1));
    DiffReport {
        size_mismatch: None,
        differing_pixels: differing,
        max_channel_delta: max,
        bounds,
    }
}

/// A visual diff image: unchanged pixels dimmed, differing pixels magenta.
pub fn diff_image(a: &[u8], b: &[u8], t: Tolerance) -> Vec<u8> {
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0.iter())
        .flat_map(|(pa, pb)| {
            let delta = pa
                .iter()
                .zip(pb)
                .map(|(x, y)| x.abs_diff(*y))
                .max()
                .unwrap_or(0);
            if delta > t.channel {
                [255, 0, 255, 255]
            } else {
                [pa[0] / 3, pa[1] / 3, pa[2] / 3, 255]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let f = &f;
        (0..h)
            .flat_map(|y| (0..w).flat_map(move |x| f(x, y)))
            .collect()
    }

    #[test]
    fn identical_and_tolerated_noise_match() {
        let a = img(4, 3, |x, y| [x as u8 * 10, y as u8 * 10, 0, 255]);
        let r = diff((&a, 4, 3), (&a, 4, 3), Tolerance::EXACT);
        assert!(r.matches(Tolerance::EXACT));
        let noisy = img(4, 3, |x, y| [x as u8 * 10 + 1, y as u8 * 10, 0, 255]);
        assert!(!diff((&a, 4, 3), (&noisy, 4, 3), Tolerance::EXACT).matches(Tolerance::EXACT));
        assert!(diff((&a, 4, 3), (&noisy, 4, 3), Tolerance::DEFAULT).matches(Tolerance::DEFAULT));
    }

    #[test]
    fn real_changes_report_count_max_and_bounds() {
        let a = img(5, 5, |_, _| [0, 0, 0, 255]);
        let b = img(5, 5, |x, y| {
            if (1..3).contains(&x) && y == 4 {
                [200, 0, 0, 255]
            } else {
                [0, 0, 0, 255]
            }
        });
        let r = diff((&a, 5, 5), (&b, 5, 5), Tolerance::DEFAULT);
        assert_eq!(
            (r.differing_pixels, r.max_channel_delta, r.bounds),
            (2, 200, Some((1, 4, 2, 1)))
        );
        assert!(!r.matches(Tolerance::DEFAULT));
        assert!(r.matches(Tolerance {
            channel: 2,
            pixels: 2
        }));
        let d = diff_image(&a, &b, Tolerance::DEFAULT);
        assert_eq!(
            &d[(4 * 5 + 1) * 4..(4 * 5 + 1) * 4 + 4],
            &[255, 0, 255, 255]
        );
    }

    #[test]
    fn size_mismatch_never_matches() {
        let a = img(2, 2, |_, _| [0; 4]);
        let b = img(3, 2, |_, _| [0; 4]);
        let r = diff(
            (&a, 2, 2),
            (&b, 3, 2),
            Tolerance {
                channel: 255,
                pixels: usize::MAX,
            },
        );
        assert!(
            r.size_mismatch.is_some()
                && !r.matches(Tolerance {
                    channel: 255,
                    pixels: usize::MAX
                })
        );
        let short = vec![0u8; 3];
        assert!(
            diff((&short, 2, 2), (&a, 2, 2), Tolerance::EXACT)
                .size_mismatch
                .is_some(),
            "bad buffer length"
        );
    }
}
