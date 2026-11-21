//! GUIraffe inputs: Mantine-style inputs that GPUI Controls doesn't provide.
//! (Text, number, PIN/OTP, textarea, select and checkbox come from Controls.)

pub mod logic;
pub mod widgets;

pub use logic::{Range2, TagRejected, TagRules, Thumb};
pub use widgets::{
    ChipEvent, ChipGroup, DropEvent, Dropzone, PasswordStrength, RangeEvent, RangeSlider,
    TagsEvent, TagsInput,
};

/// Register key bindings (idempotent to call alongside other crates' init).
pub fn init(cx: &mut gpui_kit::App) {
    guiraffe_components::init(cx);
    widgets::init(cx);
    register_widgets(cx);
}

use guiraffe_core::widget_spec;
mod widget_specs;

fn register_widgets(cx: &mut gpui_kit::App) {
    use gpui_kit::IntoElement;
    use guiraffe_core::{
        widget_catalog::{WidgetRegistration, WidgetRegistry},
        widget_spec::Schema,
    };

    WidgetRegistry::install(
        cx,
        &[WidgetRegistration {
            spec: widget_specs::WIDGETS[0],
            schema: &Schema {
                props: &["password"],
                events: &[],
            },
            render: |node, _, _, _| {
                PasswordStrength::new(node.prop(0).as_str().unwrap_or_default()).into_any_element()
            },
        }],
    );
}
