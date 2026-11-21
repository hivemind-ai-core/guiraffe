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

pub const WIDGETS: &[&WidgetSpec] = &[
    &CENTER,
    &ASPECT_RATIO,
    &SPACE,
    &CONTAINER,
    &FLEX,
    &DIVIDER,
    &SCROLL_AREA,
    &SIMPLE_GRID,
];

const fn choice(
    name: &'static str,
    default: &'static str,
    accepted: &'static [&'static str],
) -> PropSpec {
    PropSpec {
        default_json: default,
        accepted,
        token_backed: false,
        ..size(name)
    }
}

const fn flag(name: &'static str, default: &'static str) -> PropSpec {
    PropSpec {
        name,
        type_name: "boolean",
        default_json: default,
        accepted: &[],
        range: None,
        token_backed: false,
        required: false,
    }
}

pub static CONTAINER: WidgetSpec = WidgetSpec {
    id: "container",
    summary: "Centers content with an optional width limit and padding.",
    props: &[
        PropSpec {
            default_json: "\"md\"",
            ..size("size")
        },
        flag("fluid", "false"),
        size("padding"),
        number("padding-px", "null", 0.0),
    ],
    ..CENTER
};

pub static FLEX: WidgetSpec = WidgetSpec {
    id: "flex",
    summary: "Arranges children in a configurable flex row or column.",
    props: &[
        choice(
            "direction",
            "\"row\"",
            &["row", "row-reverse", "column", "column-reverse"],
        ),
        PropSpec {
            default_json: "\"md\"",
            ..size("gap")
        },
        number("gap-px", "null", 0.0),
        choice(
            "align",
            "\"start\"",
            &["start", "center", "end", "stretch", "baseline"],
        ),
        choice(
            "justify",
            "\"start\"",
            &["start", "center", "end", "between", "around", "evenly"],
        ),
        flag("grow", "false"),
        flag("wrap", "false"),
    ],
    ..CENTER
};

pub static DIVIDER: WidgetSpec = WidgetSpec {
    id: "divider",
    summary: "Horizontal or vertical divider with optional text.",
    props: &[
        choice("orientation", "\"horizontal\"", &["horizontal", "vertical"]),
        choice("label", "null", &[]),
        choice("label-position", "\"center\"", &["start", "center", "end"]),
        PropSpec {
            token_backed: true,
            ..choice(
                "color",
                "null",
                &[
                    "dark", "gray", "red", "pink", "grape", "violet", "indigo", "blue", "cyan",
                    "teal", "green", "lime", "yellow", "orange",
                ],
            )
        },
        number("thickness", "1", 0.0),
    ],
    accepts_children: false,
    a11y_role: "separator",
    ..CENTER
};

pub static SCROLL_AREA: WidgetSpec = WidgetSpec {
    id: "scroll-area",
    summary: "Scrollable content in one or both axes.",
    props: &[flag("scroll-x", "false"), flag("scroll-y", "true")],
    state: "keyed scroll position",
    ..CENTER
};

pub static SIMPLE_GRID: WidgetSpec = WidgetSpec {
    id: "simple-grid",
    summary: "Equal-width cells with a fixed or width-driven column count.",
    props: &[
        PropSpec {
            range: Some((1.0, 1024.0)),
            ..number("cols", "2", 1.0)
        },
        number("spacing", "16", 0.0),
        number("vertical-spacing", "null", 0.0),
        number("min-child-width", "null", 0.0),
    ],
    ..CENTER
};
