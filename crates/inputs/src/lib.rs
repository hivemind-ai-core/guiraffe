//! GUIraffe inputs: Mantine-style inputs that GPUI Controls doesn't provide.
//! (Text, number, PIN/OTP, textarea, select and checkbox come from Controls.)
//! Date and time pickers live in [`dates`].

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
