//! Layout metadata shared with the authoring catalogue generator.

use super::widget_spec::{PropSpec, WidgetSpec};

const SIZES: &[&str] = &["xs", "sm", "md", "lg", "xl"];

const fn size(name: &'static str) -> PropSpec {
    PropSpec {
        name,
        type_name: "string",
        default_json: "null",
        accepted: SIZES,
        range: None,
        token_backed: true,
        required: false,
    }
}

const fn number(name: &'static str, default_json: &'static str, min: f64) -> PropSpec {
    PropSpec {
        name,
        type_name: "number",
        default_json,
        accepted: &[],
        range: Some((min, f32::MAX as f64)),
        token_backed: false,
        required: false,
    }
}

pub static CENTER: WidgetSpec = WidgetSpec {
    id: "center",
    krate: "layout",
    summary: "Centers its children along both axes.",
    props: &[number("min-height", "0", 0.0)],
    accepts_children: true,
    events: &[],
    state: "stateless",
    theme_tokens: &[],
    a11y_role: "group",
};

pub static ASPECT_RATIO: WidgetSpec = WidgetSpec {
    id: "aspect-ratio",
    krate: "layout",
    summary: "Preserves a width-to-height ratio while arranging children.",
    props: &[number("ratio", "1", f32::MIN_POSITIVE as f64)],
    accepts_children: true,
    events: &[],
    state: "stateless",
    theme_tokens: &[],
    a11y_role: "group",
};

pub static SPACE: WidgetSpec = WidgetSpec {
    id: "space",
    krate: "layout",
    summary: "Fixed empty space. Pixel dimensions take precedence over size tokens.",
    props: &[
        size("width"),
        size("height"),
        number("width-px", "null", 0.0),
        number("height-px", "null", 0.0),
    ],
    accepts_children: false,
    events: &[],
    state: "stateless",
    theme_tokens: &[],
    a11y_role: "presentation",
};

pub const WIDGETS: &[&WidgetSpec] = &[&CENTER, &ASPECT_RATIO, &SPACE];
