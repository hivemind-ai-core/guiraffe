//! Editing policy shared by the Gantt and track views: which edges resize, what a press at a
//! pixel grabs, and the cursor that advertises it. Pure; unit-tested.

use gpui_kit::CursorStyle;

use crate::clips::Edge;

/// Which item edges can be dragged to resize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResizeEdges {
    /// Items can only be moved.
    None,
    /// Only the start (left) edge resizes.
    Start,
    /// Only the end (right) edge resizes.
    End,
    /// Both edges resize.
    #[default]
    Both,
}

impl ResizeEdges {
    pub fn allows(self, edge: Edge) -> bool {
        matches!(
            (self, edge),
            (ResizeEdges::Both, _)
                | (ResizeEdges::Start, Edge::Start)
                | (ResizeEdges::End, Edge::End)
        )
    }
}

/// What a press grabs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Grab {
    /// Move the item; `grab_dt` is the press time minus the item start.
    Move { grab_dt: f64 },
    /// Resize from an edge.
    Trim(Edge),
}

/// Pixels from an edge that grab a resize instead of a move.
pub const EDGE_GRAB: f64 = 6.0;

/// Decide what a press at pixel `x` (time `t`) grabs on an item spanning pixels `[x0, x1]`
/// that starts at time `start`. Edge zones never take more than a third of the item, so short
/// items stay movable; disallowed edges fall through to a move.
pub fn grab_at(x0: f64, x1: f64, x: f64, t: f64, start: f64, edges: ResizeEdges) -> Option<Grab> {
    if !(x >= x0 - EDGE_GRAB.min((x1 - x0) / 3.0) && x <= x1 + EDGE_GRAB.min((x1 - x0) / 3.0)) {
        return None;
    }
    let zone = EDGE_GRAB.min((x1 - x0) / 3.0);
    if (x - x0).abs() <= zone && edges.allows(Edge::Start) {
        Some(Grab::Trim(Edge::Start))
    } else if (x - x1).abs() <= zone && edges.allows(Edge::End) {
        Some(Grab::Trim(Edge::End))
    } else if x >= x0 && x <= x1 {
        Some(Grab::Move { grab_dt: t - start })
    } else {
        None
    }
}

/// Cursor for a hovered grab.
pub fn cursor_for(grab: Option<Grab>) -> CursorStyle {
    match grab {
        Some(Grab::Trim(_)) => CursorStyle::ResizeLeftRight,
        Some(Grab::Move { .. }) => CursorStyle::OpenHand,
        None => CursorStyle::Arrow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_follow_policy() {
        let g = |x, e| grab_at(100.0, 200.0, x, 0.0, 0.0, e);
        assert_eq!(g(101.0, ResizeEdges::Both), Some(Grab::Trim(Edge::Start)));
        assert_eq!(g(199.0, ResizeEdges::Both), Some(Grab::Trim(Edge::End)));
        assert_eq!(
            g(101.0, ResizeEdges::End),
            Some(Grab::Move { grab_dt: 0.0 })
        );
        assert_eq!(
            g(199.0, ResizeEdges::Start),
            Some(Grab::Move { grab_dt: 0.0 })
        );
        assert_eq!(
            g(97.0, ResizeEdges::Both),
            Some(Grab::Trim(Edge::Start)),
            "just outside the edge still grabs it"
        );
        assert_eq!(
            g(97.0, ResizeEdges::None),
            None,
            "outside the bar with no resize"
        );
        assert_eq!(
            g(150.0, ResizeEdges::None),
            Some(Grab::Move { grab_dt: 0.0 })
        );
        assert_eq!(g(300.0, ResizeEdges::Both), None);
    }

    #[test]
    fn short_items_stay_movable() {
        assert!(matches!(
            grab_at(100.0, 105.0, 102.5, 0.0, 0.0, ResizeEdges::Both),
            Some(Grab::Move { .. })
        ));
    }

    #[test]
    fn cursors() {
        assert_eq!(
            cursor_for(Some(Grab::Trim(Edge::End))),
            CursorStyle::ResizeLeftRight
        );
        assert_eq!(cursor_for(None), CursorStyle::Arrow);
    }
}
