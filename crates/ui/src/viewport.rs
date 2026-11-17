//! Pure sizing policy for uniform repeats. Natural-height rows bypass this policy.

/// Optional maximum height for uniform repeats within one host.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RepeatViewport {
    max_height: Option<f32>,
}

/// A repeat height must be finite and nonnegative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepeatViewportError;
impl std::fmt::Display for RepeatViewportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("repeat maximum height must be finite and nonnegative")
    }
}
impl std::error::Error for RepeatViewportError {}

impl RepeatViewport {
    pub fn new(max_height: Option<f32>) -> Result<Self, RepeatViewportError> {
        if max_height.is_some_and(|height| !height.is_finite() || height < 0.) {
            return Err(RepeatViewportError);
        }
        Ok(Self { max_height })
    }
    pub fn max_height(self) -> Option<f32> {
        self.max_height
    }
    pub(crate) fn resolve(self, row_height: f32, count: usize, window_height: f32) -> (f32, bool) {
        let valid = |height: f32| {
            if height.is_finite() && height > 0. {
                height
            } else {
                0.
            }
        };
        let cap = valid(window_height).min(self.max_height.unwrap_or(f32::MAX));
        let content = f64::from(valid(row_height)) * count as f64;
        (content.min(f64::from(cap)) as f32, content > f64::from(cap))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_fills_available_height_without_a_row_count_cap() {
        let viewport = RepeatViewport::default();
        for count in 0..100 {
            let (height, scroll) = viewport.resolve(44., count, 600.);
            assert_eq!(height, (44. * count as f32).min(600.));
            assert_eq!(scroll, count > 13);
        }
    }
    #[test]
    fn configured_height_is_bounded_by_window_and_handles_large_counts() {
        let viewport = RepeatViewport::new(Some(240.)).expect("valid");
        assert_eq!(viewport.max_height(), Some(240.));
        assert_eq!(viewport.resolve(44., 20, 600.), (240., true));
        assert_eq!(viewport.resolve(44., 20, 100.), (100., true));
        assert_eq!(viewport.resolve(f32::MAX, usize::MAX, 600.), (240., true));
        assert_eq!(
            RepeatViewport::new(Some(0.))
                .expect("zero")
                .resolve(44., 1, 600.),
            (0., true)
        );
    }
    #[test]
    fn rejects_invalid_configuration_and_safely_handles_invalid_geometry() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.] {
            assert_eq!(RepeatViewport::new(Some(value)), Err(RepeatViewportError));
            assert_eq!(
                RepeatViewport::default().resolve(value, 10, 600.),
                (0., false)
            );
            assert_eq!(
                RepeatViewport::default().resolve(44., 10, value),
                (0., true)
            );
        }
    }
}
