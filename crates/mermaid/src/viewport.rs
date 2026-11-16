//! Pan, zoom and fit math for the diagram viewport.
//!
//! All of this is pure: the GPUI element feeds it pointer deltas and its own
//! bounds, and paints the result. Keeping it out of the element means the
//! behaviours that are easy to get subtly wrong — clamping, zoom-about-a-point,
//! and fitting without leaving a scrollbar's worth of slack — are unit tested.

/// Bounds of zoom, as the renderer accepts them.
pub const MIN_ZOOM: f32 = 0.25;
pub const MAX_ZOOM: f32 = 4.0;

/// Zoom applied by one wheel notch or one button press.
pub const ZOOM_STEP: f32 = 1.25;

/// A point in viewport coordinates, in logical pixels from its top-left.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// What the viewport shows: the displayed diagram size, where its top-left
/// sits, and the zoom factor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Zoom in `MIN_ZOOM..=MAX_ZOOM`.
    zoom: f32,
    offset: Point,
    /// Natural (1×) diagram size, in logical pixels.
    natural: (f32, f32),
    /// Available viewport size, in logical pixels.
    bounds: (f32, f32),
    /// Whether the view has been fitted since the last explicit zoom, so a
    /// resized view keeps fitting the diagram until the user takes over.
    auto_fit: bool,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset: Point::default(),
            natural: (0.0, 0.0),
            bounds: (0.0, 0.0),
            auto_fit: true,
        }
    }
}

fn clamp_zoom(zoom: f32) -> f32 {
    if zoom.is_finite() {
        zoom.clamp(MIN_ZOOM, MAX_ZOOM)
    } else {
        1.0
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

impl Viewport {
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn offset(&self) -> Point {
        self.offset
    }

    pub fn natural(&self) -> (f32, f32) {
        self.natural
    }

    pub fn bounds(&self) -> (f32, f32) {
        self.bounds
    }

    /// Whether the diagram is currently shown fit-to-view.
    pub fn auto_fit(&self) -> bool {
        self.auto_fit
    }

    /// Displayed diagram size, in logical pixels.
    pub fn displayed(&self) -> (f32, f32) {
        (self.natural.0 * self.zoom, self.natural.1 * self.zoom)
    }

    /// Set the natural diagram size, keeping the current fit policy.
    ///
    /// A new diagram while auto-fitting refits; otherwise the user's zoom is
    /// kept, because an agent chat re-renders a diagram as it streams and
    /// snapping back to fit on every token would fight the reader.
    pub fn set_natural(&mut self, width: f32, height: f32) {
        let size = (
            finite_or(width, 0.0).max(0.0),
            finite_or(height, 0.0).max(0.0),
        );
        if size == self.natural {
            return;
        }
        self.natural = size;
        if self.auto_fit {
            self.fit();
        } else {
            self.offset = self.clamp_offset(self.offset);
        }
    }

    /// Set the viewport size in logical pixels.
    pub fn set_bounds(&mut self, width: f32, height: f32) {
        let size = (
            finite_or(width, 0.0).max(0.0),
            finite_or(height, 0.0).max(0.0),
        );
        if size == self.bounds {
            return;
        }
        self.bounds = size;
        if self.auto_fit {
            self.fit();
        } else {
            self.offset = self.clamp_offset(self.offset);
        }
    }

    /// Zoom that shows the whole diagram, never above 1×.
    ///
    /// A tiny diagram is not blown up: enlarging it past its natural size
    /// makes text look soft, and the reader asked to see it, not to fill space.
    pub fn fit_zoom(&self) -> f32 {
        let (nw, nh) = self.natural;
        let (bw, bh) = self.bounds;
        if nw <= 0.0 || nh <= 0.0 || bw <= 0.0 || bh <= 0.0 {
            return 1.0;
        }
        // A small margin so the diagram's border is not flush with the edge.
        let pad = 1.0;
        clamp_zoom((bw * pad / nw).min(bh * pad / nh).min(1.0))
    }

    /// Fit the whole diagram in the viewport, centered.
    pub fn fit(&mut self) {
        self.auto_fit = true;
        self.zoom = self.fit_zoom();
        self.offset = self.centering_offset(self.zoom);
    }

    /// Center the diagram at its current zoom, leaving auto-fit.
    pub fn reset(&mut self) {
        self.auto_fit = false;
        self.zoom = 1.0;
        self.offset = self.centering_offset(self.zoom);
    }

    /// Multiply the zoom, keeping the point under `anchor` in place.
    ///
    /// `anchor` is in viewport coordinates. Zooming about the pointer is what
    /// makes wheel zoom feel like a magnifier rather than a jump.
    pub fn zoom_by(&mut self, factor: f32, anchor: Point) {
        self.zoom_at(clamp_zoom(self.zoom * factor), anchor);
    }

    /// Set the zoom exactly, keeping the point under `anchor` in place.
    pub fn zoom_at(&mut self, zoom: f32, anchor: Point) {
        let zoom = clamp_zoom(zoom);
        if zoom == self.zoom {
            return;
        }
        // The diagram point under the anchor before and after must match:
        //   (anchor - offset) / zoom  ==  (anchor - offset') / zoom'
        let dx = anchor.x - self.offset.x;
        let dy = anchor.y - self.offset.y;
        let ratio = zoom / self.zoom;
        let offset = Point::new(anchor.x - dx * ratio, anchor.y - dy * ratio);
        self.zoom = zoom;
        self.auto_fit = false;
        self.offset = self.clamp_offset(offset);
    }

    /// Pan by a pointer delta, in logical pixels.
    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        let dx = finite_or(dx, 0.0);
        let dy = finite_or(dy, 0.0);
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        self.auto_fit = false;
        let moved = Point::new(self.offset.x + dx, self.offset.y + dy);
        self.offset = self.clamp_offset(moved);
    }

    /// Whether the diagram overflows the viewport at the current zoom, so
    /// panning has anywhere to go.
    pub fn is_pannable(&self) -> bool {
        let (dw, dh) = self.displayed();
        let (bw, bh) = self.bounds;
        dw > bw + 0.5 || dh > bh + 0.5
    }

    /// Zoom by one step about the viewport center.
    pub fn zoom_in(&mut self) {
        let center = Point::new(self.bounds.0 / 2.0, self.bounds.1 / 2.0);
        self.zoom_by(ZOOM_STEP, center);
    }

    /// Zoom out by one step about the viewport center.
    pub fn zoom_out(&mut self) {
        let center = Point::new(self.bounds.0 / 2.0, self.bounds.1 / 2.0);
        self.zoom_by(1.0 / ZOOM_STEP, center);
    }

    /// Offset that centers the diagram at `zoom`, clamped into the pannable
    /// range so an overflowing diagram is centered rather than pinned to a
    /// corner it cannot reach.
    fn centering_offset(&self, zoom: f32) -> Point {
        let (nw, nh) = self.natural;
        let (bw, bh) = self.bounds;
        self.clamp_offset(Point::new((bw - nw * zoom) / 2.0, (bh - nh * zoom) / 2.0))
    }

    /// Clamp an offset so the diagram cannot be dragged out of sight.
    ///
    /// An axis smaller than the viewport is centered; a larger one may be
    /// panned until its edge reaches the viewport edge.
    fn clamp_offset(&self, offset: Point) -> Point {
        let (dw, dh) = self.displayed();
        let (bw, bh) = self.bounds;
        let axis = |position: f32, displayed: f32, viewport: f32| {
            if viewport <= 0.0 {
                return 0.0;
            }
            if displayed <= viewport {
                (viewport - displayed) / 2.0
            } else {
                // Left/top gap in 0..=(viewport - displayed) keeps both edges
                // reachable: 0 shows the top-left, the minimum shows the
                // bottom-right.
                position.clamp(viewport - displayed, 0.0)
            }
        };
        Point::new(axis(offset.x, dw, bw), axis(offset.y, dh, bh))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport(natural: (f32, f32), bounds: (f32, f32)) -> Viewport {
        let mut v = Viewport::default();
        v.set_natural(natural.0, natural.1);
        v.set_bounds(bounds.0, bounds.1);
        v
    }

    #[test]
    fn fitting_a_large_diagram_scales_it_down_and_centers_it() {
        let v = viewport((2000.0, 1000.0), (800.0, 600.0));
        assert!(v.auto_fit());
        assert!((v.zoom() - 0.4).abs() < 1e-5, "{}", v.zoom());
        let (dw, dh) = v.displayed();
        assert!((dw - 800.0).abs() < 1e-3);
        assert!((dh - 400.0).abs() < 1e-3);
        assert_eq!(v.offset(), Point::new(0.0, 100.0));
    }

    #[test]
    fn fitting_a_small_diagram_does_not_enlarge_it() {
        let v = viewport((200.0, 100.0), (800.0, 600.0));
        assert_eq!(v.zoom(), 1.0);
        assert_eq!(v.offset(), Point::new(300.0, 250.0));
    }

    #[test]
    fn no_fit_before_both_sizes_are_known() {
        let mut v = Viewport::default();
        assert_eq!(v.zoom(), 1.0);
        v.set_natural(100.0, 100.0);
        assert_eq!(v.zoom(), 1.0, "no bounds yet");
        v.set_bounds(0.0, 0.0);
        assert_eq!(v.zoom(), 1.0);
    }

    #[test]
    fn wheel_zoom_keeps_the_point_under_the_pointer() {
        let mut v = viewport((1000.0, 1000.0), (500.0, 500.0));
        v.fit();
        let anchor = Point::new(100.0, 400.0);
        let before = diagram_point(&v, anchor);
        v.zoom_by(ZOOM_STEP, anchor);
        let after = diagram_point(&v, anchor);
        assert!((before.0 - after.0).abs() < 1e-3, "{before:?} {after:?}");
        assert!((before.1 - after.1).abs() < 1e-3);
        assert!(!v.auto_fit());
    }

    fn diagram_point(v: &Viewport, p: Point) -> (f32, f32) {
        (
            (p.x - v.offset().x) / v.zoom(),
            (p.y - v.offset().y) / v.zoom(),
        )
    }

    #[test]
    fn zoom_is_clamped_to_the_supported_range() {
        let mut v = viewport((100.0, 100.0), (500.0, 500.0));
        v.zoom_at(100.0, Point::new(0.0, 0.0));
        assert_eq!(v.zoom(), MAX_ZOOM);
        v.zoom_at(0.001, Point::new(250.0, 250.0));
        assert_eq!(v.zoom(), MIN_ZOOM);
        v.zoom_at(f32::NAN, Point::new(0.0, 0.0));
        assert_eq!(v.zoom(), 1.0);
    }

    #[test]
    fn panning_a_small_diagram_does_nothing() {
        let mut v = viewport((100.0, 100.0), (500.0, 500.0));
        v.fit();
        let before = v.offset();
        v.pan_by(50.0, 50.0);
        assert_eq!(v.offset(), before);
        assert!(!v.is_pannable());
    }

    #[test]
    fn panning_a_large_diagram_stops_at_the_edges() {
        let mut v = viewport((2000.0, 2000.0), (500.0, 500.0));
        v.zoom_at(1.0, Point::new(0.0, 0.0));
        assert!(v.is_pannable());
        v.pan_by(10_000.0, 10_000.0);
        assert_eq!(v.offset(), Point::new(0.0, 0.0), "cannot pan past top-left");
        v.pan_by(-100_000.0, -100_000.0);
        assert_eq!(
            v.offset(),
            Point::new(-1500.0, -1500.0),
            "stops when the bottom-right edge reaches the viewport"
        );
    }

    #[test]
    fn resizing_refits_while_auto_fitting_and_keeps_zoom_otherwise() {
        let mut v = viewport((1000.0, 500.0), (800.0, 600.0));
        assert!((v.zoom() - 0.8).abs() < 1e-5);
        v.set_bounds(500.0, 600.0);
        assert!((v.zoom() - 0.5).abs() < 1e-5, "{}", v.zoom());

        v.zoom_at(1.0, Point::new(10.0, 10.0));
        v.set_bounds(1600.0, 1200.0);
        assert_eq!(v.zoom(), 1.0, "an explicit zoom survives a resize");
    }

    #[test]
    fn streaming_a_new_size_keeps_the_reader_zoom() {
        let mut v = viewport((1000.0, 500.0), (800.0, 600.0));
        v.zoom_at(2.0, Point::new(0.0, 0.0));
        v.set_natural(1400.0, 700.0);
        assert_eq!(v.zoom(), 2.0);
    }

    #[test]
    fn reset_returns_to_actual_size_centered() {
        let mut v = viewport((2000.0, 1000.0), (800.0, 600.0));
        v.zoom_at(3.0, Point::new(10.0, 10.0));
        v.pan_by(-100.0, -100.0);
        v.reset();
        assert_eq!(v.zoom(), 1.0);
        assert!(!v.auto_fit());
        assert_eq!(v.offset(), Point::new(-600.0, -200.0));
    }

    #[test]
    fn degenerate_sizes_are_ignored_rather_than_panicking() {
        let mut v = Viewport::default();
        v.set_natural(f32::NAN, f32::INFINITY);
        assert_eq!(v.natural(), (0.0, 0.0));
        v.set_bounds(-10.0, 10.0);
        assert_eq!(v.bounds(), (0.0, 10.0));
        assert_eq!(v.zoom(), 1.0);
        v.pan_by(f32::NAN, 5.0);
        assert_eq!(v.offset(), Point::new(0.0, 5.0), "NaN delta is dropped");
    }

    #[test]
    fn fitting_never_goes_below_the_minimum_zoom() {
        // A diagram a hundred times the viewport cannot be fit at 0.01x.
        let v = viewport((50_000.0, 50_000.0), (500.0, 500.0));
        assert_eq!(v.zoom(), MIN_ZOOM);
        assert!(
            v.is_pannable(),
            "the fitted diagram is still larger than the view"
        );
    }

    #[test]
    fn zoom_by_equal_factor_is_a_no_op() {
        let mut v = viewport((1000.0, 1000.0), (500.0, 500.0));
        let before = v.zoom();
        v.zoom_by(1.0, Point::new(250.0, 250.0));
        assert_eq!(v.zoom(), before);
    }
}
