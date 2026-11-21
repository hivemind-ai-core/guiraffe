//! Layout metadata shared with the authoring catalogue generator.

use super::widget_spec::{EventSpec, PropSpec, WidgetSpec};

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

pub static SPLITTER: WidgetSpec = WidgetSpec {
    id: "splitter",
    krate: "layout",
    summary: "Resizable panes with optional persisted divider positions.",
    props: &[
        PropSpec {
            name: "axis",
            type_name: "string",
            default_json: "\"horizontal\"",
            accepted: &["horizontal", "vertical"],
            range: None,
            required: false,
            token_backed: false,
        },
        PropSpec {
            name: "pane-sizes",
            type_name: "splitter-sizes",
            default_json: "[]",
            accepted: &[],
            range: None,
            required: false,
            token_backed: false,
        },
        number("first-size", "null", 0.0),
        number("min-size", "72", 0.0),
        number("max-size", "null", 0.0),
        PropSpec {
            name: "persist-key",
            type_name: "string",
            default_json: "null",
            accepted: &[],
            range: None,
            required: false,
            token_backed: false,
        },
    ],
    accepts_children: true,
    events: &[EventSpec {
        id: "resize",
        payload_type: "number-array",
    }],
    state: "keyed native pane sizes and optional layout persistence",
    theme_tokens: &["surfaces.border", "surfaces.hover"],
    a11y_role: "group",
};

pub static RESPONSIVE_GRID: WidgetSpec = WidgetSpec {
    id: "responsive-grid",
    krate: "layout",
    summary: "Responsive columns with optional span and offset records for each child, in document order.",
    props: &[
        PropSpec {
            name: "columns",
            type_name: "integer",
            default_json: "12",
            accepted: &[],
            range: Some((1.0, u16::MAX as f64)),
            token_backed: false,
            required: false,
        },
        size("gutter"),
        number("gutter-px", "null", 0.0),
        choice(
            "justify",
            "\"start\"",
            &["start", "center", "end", "between", "around", "evenly"],
        ),
        PropSpec {
            name: "grow",
            type_name: "boolean",
            default_json: "false",
            accepted: &[],
            range: None,
            token_backed: false,
            required: false,
        },
        choice("width-source", "\"container\"", &["container", "viewport"]),
        number("width-px", "null", 0.0),
        PropSpec {
            name: "placements",
            type_name: "grid-placements",
            default_json: "[]",
            accepted: &[],
            range: None,
            token_backed: false,
            required: false,
        },
    ],
    accepts_children: true,
    events: &[],
    state: "measured container width local to the document node",
    theme_tokens: &[],
    a11y_role: "group",
};

pub static APP_SHELL: WidgetSpec = WidgetSpec {
    id: "app-shell",
    krate: "layout",
    summary: "Application layout with header, navigation, main, aside and footer. Assign each child a region using slots; unassigned children appear in main.",
    props: &[
        PropSpec {
            name: "slots",
            type_name: "app-shell-slots",
            default_json: "[]",
            accepted: &[],
            range: None,
            token_backed: false,
            required: false,
        },
        number("header-height", "null", 0.0),
        number("footer-height", "null", 0.0),
        size("padding"),
        boolean("border", "false"),
        choice(
            "collapse-below",
            "null",
            &["base", "xs", "sm", "md", "lg", "xl"],
        ),
        choice(
            "collapse-aside-below",
            "null",
            &["base", "xs", "sm", "md", "lg", "xl"],
        ),
        boolean("resizable-navbar", "false"),
        boolean("navbar-collapsed", "null"),
        boolean("aside-collapsed", "null"),
        number("navbar-width", "null", 0.0),
        number("aside-width", "null", 0.0),
        boolean("resizable-aside", "false"),
    ],
    accepts_children: true,
    events: &[
        EventSpec {
            id: "navbar-collapsed",
            payload_type: "boolean",
        },
        EventSpec {
            id: "aside-collapsed",
            payload_type: "boolean",
        },
        EventSpec {
            id: "navbar-resized",
            payload_type: "number",
        },
        EventSpec {
            id: "aside-resized",
            payload_type: "number",
        },
    ],
    state: "keyed native state for responsive visibility and panel sizing",
    theme_tokens: &["surfaces.border"],
    a11y_role: "group",
};

const fn boolean(name: &'static str, default_json: &'static str) -> PropSpec {
    PropSpec {
        name,
        type_name: "boolean",
        default_json,
        accepted: &[],
        range: None,
        token_backed: false,
        required: false,
    }
}

pub const WIDGETS: &[&WidgetSpec] = &[
    &CENTER,
    &ASPECT_RATIO,
    &SPACE,
    &CONTAINER,
    &FLEX,
    &DIVIDER,
    &SCROLL_AREA,
    &SIMPLE_GRID,
    &SPLITTER,
    &RESPONSIVE_GRID,
    &APP_SHELL,
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
