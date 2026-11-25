//! Reuse the coverage of a dense row of plain diamonds across matching lanes.
//! Keep one strip per editor. Different rows in the same frame use the vector path.
use gpui_kit::{App, Bounds, Hsla, Pixels, RenderImage, Rgba, Window, point, px, size};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct DiamondStrip {
    centers: Vec<f32>,
    image: Option<Arc<RenderImage>>,
    pub(super) retired: Option<Arc<RenderImage>>,
    color: Option<Hsla>,
    scale: f32,
    width: usize,
    phase: f32,
}

impl DiamondStrip {
    pub fn release(&mut self, cx: &mut App) {
        for old in [self.image.take(), self.retired.take()]
            .into_iter()
            .flatten()
        {
            cx.drop_image(old, None);
        }
    }

    pub fn paint(
        &mut self,
        centers: &[f32],
        bounds: Bounds<Pixels>,
        center_y: f32,
        color: Hsla,
        can_replace: bool,
        window: &mut Window,
    ) -> bool {
        let scale = window.scale_factor();
        let left = (f32::from(bounds.left()) * scale).floor();
        let width = ((f32::from(bounds.right()) * scale).ceil() - left) as usize;
        let radius = 4. * scale;
        let cy = center_y * scale;
        let top = (cy - radius).floor();
        let phase = cy - top;
        let height = ((cy + radius).ceil() - top) as usize;
        if !(1..=4096).contains(&width) || !(1..=64).contains(&height) {
            return false;
        }
        if self.image.is_none()
            || self.centers != centers
            || self.color != Some(color)
            || self.scale != scale
            || self.width != width
            || self.phase != phase
        {
            if !can_replace || self.retired.is_some() {
                return false;
            }
            let mut pixels = vec![0; width * height * 4];
            let rgba = Rgba::from(color);
            rasterize(&mut pixels, width, centers, phase, radius, rgba);
            let Some(buffer) = image::RgbaImage::from_vec(width as u32, height as u32, pixels)
            else {
                return false;
            };
            let image = Arc::new(RenderImage::new([image::Frame::new(buffer)]));
            self.retired = self.image.replace(image);
            self.centers.clear();
            self.centers.extend_from_slice(centers);
            self.color = Some(color);
            self.phase = phase;
            self.scale = scale;
            self.width = width;
        }
        let rect = Bounds::new(
            point(px(left / scale), px(top / scale)),
            size(px(width as f32 / scale), px(height as f32 / scale)),
        );
        self.image.as_ref().is_some_and(|image| {
            window
                .paint_image(rect, rect, Default::default(), image.clone(), 0, false)
                .is_ok()
        })
    }
}

fn rasterize(pixels: &mut [u8], width: usize, centers: &[f32], y: f32, radius: f32, color: Rgba) {
    let rgb = [color.b, color.g, color.r].map(|c| (c * 255. + 0.5) as u8);
    let opaque = [rgb[0], rgb[1], rgb[2], (color.a * 255. + 0.5) as u8];
    let stride = width * 4;
    let height = pixels.len() / stride;
    let symmetric = y * 2. == height as f32;
    let lines = if symmetric {
        height.div_ceil(2)
    } else {
        height
    };
    for (row, line) in pixels.chunks_exact_mut(stride).take(lines).enumerate() {
        let dy = (row as f32 + 0.5 - y).abs();
        let half = radius - dy;
        let line = line.as_chunks_mut::<4>().0;
        for &x in centers {
            let first = (x - half - 1.).floor().clamp(0., width as f32) as usize;
            let end = (x + half + 1.).ceil().clamp(0., width as f32) as usize;
            for (index, pixel) in line[first..end].iter_mut().enumerate() {
                let dx = ((first + index) as f32 + 0.5 - x).abs();
                let distance = half - dx;
                if distance >= 1. {
                    *pixel = opaque;
                    continue;
                }
                if distance <= -1. {
                    continue;
                }
                // A 45-degree edge cuts a triangle off a unit pixel. At a tip
                // two edges may cross that pixel, so integrate its short spans.
                let coverage = if dx >= 0.5 && dy >= 0.5 {
                    let edge = 1. - distance.abs();
                    let triangle = 0.5 * edge * edge;
                    if distance < 0. {
                        triangle
                    } else {
                        1. - triangle
                    }
                } else {
                    let col = (first + index) as f32;
                    [0.125, 0.375, 0.625, 0.875]
                        .iter()
                        .map(|sample| {
                            let half = (radius - (row as f32 + sample - y).abs()).max(0.);
                            ((x + half).min(col + 1.) - (x - half).max(col)).max(0.)
                        })
                        .sum::<f32>()
                        * 0.25
                };
                let alpha = (coverage * 255. * color.a + 0.5) as u8;
                if alpha > pixel[3] {
                    *pixel = [rgb[0], rgb[1], rgb[2], alpha];
                }
            }
        }
    }
    if symmetric {
        for row in lines..height {
            let source = (height - row - 1) * stride;
            pixels.copy_within(source..source + stride, row * stride);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diamond_coverage_is_symmetric_clipped_and_uses_bgra() {
        let color = Rgba {
            r: 1.,
            g: 0.5,
            b: 0.,
            a: 1.,
        };
        let mut pixels = vec![0; 12 * 8 * 4];
        rasterize(&mut pixels, 12, &[6.], 4., 4., color);
        let pixel = |x: usize, y: usize| &pixels[(y * 12 + x) * 4..(y * 12 + x + 1) * 4];
        assert_eq!(pixel(5, 4), [0, 128, 255, 255]);
        assert_eq!(pixel(0, 4), [0; 4]);
        assert_eq!(pixel(5, 0), [0, 128, 255, 128]);
        for x in 0..12 {
            for y in 0..8 {
                assert_eq!(pixel(x, y), pixel(11 - x, 7 - y));
            }
        }
        let mut clipped = vec![0; 3 * 8 * 4];
        rasterize(&mut clipped, 3, &[-0.25], 4., 4., color);
        assert_eq!(clipped[(3 * 3) * 4 + 3], 255);
    }
}
