//! Overlay metadata shared with the authoring catalogue generator.

use super::widget_spec::{PropSpec, WidgetSpec};

const SIZES: &[&str] = &["xs", "sm", "md", "lg", "xl"];
const COLORS: &[&str] = &[
    "dark", "gray", "red", "pink", "grape", "violet", "indigo", "blue", "cyan", "teal", "green",
    "lime", "yellow", "orange",
];
const POSITIONS: &[&str] = &[
    "top-left",
    "top-center",
    "top-right",
    "bottom-left",
    "bottom-center",
    "bottom-right",
];
const VARIANTS: &[&str] = &[
    "filled",
    "light",
    "outline",
    "subtle",
    "transparent",
    "white",
    "default",
];

const fn choice(
    name: &'static str,
    default_json: &'static str,
    accepted: &'static [&'static str],
    token_backed: bool,
) -> PropSpec {
    PropSpec {
        name,
        type_name: "string",
        default_json,
        accepted,
        range: None,
        token_backed,
        required: false,
    }
}

pub static AFFIX: WidgetSpec = WidgetSpec {
    id: "affix",
    krate: "overlays",
    summary: "Anchors children to the edge of the containing block.",
    props: &[
        choice("position", "\"bottom-right\"", POSITIONS, false),
        choice("offset", "\"md\"", SIZES, true),
    ],
    accepts_children: true,
    events: &[],
    state: "stateless",
    theme_tokens: &[],
    a11y_role: "group",
};

pub static LOADING_OVERLAY: WidgetSpec = WidgetSpec {
    id: "loading-overlay",
    krate: "overlays",
    summary: "Blocks input over its containing block while loading.",
    props: &[
        PropSpec {
            name: "visible",
            type_name: "boolean",
            default_json: "true",
            accepted: &[],
            range: None,
            token_backed: false,
            required: false,
        },
        choice("size", "\"md\"", SIZES, true),
        choice("color", "null", COLORS, true),
        choice("variant", "\"light\"", VARIANTS, false),
    ],
    accepts_children: false,
    events: &[],
    state: "stateless",
    theme_tokens: &["surfaces.veil", "default_radius", "variant.foreground"],
    a11y_role: "progressbar",
};

pub const WIDGETS: &[&WidgetSpec] = &[&AFFIX, &LOADING_OVERLAY];
