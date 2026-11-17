//! Width measurement shared by the timeline views. Views draw against the bounds measured in
//! the previous frame; when those change, prepaint notifies only the editor for one more frame so geometry
//! (axis width, initial fit) catches up without a visible mis-sized frame persisting.

use std::{cell::Cell, rc::Rc};

use gpui_kit::{App, Bounds, EntityId, Pixels};

pub(crate) type BodyCell = Rc<Cell<Bounds<Pixels>>>;

/// Prepaint hook: record `b` and invalidate the editor if its size changed.
pub(crate) fn record(cell: &BodyCell, b: Bounds<Pixels>, cx: &mut App, owner: EntityId) {
    let changed = cell.get().size != b.size;
    cell.set(b);
    if changed {
        cx.defer(move |cx| cx.notify(owner));
    }
}

/// Measured body width, if laid out yet.
pub(crate) fn width(cell: &BodyCell) -> Option<f64> {
    let w = f64::from(f32::from(cell.get().size.width));
    (w > 0.0).then_some(w)
}
