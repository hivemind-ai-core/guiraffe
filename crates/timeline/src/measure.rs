//! Width measurement shared by the timeline views. Views draw against the bounds measured in
//! the previous frame; when those change, prepaint asks for one more frame so geometry
//! (axis width, initial fit) catches up without a visible mis-sized frame persisting.

use std::{cell::Cell, rc::Rc};

use gpui_kit::{Bounds, Pixels, Window};

pub(crate) type BodyCell = Rc<Cell<Bounds<Pixels>>>;

/// Prepaint hook: record `b`, requesting a redraw if the size changed.
pub(crate) fn record(cell: &BodyCell, b: Bounds<Pixels>, window: &mut Window) {
    if cell.get().size != b.size {
        window.refresh();
    }
    cell.set(b);
}

/// Measured body width, if laid out yet.
pub(crate) fn width(cell: &BodyCell) -> Option<f64> {
    let w = f64::from(f32::from(cell.get().size.width));
    (w > 0.0).then_some(w)
}
