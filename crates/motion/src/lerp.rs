//! Interpolation between values.

use gpui_kit::{Hsla, Pixels, Point, Size, px};
use guiraffe_core::Rgb;

/// Linear interpolation. `t` may lie outside `[0, 1]` (springs and back-easings overshoot);
/// colors clamp `t` to `[0, 1]` because channel values cannot overshoot.
pub trait Lerp: Clone {
    fn lerp(&self, to: &Self, t: f32) -> Self;

    /// How far along the new segment `new_from -> new_to` one unit of progress on the old
    /// segment `self -> old_to` moves (projection of the old direction onto the new one).
    /// Used to keep spring velocity continuous when retargeting. Default 0: velocity is dropped.
    fn span_ratio(&self, _old_to: &Self, _new_from: &Self, _new_to: &Self) -> f32 {
        0.0
    }
}

fn ratio1(old: f32, new: f32) -> f32 {
    if new.abs() < 1e-9 || !old.is_finite() || !new.is_finite() {
        0.0
    } else {
        old / new
    }
}

impl Lerp for f32 {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        self + (to - self) * t
    }
    fn span_ratio(&self, old_to: &Self, new_from: &Self, new_to: &Self) -> f32 {
        ratio1(old_to - self, new_to - new_from)
    }
}

impl Lerp for Pixels {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        px(f32::from(*self).lerp(&f32::from(*to), t))
    }
    fn span_ratio(&self, old_to: &Self, new_from: &Self, new_to: &Self) -> f32 {
        f32::from(*self).span_ratio(
            &f32::from(*old_to),
            &f32::from(*new_from),
            &f32::from(*new_to),
        )
    }
}

impl Lerp for Point<Pixels> {
    fn lerp(&self, to: &Self, t: f32) -> Self {
        Point {
            x: self.x.lerp(&to.x, t),
            y: self.y.lerp(&to.y, t),
        }
    }
    fn span_ratio(&self, old_to: &Self, new_from: &Self, new_to: &Self) -> f32 {
        let (ox, oy) = (f32::from(old_to.x - self.x), f32::from(old_to.y - self.y));
        let (nx, ny) = (
            f32::from(new_to.x - new_from.x),
            f32::from(new_to.y - new_from.y),
        );
        ratio1(ox * nx + oy * ny, nx * nx + ny * ny)
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
    fn span_ratio_projects_velocity() {
        assert_eq!(0.0f32.span_ratio(&10.0, &5.0, &25.0), 0.5);
        assert_eq!(
            0.0f32.span_ratio(&10.0, &5.0, &0.0),
            -2.0,
            "reversal flips the sign"
        );
        assert_eq!(
            0.0f32.span_ratio(&10.0, &5.0, &5.0),
            0.0,
            "zero-length new span"
        );
        let o = Point {
            x: px(0.0),
            y: px(0.0),
        };
        let r = o.span_ratio(
            &Point {
                x: px(10.0),
                y: px(0.0),
            },
            &o,
            &Point {
                x: px(0.0),
                y: px(10.0),
            },
        );
        assert_eq!(r, 0.0, "perpendicular retarget carries no velocity");
        assert_eq!(
            Rgb::hex(0).span_ratio(&Rgb::hex(1), &Rgb::hex(0), &Rgb::hex(2)),
            0.0
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
