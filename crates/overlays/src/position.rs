//! GPUI-independent floating placement. Coordinates are logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}
impl Side {
    pub fn opposite(self) -> Self {
        match self {
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Top,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatingOptions {
    pub side: Side,
    pub align: Align,
    pub gap: f32,
    pub padding: f32,
    pub arrow_padding: f32,
    pub flip: bool,
    pub shift: bool,
}
impl Default for FloatingOptions {
    fn default() -> Self {
        Self {
            side: Side::Bottom,
            align: Align::Center,
            gap: 4.,
            padding: 8.,
            arrow_padding: 8.,
            flip: true,
            shift: true,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatingPosition {
    pub x: f32,
    pub y: f32,
    pub side: Side,
    /// Arrow coordinate relative to floating surface, on the cross axis.
    pub arrow: f32,
}
/// Reject non-finite coordinates and negative dimensions rather than emitting invalid layout.
pub fn position(
    anchor: Rect,
    floating: (f32, f32),
    viewport: Rect,
    options: FloatingOptions,
) -> Option<FloatingPosition> {
    let values = [
        anchor.x,
        anchor.y,
        anchor.width,
        anchor.height,
        floating.0,
        floating.1,
        viewport.x,
        viewport.y,
        viewport.width,
        viewport.height,
        options.gap,
        options.padding,
        options.arrow_padding,
    ];
    if values.iter().any(|v| !v.is_finite())
        || [
            anchor.width,
            anchor.height,
            floating.0,
            floating.1,
            viewport.width,
            viewport.height,
            options.gap,
            options.padding,
            options.arrow_padding,
        ]
        .iter()
        .any(|v| *v < 0.)
    {
        return None;
    }
    let (w, h) = floating;
    let candidate = |side| {
        let cross = |start: f32, extent: f32, size: f32| match options.align {
            Align::Start => start,
            Align::Center => start + (extent - size) / 2.,
            Align::End => start + extent - size,
        };
        match side {
            Side::Top => (cross(anchor.x, anchor.width, w), anchor.y - options.gap - h),
            Side::Bottom => (
                cross(anchor.x, anchor.width, w),
                anchor.y + anchor.height + options.gap,
            ),
            Side::Left => (
                anchor.x - options.gap - w,
                cross(anchor.y, anchor.height, h),
            ),
            Side::Right => (
                anchor.x + anchor.width + options.gap,
                cross(anchor.y, anchor.height, h),
            ),
        }
    };
    let min_x = viewport.x + options.padding;
    let max_x = (viewport.x + viewport.width - options.padding - w).max(min_x);
    let min_y = viewport.y + options.padding;
    let max_y = (viewport.y + viewport.height - options.padding - h).max(min_y);
    let overflow = |(x, y): (f32, f32)| {
        (min_x - x).max(0.)
            + (x + w - (viewport.x + viewport.width - options.padding)).max(0.)
            + (min_y - y).max(0.)
            + (y + h - (viewport.y + viewport.height - options.padding)).max(0.)
    };
    let mut side = options.side;
    let mut p = candidate(side);
    if options.flip {
        let opposite = candidate(side.opposite());
        if overflow(opposite) < overflow(p) {
            side = side.opposite();
            p = opposite;
        }
    }
    if options.shift {
        p.0 = p.0.clamp(min_x, max_x);
        p.1 = p.1.clamp(min_y, max_y);
    }
    let (center, origin, extent) = match side {
        Side::Top | Side::Bottom => (anchor.x + anchor.width / 2., p.0, w),
        _ => (anchor.y + anchor.height / 2., p.1, h),
    };
    let pad = options.arrow_padding.min(extent / 2.);
    if ![p.0, p.1, center, origin, extent]
        .iter()
        .all(|v| v.is_finite())
    {
        return None;
    }
    Some(FloatingPosition {
        x: p.0,
        y: p.1,
        side,
        arrow: (center - origin).clamp(pad, (extent - pad).max(pad)),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }
    #[test]
    fn all_sides_alignments_and_edges() {
        for side in [Side::Top, Side::Right, Side::Bottom, Side::Left] {
            for align in [Align::Start, Align::Center, Align::End] {
                for x in [0., 40., 90.] {
                    for y in [0., 40., 90.] {
                        let p = position(
                            rect(x, y, 10., 10.),
                            (30., 20.),
                            rect(0., 0., 100., 100.),
                            FloatingOptions {
                                side,
                                align,
                                ..Default::default()
                            },
                        )
                        .expect("position");
                        assert!(p.x >= 8. && p.x <= 62.);
                        assert!(p.y >= 8. && p.y <= 72.);
                        let extent = if matches!(p.side, Side::Top | Side::Bottom) {
                            30.
                        } else {
                            20.
                        };
                        assert!(p.arrow >= 8. && p.arrow <= extent - 8.);
                    }
                }
            }
        }
    }
    #[test]
    fn flips_only_when_better() {
        let p = position(
            rect(40., 90., 10., 10.),
            (20., 30.),
            rect(0., 0., 100., 100.),
            Default::default(),
        )
        .expect("position");
        assert_eq!(p.side, Side::Top);
        assert_eq!(p.y, 56.);
    }
    #[test]
    fn exact_alignment_without_shift_or_flip() {
        for (align, x) in [(Align::Start, 40.), (Align::Center, 30.), (Align::End, 20.)] {
            let p = position(
                rect(40., 40., 20., 10.),
                (40., 30.),
                rect(0., 0., 100., 100.),
                FloatingOptions {
                    align,
                    flip: false,
                    shift: false,
                    ..Default::default()
                },
            )
            .expect("position");
            assert_eq!((p.x, p.y), (x, 54.));
        }
    }
    #[test]
    fn oversized_and_tiny_viewport() {
        let p = position(
            rect(-10., -10., 0., 0.),
            (200., 200.),
            rect(20., 30., 1., 1.),
            Default::default(),
        )
        .expect("position");
        assert_eq!((p.x, p.y), (28., 38.));
        assert!(p.arrow.is_finite());
    }
    #[test]
    fn invalid_inputs_rejected() {
        for bad in [f32::NAN, f32::INFINITY, -1.] {
            assert!(
                position(
                    rect(0., 0., bad, 1.),
                    (1., 1.),
                    rect(0., 0., 10., 10.),
                    Default::default()
                )
                .is_none()
            );
        }
    }
    #[test]
    fn zero_arrow_and_offset_viewport() {
        let p = position(
            rect(100., 200., 0., 0.),
            (0., 0.),
            rect(100., 200., 20., 20.),
            Default::default(),
        )
        .expect("position");
        assert_eq!(p.arrow, 0.);
        assert!(p.x >= 108.);
    }
}
