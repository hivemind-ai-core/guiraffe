//! GUIraffe inputs: Mantine-style inputs that GPUI Controls doesn't provide.
//! (Text, number, PIN/OTP, textarea, select and checkbox come from Controls.)
//! Date and time pickers live in [`dates`].
//!
//! A Rust `ui!` example. Create shared entities before rendering when the host owns their state.
//!
//! ```ignore
//! ui! { div { PasswordStrength("A7!example") { } {ChipGroup::new(["Rust", "Zig"], true, cx)} } }
//! ```

pub mod dates;
pub mod logic;
pub mod widgets;

pub use dates::{
    DatePicker, DatePreset, DateRange, DateRangePicker, DateTimePicker, DateTimeRange,
    DateTimeRangePicker, InputWrapper, PickerEvent, PickerProps, PresetRange, TimePicker,
};
pub use logic::{Range2, TagRejected, TagRules, Thumb};
pub use widgets::{
    ChipEvent, ChipGroup, DropEvent, Dropzone, PasswordStrength, RangeEvent, RangeSlider,
    TagsEvent, TagsInput,
};

/// Register key bindings (idempotent to call alongside other crates' init).
pub fn init(cx: &mut gpui_kit::App) {
    guiraffe_components::init(cx);
    widgets::init(cx);
    dates::init(cx);
}

use guiraffe_core::widget_spec;
mod widget_specs;
