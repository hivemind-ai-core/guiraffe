//! Breakpoints and responsive values.
//!
//! The breakpoint table and the resolution rules are pure: they take a width
//! and return a decision, so they can be tested without a window.

use std::collections::BTreeMap;

use crate::tokens::Size;

/// The width bands the responsive props switch at.
///
/// The bands are the ones the core size scale names, so a breakpoint and a
/// size token agree on where the edges are. Below the smallest band there is
/// the [`Base`](Breakpoint::Base) band: every width starts there, and a value
/// set for `Base` holds until the next band takes over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Breakpoint {
    /// Below 576 pixels.
    #[default]
    Base,
    /// 576px and up.
    Xs,
    /// 768px and up.
    Sm,
    /// 992px and up.
    Md,
    /// 1200px and up.
    Lg,
    /// 1408px and up.
    Xl,
}

impl Breakpoint {
    /// Every breakpoint, narrowest first.
    pub const ALL: [Breakpoint; 6] = [
        Breakpoint::Base,
        Breakpoint::Xs,
        Breakpoint::Sm,
        Breakpoint::Md,
        Breakpoint::Lg,
        Breakpoint::Xl,
    ];

    /// The narrowest width that belongs to this breakpoint.
    pub fn min_width(self) -> f32 {
        match self {
            Breakpoint::Base => 0.,
            Breakpoint::Xs => Size::Xs.breakpoint(),
            Breakpoint::Sm => Size::Sm.breakpoint(),
            Breakpoint::Md => Size::Md.breakpoint(),
            Breakpoint::Lg => Size::Lg.breakpoint(),
            Breakpoint::Xl => Size::Xl.breakpoint(),
        }
    }

    /// The breakpoint a width falls into: the widest band the width reaches.
    pub fn from_width(width: f32) -> Breakpoint {
        let mut resolved = Breakpoint::Base;
        for breakpoint in Breakpoint::ALL {
            if width >= breakpoint.min_width() {
                resolved = breakpoint;
            }
        }
        resolved
    }

    /// This breakpoint's position on the scale, narrowest first.
    pub fn index(self) -> usize {
        match self {
            Breakpoint::Base => 0,
            Breakpoint::Xs => 1,
            Breakpoint::Sm => 2,
            Breakpoint::Md => 3,
            Breakpoint::Lg => 4,
            Breakpoint::Xl => 5,
        }
    }

    /// Whether this breakpoint is at least as wide as `other`.
    pub fn at_least(self, other: Breakpoint) -> bool {
        self >= other
    }

    /// The breakpoints from this one downwards, widest first.
    pub fn downwards(self) -> impl Iterator<Item = Breakpoint> {
        Breakpoint::ALL
            .into_iter()
            .filter(move |breakpoint| breakpoint.index() <= self.index())
            .rev()
    }
}

/// A value that can differ per breakpoint.
///
/// `Value` is one value everywhere. `Map` carries a base value plus the
/// breakpoints that override it; a width resolves to the override of the
/// widest band at or below it, and to the base value when no band matches.
///
/// The base value belongs to the [`Base`](Breakpoint::Base) band: it holds
/// from zero pixels up to the next override.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Responsive<T> {
    /// One value for every width.
    Value(T),
    /// A base value plus per-breakpoint overrides.
    Map {
        /// The value used when no override applies.
        base: T,
        /// The overrides, keyed by breakpoint.
        overrides: BTreeMap<Breakpoint, T>,
    },
}

impl<T> Responsive<T> {
    /// A responsive value that never changes.
    pub fn value(value: T) -> Self {
        Responsive::Value(value)
    }

    /// A responsive value whose base applies until a breakpoint overrides it.
    pub fn base(value: T) -> Self {
        Responsive::Map {
            base: value,
            overrides: BTreeMap::new(),
        }
    }

    /// Whether this value changes with the width.
    pub fn is_constant(&self) -> bool {
        match self {
            Responsive::Value(_) => true,
            Responsive::Map { overrides, .. } => overrides.is_empty(),
        }
    }
}

impl<T> Responsive<T> {
    /// Sets the value for one breakpoint.
    pub fn at(self, breakpoint: Breakpoint, value: T) -> Self {
        let (base, mut overrides) = match self {
            Responsive::Value(value) => (value, BTreeMap::new()),
            Responsive::Map { base, overrides } => (base, overrides),
        };
        overrides.insert(breakpoint, value);
        Responsive::Map { base, overrides }
    }

    /// Resolves the value for a width.
    pub fn resolve(&self, width: f32) -> T
    where
        T: Clone,
    {
        self.resolve_at(Breakpoint::from_width(width))
    }

    /// Resolves the value for a breakpoint.
    pub fn resolve_at(&self, breakpoint: Breakpoint) -> T
    where
        T: Clone,
    {
        match self {
            Responsive::Value(value) => value.clone(),
            Responsive::Map { base, overrides } => {
                for candidate in breakpoint.downwards() {
                    if let Some(value) = overrides.get(&candidate) {
                        return value.clone();
                    }
                }
                base.clone()
            }
        }
    }

    /// The base value, ignoring every breakpoint.
    pub fn base_value(&self) -> &T {
        match self {
            Responsive::Value(value) => value,
            Responsive::Map { base, .. } => base,
        }
    }
}

impl<T> From<T> for Responsive<T> {
    fn from(value: T) -> Self {
        Responsive::Value(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{Breakpoint, Responsive};
    use crate::tokens::Size;

    #[test]
    fn a_width_below_the_smallest_band_is_in_the_base_band() {
        assert_eq!(Breakpoint::from_width(0.), Breakpoint::Base);
        assert_eq!(Breakpoint::from_width(575.), Breakpoint::Base);
        assert_eq!(Breakpoint::from_width(575.9), Breakpoint::Base);
    }

    #[test]
    fn an_xs_override_only_starts_at_the_xs_band() {
        let value = Responsive::base(12u16).at(Breakpoint::Xs, 6);
        assert_eq!(value.resolve(0.), 12, "the base band keeps the base value");
        assert_eq!(value.resolve(575.), 12, "the base band reaches up to 575");
        assert_eq!(value.resolve(576.), 6, "the xs band takes over at 576");
    }

    #[test]
    fn the_base_band_can_be_set_on_its_own() {
        let value = Responsive::base(12u16)
            .at(Breakpoint::Base, 3)
            .at(Breakpoint::Sm, 9);
        assert_eq!(value.resolve(0.), 3);
        assert_eq!(value.resolve(575.), 3);
        assert_eq!(value.resolve(768.), 9);
    }

    #[test]
    fn widths_map_to_the_widest_band_they_reach() {
        assert_eq!(Breakpoint::from_width(576.), Breakpoint::Xs);
        assert_eq!(Breakpoint::from_width(767.), Breakpoint::Xs);
        assert_eq!(Breakpoint::from_width(768.), Breakpoint::Sm);
        assert_eq!(Breakpoint::from_width(991.), Breakpoint::Sm);
        assert_eq!(Breakpoint::from_width(992.), Breakpoint::Md);
        assert_eq!(Breakpoint::from_width(1199.), Breakpoint::Md);
        assert_eq!(Breakpoint::from_width(1200.), Breakpoint::Lg);
        assert_eq!(Breakpoint::from_width(1407.), Breakpoint::Lg);
        assert_eq!(Breakpoint::from_width(1408.), Breakpoint::Xl);
        assert_eq!(Breakpoint::from_width(4000.), Breakpoint::Xl);
    }

    #[test]
    fn every_breakpoint_reports_its_own_floor() {
        assert_eq!(Breakpoint::Xs.min_width(), 576.);
        assert_eq!(Breakpoint::Sm.min_width(), 768.);
        assert_eq!(Breakpoint::Md.min_width(), 992.);
        assert_eq!(Breakpoint::Lg.min_width(), 1200.);
        assert_eq!(Breakpoint::Xl.min_width(), 1408.);
        for breakpoint in Breakpoint::ALL {
            assert_eq!(Breakpoint::from_width(breakpoint.min_width()), breakpoint);
        }
    }

    #[test]
    fn the_bands_are_the_ones_the_core_scale_names() {
        for (breakpoint, size) in Breakpoint::ALL[1..].iter().zip(Size::ALL) {
            assert_eq!(breakpoint.min_width(), size.breakpoint());
        }
    }

    #[test]
    fn breakpoints_are_ordered_and_indexed() {
        assert!(Breakpoint::Base < Breakpoint::Xs);
        assert!(Breakpoint::Xs < Breakpoint::Sm);
        assert!(Breakpoint::Sm < Breakpoint::Md);
        assert!(Breakpoint::Md < Breakpoint::Lg);
        assert!(Breakpoint::Lg < Breakpoint::Xl);
        for (index, breakpoint) in Breakpoint::ALL.iter().enumerate() {
            assert_eq!(breakpoint.index(), index);
        }
        assert!(Breakpoint::Lg.at_least(Breakpoint::Md));
        assert!(!Breakpoint::Md.at_least(Breakpoint::Lg));
    }

    #[test]
    fn a_constant_value_resolves_to_itself_at_every_width() {
        let value = Responsive::value(6u16);
        assert!(value.is_constant());
        for width in [0., 500., 800., 1500.] {
            assert_eq!(value.resolve(width), 6);
        }
    }

    #[test]
    fn an_override_applies_from_its_breakpoint_upwards() {
        let value = Responsive::base(12u16).at(Breakpoint::Md, 6);
        assert!(!value.is_constant());
        assert_eq!(value.resolve(0.), 12);
        assert_eq!(value.resolve(991.), 12);
        assert_eq!(value.resolve(992.), 6);
        assert_eq!(value.resolve(2000.), 6);
    }

    #[test]
    fn the_widest_matching_override_wins() {
        let value = Responsive::base(1u16)
            .at(Breakpoint::Sm, 2)
            .at(Breakpoint::Lg, 4)
            .at(Breakpoint::Md, 3);
        assert_eq!(value.resolve(0.), 1);
        assert_eq!(value.resolve(767.), 1);
        assert_eq!(value.resolve(800.), 2);
        assert_eq!(value.resolve(1000.), 3);
        assert_eq!(value.resolve(1300.), 4);
    }

    #[test]
    fn a_gap_between_overrides_falls_back_to_the_wider_one() {
        let value = Responsive::base(0u16)
            .at(Breakpoint::Sm, 2)
            .at(Breakpoint::Xl, 5);
        assert_eq!(value.resolve(1000.), 2);
        assert_eq!(value.resolve(1500.), 5);
    }

    #[test]
    fn the_base_value_is_reachable_without_a_width() {
        let value = Responsive::base(7u16).at(Breakpoint::Lg, 9);
        assert_eq!(*value.base_value(), 7);
    }

    #[test]
    fn a_value_can_be_built_from_a_plain_value() {
        let value: Responsive<u16> = 4u16.into();
        assert!(value.is_constant());
        assert_eq!(value.resolve(1000.), 4);
    }
}
