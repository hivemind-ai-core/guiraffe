//! Overlay metadata shared with the authoring catalogue generator.

use super::widget_spec::{EventSpec, PropSpec, WidgetSpec};

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

const fn placement_px(name: &'static str, default_json: &'static str, minimum: f64) -> PropSpec {
    PropSpec {
        name,
        type_name: "number",
        default_json,
        accepted: &[],
        range: Some((minimum, f32::MAX as f64)),
        token_backed: false,
        required: false,
    }
}

const fn placement_flag(name: &'static str) -> PropSpec {
    PropSpec {
        name,
        type_name: "boolean",
        default_json: "true",
        accepted: &[],
        range: None,
        token_backed: false,
        required: false,
    }
}

pub static FLOATING_TOOLTIP: WidgetSpec = WidgetSpec {
    id: "floating-tooltip",
    krate: "overlays",
    summary: "Controlled tooltip positioned at a rectangle in window logical pixels.",
    props: &[
        PropSpec {
            required: true,
            ..choice("text", "\"Tooltip\"", &[], false)
        },
        placement_px("x", "0", -(f32::MAX as f64)),
        placement_px("y", "0", -(f32::MAX as f64)),
        placement_px("width", "0", 0.0),
        placement_px("height", "0", 0.0),
        choice(
            "side",
            "\"bottom\"",
            &["top", "right", "bottom", "left"],
            false,
        ),
        choice("align", "\"center\"", &["start", "center", "end"], false),
        placement_px("gap", "4", 0.0),
        placement_px("padding", "8", 0.0),
        placement_px("arrow-padding", "8", 0.0),
        placement_flag("flip"),
        placement_flag("shift"),
    ],
    accepts_children: false,
    events: &[],
    state: "controlled from the anchor coordinates and surrounding document conditions",
    theme_tokens: &[
        "surfaces.surface",
        "surfaces.border",
        "surfaces.text",
        "default_radius",
    ],
    a11y_role: "tooltip",
};

pub static SPOTLIGHT: WidgetSpec = WidgetSpec {
    id: "spotlight",
    krate: "overlays",
    summary: "Search and select commands from a document-provided list.",
    props: &[PropSpec {
        name: "commands",
        type_name: "spotlight-commands",
        default_json: "[]",
        accepted: &[],
        range: None,
        required: false,
        token_backed: false,
    }],
    accepts_children: false,
    events: &[EventSpec {
        id: "select",
        payload_type: "string",
    }],
    state: "keyed search input and result selection",
    theme_tokens: &["surfaces.hover", "control_size"],
    a11y_role: "combobox",
};

pub const WIDGETS: &[&WidgetSpec] = &[
    &AFFIX,
    &LOADING_OVERLAY,
    &FLOATING_TOOLTIP,
    &SPOTLIGHT,
    &MODAL,
    &DRAWER,
    &TOAST_TRAY,
];

const fn service_property(
    name: &'static str,
    type_name: &'static str,
    default_json: &'static str,
) -> PropSpec {
    PropSpec {
        name,
        type_name,
        default_json,
        accepted: &[],
        range: None,
        token_backed: false,
        required: false,
    }
}

pub static MODAL: WidgetSpec = WidgetSpec {
    id: "modal",
    krate: "overlays",
    summary: "Open a managed dialog at the window root with text or a host-registered context.",
    props: &[
        service_property("open", "boolean", "false"),
        service_property("title", "string", "\"\""),
        service_property("message", "string", "\"\""),
        service_property("ok-label", "string", "null"),
        service_property("cancel-label", "string", "null"),
        service_property("keyboard", "boolean", "true"),
        service_property("overlay-closable", "boolean", "true"),
        service_property("overlay", "boolean", "true"),
        service_property("close-button", "boolean", "true"),
        PropSpec {
            range: Some((1., f32::MAX as f64)),
            ..service_property("width", "number", "null")
        },
        service_property("context", "string", "null"),
    ],
    accepts_children: false,
    events: &[
        EventSpec {
            id: "ok",
            payload_type: "null",
        },
        EventSpec {
            id: "cancel",
            payload_type: "null",
        },
        EventSpec {
            id: "close",
            payload_type: "null",
        },
        EventSpec {
            id: "error",
            payload_type: "string",
        },
    ],
    state: "document-owned request in the window overlay stack",
    theme_tokens: &["surfaces.body", "surfaces.text", "surfaces.border"],
    a11y_role: "dialog",
};

pub static TOAST_TRAY: WidgetSpec = WidgetSpec {
    id: "toast-tray",
    krate: "overlays",
    summary: "Add, update and remove document-owned notifications on the window's native toast tray.",
    props: &[
        service_property("toasts", "overlay-toasts", "[]"),
        PropSpec {
            range: Some((0., 100.)),
            ..service_property("toast-limit", "integer", "null")
        },
    ],
    accepts_children: true,
    events: &[EventSpec {
        id: "error",
        payload_type: "string",
    }],
    state: "window notification queue with document-local record IDs",
    theme_tokens: &["surfaces.body", "surfaces.text"],
    a11y_role: "group",
};

pub static DRAWER: WidgetSpec = WidgetSpec {
    id: "drawer",
    krate: "overlays",
    summary: "Window sheet with an optional overlay and an event when the user closes it.",
    props: &[
        service_property("open", "boolean", "false"),
        service_property("title", "string", "\"\""),
        service_property("message", "string", "\"\""),
        PropSpec {
            accepted: &["top", "left", "bottom", "right"],
            ..service_property("position", "string", "\"right\"")
        },
        PropSpec {
            accepted: &["xs", "sm", "md", "lg", "xl"],
            token_backed: true,
            ..service_property("size", "string", "\"md\"")
        },
        service_property("close-on-escape", "boolean", "true"),
        service_property("close-on-click-outside", "boolean", "true"),
        service_property("overlay", "boolean", "true"),
        service_property("resizable", "boolean", "true"),
    ],
    accepts_children: false,
    events: &[
        EventSpec {
            id: "close",
            payload_type: "null",
        },
        EventSpec {
            id: "error",
            payload_type: "string",
        },
    ],
    state: "document-owned request for the window's native sheet",
    theme_tokens: &["surfaces.body", "surfaces.text"],
    a11y_role: "dialog",
};
