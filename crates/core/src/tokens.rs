//! Size scale tokens (Mantine defaults, in px at 1rem = 16px).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub enum Size {
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
    Xl,
}

impl Size {
    pub const ALL: [Size; 5] = [Size::Xs, Size::Sm, Size::Md, Size::Lg, Size::Xl];

    fn pick(self, t: [f32; 5]) -> f32 {
        t[self as usize]
    }
    /// Gap/padding spacing.
    pub fn spacing(self) -> f32 {
        self.pick([10.0, 12.0, 16.0, 20.0, 32.0])
    }
    pub fn radius(self) -> f32 {
        self.pick([2.0, 4.0, 8.0, 16.0, 32.0])
    }
    pub fn font_size(self) -> f32 {
        self.pick([12.0, 14.0, 16.0, 18.0, 20.0])
    }
    pub fn line_height(self) -> f32 {
        self.pick([1.4, 1.45, 1.55, 1.6, 1.65])
    }
    /// Height of inputs/buttons.
    pub fn control_height(self) -> f32 {
        self.pick([30.0, 36.0, 42.0, 50.0, 60.0])
    }
    /// Icon size inside a control of this size.
    pub fn icon_size(self) -> f32 {
        self.pick([12.0, 14.0, 16.0, 20.0, 24.0])
    }
    /// Container max width.
    pub fn container_width(self) -> f32 {
        self.pick([540.0, 720.0, 960.0, 1140.0, 1320.0])
    }
    /// Breakpoint min width.
    pub fn breakpoint(self) -> f32 {
        self.pick([576.0, 768.0, 992.0, 1200.0, 1408.0])
    }

    /// Largest breakpoint whose min width is <= `width`; `None` below `Xs`.
    pub fn breakpoint_for(width: f32) -> Option<Size> {
        Self::ALL
            .into_iter()
            .rev()
            .find(|s| width >= s.breakpoint())
    }
}

/// Box shadow token: (y offset, blur, alpha) layers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowLayer {
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub alpha: f32,
}

pub fn shadow(size: Size) -> [ShadowLayer; 2] {
    let l = |y, blur, spread, alpha| ShadowLayer {
        y,
        blur,
        spread,
        alpha,
    };
    match size {
        Size::Xs => [l(1.0, 3.0, 0.0, 0.05), l(1.0, 2.0, 0.0, 0.1)],
        Size::Sm => [l(1.0, 3.0, 0.0, 0.05), l(10.0, 15.0, -5.0, 0.05)],
        Size::Md => [l(1.0, 3.0, 0.0, 0.05), l(20.0, 25.0, -5.0, 0.05)],
        Size::Lg => [l(1.0, 3.0, 0.0, 0.05), l(28.0, 23.0, -7.0, 0.05)],
        Size::Xl => [l(1.0, 3.0, 0.0, 0.05), l(36.0, 28.0, -7.0, 0.05)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_are_monotonic() {
        for w in Size::ALL.windows(2) {
            let (a, b) = (w[0], w[1]);
            assert!(a < b);
            assert!(a.spacing() < b.spacing());
            assert!(a.radius() < b.radius());
            assert!(a.font_size() < b.font_size());
            assert!(a.control_height() < b.control_height());
            assert!(a.breakpoint() < b.breakpoint());
            assert!(a.container_width() < b.container_width());
        }
    }

    #[test]
    fn breakpoints() {
        assert_eq!(Size::breakpoint_for(0.0), None);
        assert_eq!(Size::breakpoint_for(575.9), None);
        assert_eq!(Size::breakpoint_for(576.0), Some(Size::Xs));
        assert_eq!(Size::breakpoint_for(1000.0), Some(Size::Md));
        assert_eq!(Size::breakpoint_for(10_000.0), Some(Size::Xl));
    }
}
