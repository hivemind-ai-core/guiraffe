//! Mantine-named transition presets, resolved to a pure style frame.
//!
//! GPUI has no general element transforms, so presets are expressed as opacity plus a
//! positional offset. Mantine's `scale`/`pop`/`rotate` presets map to fade with a small rise.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TransitionKind {
    #[default]
    Fade,
    FadeUp,
    FadeDown,
    FadeLeft,
    FadeRight,
    SlideUp,
    SlideDown,
    SlideLeft,
    SlideRight,
    /// Approximated as fade + 4px rise (no element scale in GPUI).
    Pop,
    /// Approximated like `Pop`.
    Scale,
    /// No visual change; for presence bookkeeping only.
    None,
}

/// Style to apply at a point in a transition. `offset` is in pixels, relative to resting place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub opacity: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

impl Frame {
    pub const REST: Frame = Frame {
        opacity: 1.0,
        offset_x: 0.0,
        offset_y: 0.0,
    };
}

impl TransitionKind {
    /// Frame at `visibility` ∈ [0,1] (0 = fully hidden, 1 = at rest). `extent` is the travel
    /// distance for slide presets (usually the element's size); fades travel a fixed 8px.
    pub fn frame(self, visibility: f32, extent: f32) -> Frame {
        let v = if visibility.is_finite() {
            visibility
        } else {
            1.0
        };
        // Opacity never overshoots even when a spring does.
        let opacity = v.clamp(0.0, 1.0);
        let hidden = 1.0 - v;
        let extent = if extent.is_finite() {
            extent.abs()
        } else {
            0.0
        };
        let (dx, dy, fade) = match self {
            Self::Fade => (0.0, 0.0, true),
            Self::FadeUp => (0.0, 8.0, true),
            Self::FadeDown => (0.0, -8.0, true),
            Self::FadeLeft => (8.0, 0.0, true),
            Self::FadeRight => (-8.0, 0.0, true),
            Self::SlideUp => (0.0, extent, false),
            Self::SlideDown => (0.0, -extent, false),
            Self::SlideLeft => (extent, 0.0, false),
            Self::SlideRight => (-extent, 0.0, false),
            Self::Pop | Self::Scale => (0.0, 4.0, true),
            Self::None => return Frame::REST,
        };
        Frame {
            opacity: if fade { opacity } else { 1.0 },
            offset_x: dx * hidden,
            offset_y: dy * hidden,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [TransitionKind; 12] = [
        TransitionKind::Fade,
        TransitionKind::FadeUp,
        TransitionKind::FadeDown,
        TransitionKind::FadeLeft,
        TransitionKind::FadeRight,
        TransitionKind::SlideUp,
        TransitionKind::SlideDown,
        TransitionKind::SlideLeft,
        TransitionKind::SlideRight,
        TransitionKind::Pop,
        TransitionKind::Scale,
        TransitionKind::None,
    ];

    #[test]
    fn every_preset_rests_at_one() {
        for k in ALL {
            assert_eq!(k.frame(1.0, 300.0), Frame::REST, "{k:?}");
        }
    }

    #[test]
    fn hidden_frames() {
        assert_eq!(TransitionKind::Fade.frame(0.0, 0.0).opacity, 0.0);
        let s = TransitionKind::SlideUp.frame(0.0, 300.0);
        assert_eq!((s.opacity, s.offset_y), (1.0, 300.0));
        assert_eq!(TransitionKind::SlideLeft.frame(0.5, 100.0).offset_x, 50.0);
        assert_eq!(TransitionKind::None.frame(0.0, 10.0), Frame::REST);
    }

    #[test]
    fn overshoot_and_bad_input_are_safe() {
        let f = TransitionKind::FadeUp.frame(1.1, 0.0);
        assert_eq!(f.opacity, 1.0);
        assert!(
            f.offset_y < 0.0,
            "spring overshoot travels slightly past rest"
        );
        for k in ALL {
            let f = k.frame(f32::NAN, f32::INFINITY);
            assert!(f.opacity.is_finite() && f.offset_x.is_finite() && f.offset_y.is_finite());
        }
    }
}
