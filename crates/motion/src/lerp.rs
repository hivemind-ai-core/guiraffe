//! Interpolation between values.

use gpui_kit::{Hsla, Pixels, Point, Size, px};
use guiraffe_core::Rgb;

/// Linear interpolation. `t` may lie outside `[0, 1]` (springs and back-easings overshoot).
pub trait Lerp: Clone {
    fn lerp(&self, to: &Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        self + (to - self) * t
    }
}

impl Lerp for Pixels {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        px(f32::from(*self).lerp(&f32::from(*to), t))
    }
}

impl<T: Lerp + Default + std::fmt::Debug + PartialEq> Lerp for Point<T> {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        Point {
            x: self.x.lerp(&to.x, t),
            y: self.y.lerp(&to.y, t),
        }
    }
}

impl<T: Lerp + Default + std::fmt::Debug + PartialEq> Lerp for Size<T> {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        Size {
            width: self.width.lerp(&to.width, t),
            height: self.height.lerp(&to.height, t),
        }
    }
}

/// Colors interpolate in sRGB (like CSS by default), which avoids hue sweeps through unrelated colors.
impl Lerp for Rgb {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        self.mix(*to, t)
    }
}

impl Lerp for Hsla {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        let (a, b) = (self.to_rgb(), to.to_rgb());
        let l = |x: f32, y: f32| x + (y - x) * t.clamp(0.0, 1.0);
        let mixed = gpui_kit::Rgba {
            r: l(a.r, b.r),
            g: l(a.g, b.g),
            b: l(a.b, b.b),
            a: l(a.a, b.a),
        };
        mixed.into()
    }
}

impl<A: Lerp, B: Lerp> Lerp for (A, B) {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        (self.0.lerp(&to.0, t), self.1.lerp(&to.1, t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_and_overshoot() {
        assert_eq!(0.0f32.lerp(&10.0, 0.25), 2.5);
        assert_eq!(0.0f32.lerp(&10.0, 1.2), 12.0);
        assert_eq!(px(10.0).lerp(&px(20.0), 0.5), px(15.0));
        let p = Point {
            x: px(0.0),
            y: px(10.0),
        }
        .lerp(
            &Point {
                x: px(10.0),
                y: px(0.0),
            },
            0.5,
        );
        assert_eq!(
            p,
            Point {
                x: px(5.0),
                y: px(5.0)
            }
        );
    }

    #[test]
    fn colors_clamp_and_hit_endpoints() {
        let (a, b) = (Rgb::hex(0x000000), Rgb::hex(0xffffff));
        assert_eq!(a.lerp(&b, 0.0), a);
        assert_eq!(a.lerp(&b, 1.0), b);
        assert_eq!(a.lerp(&b, 3.0), b, "colors don't overshoot");
        let (h1, h2) = (Rgb::hex(0xff0000).to_hsla(), Rgb::hex(0x0000ff).to_hsla());
        let mid = h1.lerp(&h2, 0.5).to_rgb();
        assert!(
            (mid.r - 0.5).abs() < 0.01 && mid.g.abs() < 0.01 && (mid.b - 0.5).abs() < 0.01,
            "sRGB mix, no hue sweep through green"
        );
    }
}
