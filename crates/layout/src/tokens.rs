//! Size tokens shared by the layout widgets.

/// A Mantine-style size token.
///
/// `guiraffe-core` owns the canonical token tables. This copy keeps the layout
/// crate buildable while core is still being written; swapping it for
/// `guiraffe_core::Size` later touches only this module and the re-export in
/// `lib.rs`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Size {
    /// 10px of spacing, 2px of radius.
    Xs,
    /// 12px of spacing, 4px of radius.
    Sm,
    /// 16px of spacing, 8px of radius.
    #[default]
    Md,
    /// 20px of spacing, 12px of radius.
    Lg,
    /// 24px of spacing, 16px of radius.
    Xl,
}

impl Size {
    /// Every token, smallest first.
    pub const ALL: [Size; 5] = [Size::Xs, Size::Sm, Size::Md, Size::Lg, Size::Xl];

    /// The spacing this token stands for, in pixels.
    pub fn spacing(self) -> f32 {
        match self {
            Size::Xs => 10.,
            Size::Sm => 12.,
            Size::Md => 16.,
            Size::Lg => 20.,
            Size::Xl => 24.,
        }
    }

    /// The corner radius this token stands for, in pixels.
    pub fn radius(self) -> f32 {
        match self {
            Size::Xs => 2.,
            Size::Sm => 4.,
            Size::Md => 8.,
            Size::Lg => 12.,
            Size::Xl => 16.,
        }
    }

    /// The next larger token, or the token itself at the top of the scale.
    pub fn next(self) -> Size {
        match self {
            Size::Xs => Size::Sm,
            Size::Sm => Size::Md,
            Size::Md => Size::Lg,
            Size::Lg => Size::Xl,
            Size::Xl => Size::Xl,
        }
    }

    /// The next smaller token, or the token itself at the bottom of the scale.
    pub fn previous(self) -> Size {
        match self {
            Size::Xs => Size::Xs,
            Size::Sm => Size::Xs,
            Size::Md => Size::Sm,
            Size::Lg => Size::Md,
            Size::Xl => Size::Lg,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Size;

    #[test]
    fn spacing_grows_with_the_scale() {
        let spacings: Vec<f32> = Size::ALL.iter().map(|size| size.spacing()).collect();
        assert_eq!(spacings, vec![10., 12., 16., 20., 24.]);
        assert!(spacings.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn radius_grows_with_the_scale() {
        let radii: Vec<f32> = Size::ALL.iter().map(|size| size.radius()).collect();
        assert_eq!(radii, vec![2., 4., 8., 12., 16.]);
        assert!(radii.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn the_scale_walks_both_directions_and_stops_at_its_ends() {
        assert_eq!(Size::Xs.previous(), Size::Xs);
        assert_eq!(Size::Xl.next(), Size::Xl);
        assert_eq!(Size::Md.previous(), Size::Sm);
        assert_eq!(Size::Md.next(), Size::Lg);
    }

    #[test]
    fn the_default_token_is_the_middle_of_the_scale() {
        assert_eq!(Size::default(), Size::Md);
    }
}
