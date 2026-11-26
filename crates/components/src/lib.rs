//! GUIraffe components: Mantine-inspired components that GPUI Controls doesn't provide.
//!
//! Every component takes colors, radii, spacing and shadows from `guiraffe-core`'s theme,
//! uses Mantine prop names (`.size(Size::Md)`, `.variant(Variant::Light)`, `.color(..)`),
//! and is either a stateless *component* (`RenderOnce`) or a *control* (state entity).
//!
//! In `ui!`, place builder arguments after the component name and properties inside the block.
//!
//! ```ignore
//! ui! { Card { p: Size::Md;
//!     Text("Details") { fw: FontWeight::SEMIBOLD }
//! } }
//! ```

use guiraffe_core::widget_spec;
mod widget_specs;

pub mod accordion;
pub mod avatar_group;
pub mod burger;
pub mod color_swatch;
pub mod controls;
pub mod copy_button;
mod disclosure_data;
mod disclosure_motion;
pub mod expanding_list;
pub mod focus;
pub mod highlight;
pub mod icons;
pub mod kbd;
pub mod loader;
pub mod mapping;
pub mod misc;
pub mod paper;
pub mod ring;
pub mod segmented;
pub mod speed_dial;
pub mod spoiler;
pub mod stepper;
mod style;
pub mod timeline;
pub mod typography;

pub use accordion::{
    Accordion, AccordionChange, AccordionOptions, AccordionVariant, ChevronPosition, DisclosureItem,
};
pub use avatar_group::{AvatarGroup, Person};
pub use burger::Burger;
pub use color_swatch::{ColorSwatch, SwatchSelected, check_color};
pub use copy_button::CopyButton;
pub use expanding_list::{ExpandingList, ExpandingListChange};
pub use focus::ScopedButton;
pub use highlight::{Highlight, Mark, highlight_ranges};
pub use icons::{ActionIcon, ThemeIcon, close_button};
pub use kbd::{KbdGroup, KbdSeparator};
pub use loader::{Loader, LoaderType};
pub use misc::{Blockquote, Indicator, IndicatorPosition, Pill};
pub use paper::{Card, CardSection, Paper};
pub use ring::{RingProgress, arc_spans};
pub use segmented::{SegmentedControl, SegmentedEvent};
pub use speed_dial::{DialAction, DialArc, DialLayout, SpeedDial, SpeedDialEvent};
pub use spoiler::Spoiler;
pub use stepper::{Step, StepState, Stepper, connector_complete, step_state};
pub use timeline::{ItemState, Timeline, TimelineItem};
pub use typography::{Text, Title};

/// Initialize dependencies (theme, motion). Idempotent.
pub fn init(cx: &mut gpui_kit::App) {
    guiraffe_core::init(cx);
    guiraffe_motion::init(cx);
    segmented::init(cx);
    segmented::document::register(cx);
    burger::document::register(cx);
    accordion::document::register(cx);
    expanding_list::document::register(cx);
    copy_button::document::register(cx);
    spoiler::document::register(cx);
    speed_dial::document::register(cx);
}
