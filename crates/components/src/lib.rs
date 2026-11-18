//! GUIraffe components: Mantine-inspired components that GPUI Controls doesn't provide.
//!
//! Every component takes colors, radii, spacing and shadows from `guiraffe-core`'s theme,
//! uses Mantine prop names (`.size(Size::Md)`, `.variant(Variant::Light)`, `.color(..)`),
//! and is either a stateless *component* (`RenderOnce`) or a *control* (state entity).

pub mod avatar_group;
pub mod burger;
pub mod color_swatch;
pub mod copy_button;
pub mod focus;
pub mod highlight;
pub mod icons;
pub mod kbd;
pub mod loader;
pub mod misc;
pub mod paper;
pub mod ring;
pub mod segmented;
pub mod spoiler;
pub mod stepper;
mod style;
pub mod timeline;
pub mod typography;

pub use avatar_group::{AvatarGroup, Person};
pub use burger::Burger;
pub use color_swatch::{ColorSwatch, SwatchSelected, check_color};
pub use copy_button::CopyButton;
pub use focus::ScopedButton;
pub use highlight::{Highlight, Mark, highlight_ranges};
pub use icons::{ActionIcon, ThemeIcon, close_button};
pub use kbd::{KbdGroup, KbdSeparator};
pub use loader::{Loader, LoaderType};
pub use misc::{Blockquote, Indicator, IndicatorPosition, Pill};
pub use paper::{Card, CardSection, Paper};
pub use ring::{RingProgress, arc_spans};
pub use segmented::{SegmentedControl, SegmentedEvent};
pub use spoiler::Spoiler;
pub use stepper::{Step, StepState, Stepper, connector_complete, step_state};
pub use timeline::{ItemState, Timeline, TimelineItem};
pub use typography::{Text, Title};

/// Initialize dependencies (theme, motion). Idempotent.
pub fn init(cx: &mut gpui_kit::App) {
    guiraffe_core::init(cx);
    guiraffe_motion::init(cx);
    segmented::init(cx);
}
