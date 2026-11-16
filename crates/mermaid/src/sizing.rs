//! Pure raster sizing: how many device pixels a diagram is drawn into.
//!
//! Memory is the scarce resource here: a badly-behaved diagram can ask for a
//! raster larger than the screen, and zoom multiplies that. The policy is
//! "bounded area, preserved aspect ratio, never below 1×1", with the display
//! size allowed to exceed the raster size (the image is scaled up by GPUI).

/// Largest raster width or height, in device pixels.
pub const MAX_RASTER_EDGE: f32 = 4096.0;
/// Largest raster area, in device pixels.
pub const MAX_RASTER_PIXELS: f32 = 4_000_000.0;

/// Device-pixel raster size for a logical `width` × `height` drawn at `density`.
///
/// Caps the edge at [`MAX_RASTER_EDGE`] and the area at [`MAX_RASTER_PIXELS`],
/// whichever binds first, and preserves the aspect ratio. Non-finite,
/// zero or negative sizes fall back to 1×1 rather than panicking or asking
/// the rasterizer for an empty image.
pub fn raster_size(width: f32, height: f32, density: f32) -> (u32, u32) {
    let density = if density.is_finite() {
        density.max(1.0)
    } else {
        1.0
    };
    let width = if width.is_finite() { width } else { 1.0 };
    let height = if height.is_finite() { height } else { 1.0 };
    let w = (width * density).max(1.0);
    let h = (height * density).max(1.0);
    let scale = (MAX_RASTER_EDGE / w)
        .min(MAX_RASTER_EDGE / h)
        .min((MAX_RASTER_PIXELS / (w * h)).sqrt())
        .min(1.0);
    (
        (w * scale).floor().max(1.0) as u32,
        (h * scale).floor().max(1.0) as u32,
    )
}

/// Whether natural diagram dimensions are usable for layout.
pub fn dimensions_are_usable(width: f32, height: f32) -> bool {
    width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_diagrams_are_drawn_at_density() {
        assert_eq!(raster_size(900.0, 700.0, 1.0), (900, 700));
        assert_eq!(raster_size(900.0, 700.0, 2.0), (1800, 1400));
    }

    #[test]
    fn huge_diagrams_are_capped_with_aspect_preserved() {
        for (w, h) in [
            (50_000.0, 20_000.0),
            (100.0, 80_000.0),
            (20_000.0, 20_000.0),
        ] {
            let (rw, rh) = raster_size(w, h, 2.0);
            assert!(rw <= MAX_RASTER_EDGE as u32 && rh <= MAX_RASTER_EDGE as u32);
            assert!(u64::from(rw) * u64::from(rh) <= MAX_RASTER_PIXELS as u64);
            let ratio_error = (rw as f32 / rh as f32) / (w / h) - 1.0;
            assert!(ratio_error.abs() < 0.05, "{rw}x{rh} for {w}x{h}");
        }
    }

    #[test]
    fn degenerate_inputs_still_produce_one_pixel() {
        assert_eq!(raster_size(0.0, 0.0, 1.0), (1, 1));
        assert_eq!(raster_size(f32::NAN, 10.0, 1.0), (1, 10));
        assert_eq!(raster_size(-5.0, -5.0, 2.0), (1, 1));
        assert_eq!(raster_size(10.0, 10.0, f32::INFINITY), (10, 10));
        assert_eq!(
            raster_size(10.0, 10.0, 0.0),
            (10, 10),
            "density below 1 clamps"
        );
    }

    #[test]
    fn usable_dimensions_reject_non_positive_and_non_finite() {
        assert!(dimensions_are_usable(10.0, 10.0));
        assert!(!dimensions_are_usable(0.0, 10.0));
        assert!(!dimensions_are_usable(10.0, -1.0));
        assert!(!dimensions_are_usable(f32::NAN, 1.0));
        assert!(!dimensions_are_usable(1.0, f32::INFINITY));
    }
}
