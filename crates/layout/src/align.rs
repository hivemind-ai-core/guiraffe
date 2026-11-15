//! Cross-axis and main-axis alignment.

/// Where children sit across the cross axis of a stack, group or flex row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Align {
    /// Against the start of the cross axis.
    #[default]
    Start,
    /// Centered on the cross axis.
    Center,
    /// Against the end of the cross axis.
    End,
    /// Stretched to fill the cross axis.
    Stretch,
    /// Aligned on their baselines.
    Baseline,
}

/// Where children sit along the main axis of a stack, group or flex row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Justify {
    /// Against the start of the main axis.
    #[default]
    Start,
    /// Centered on the main axis.
    Center,
    /// Against the end of the main axis.
    End,
    /// First child at the start, last child at the end, the rest between.
    Between,
    /// Space distributed around every child.
    Around,
    /// Space distributed evenly, including at both ends.
    Evenly,
}

#[cfg(test)]
mod tests {
    use super::{Align, Justify};

    #[test]
    fn defaults_put_children_at_the_start() {
        assert_eq!(Align::default(), Align::Start);
        assert_eq!(Justify::default(), Justify::Start);
    }

    #[test]
    fn the_variants_are_distinct() {
        let aligns = [
            Align::Start,
            Align::Center,
            Align::End,
            Align::Stretch,
            Align::Baseline,
        ];
        assert_eq!(aligns.len(), 5);
        for (index, align) in aligns.iter().enumerate() {
            for other in aligns.iter().skip(index + 1) {
                assert_ne!(align, other);
            }
        }

        let justifies = [
            Justify::Start,
            Justify::Center,
            Justify::End,
            Justify::Between,
            Justify::Around,
            Justify::Evenly,
        ];
        assert_eq!(justifies.len(), 6);
        for (index, justify) in justifies.iter().enumerate() {
            for other in justifies.iter().skip(index + 1) {
                assert_ne!(justify, other);
            }
        }
    }
}
