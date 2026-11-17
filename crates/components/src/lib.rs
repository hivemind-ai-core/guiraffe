//! GUIraffe components: Mantine-inspired components that GPUI Controls doesn't provide.
//!
//! Every component takes colors, radii, spacing and shadows from `guiraffe-core`'s theme,
//! uses Mantine prop names (`.size(Size::Md)`, `.variant(Variant::Light)`, `.color(..)`),
//! and is either a stateless *component* (`RenderOnce`) or a *control* (state entity).

pub mod highlight;
pub mod icons;
pub mod loader;
pub mod misc;
pub mod paper;
pub mod ring;
pub mod segmented;
mod style;
pub mod timeline;
pub mod typography;

pub use highlight::{Highlight, Mark, highlight_ranges};
pub use icons::{ActionIcon, ThemeIcon, close_button};
pub use loader::{Loader, LoaderType};
pub use misc::{Blockquote, Indicator, IndicatorPosition, Pill};
pub use paper::{Card, CardSection, Paper};
pub use ring::{RingProgress, arc_spans};
pub use segmented::{SegmentedControl, SegmentedEvent};
pub use timeline::{ItemState, Timeline, TimelineItem};
pub use typography::{Text, Title};

/// Initialize dependencies (theme, motion). Idempotent.
pub fn init(cx: &mut gpui_kit::App) {
    guiraffe_core::init(cx);
    guiraffe_motion::init(cx);
    segmented::init(cx);
}
