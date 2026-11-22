//! Timeline metadata shared with the authoring catalogue generator.

use super::widget_spec::{EventSpec, PropSpec, WidgetSpec};

const fn prop(name: &'static str, type_name: &'static str, default_json: &'static str) -> PropSpec {
    PropSpec {
        name,
        type_name,
        default_json,
        range: None,
        accepted: &[],
        required: false,
        token_backed: false,
    }
}

const fn choice(
    name: &'static str,
    default_json: &'static str,
    accepted: &'static [&'static str],
) -> PropSpec {
    PropSpec {
        accepted,
        ..prop(name, "string", default_json)
    }
}

const LOCK: &[&str] = &["none", "badge", "dimmed", "hatched"];

pub static TRACKS: WidgetSpec = WidgetSpec {
    id: "tracks-view",
    krate: "timeline",
    summary: "Named tracks of clips on a shared time axis with seek, move and resize.",
    props: &[
        prop("tracks", "timeline-tracks", "[]"),
        choice("time-format", "\"seconds\"", &["seconds", "frames", "days"]),
        PropSpec {
            range: Some((1.0, 240.0)),
            ..prop("frames-per-second", "number", "30")
        },
        PropSpec {
            range: Some((1.0, f64::MAX)),
            ..prop("length", "number", "10")
        },
        prop("playhead", "number", "0"),
        prop("snap", "number", "null"),
        choice(
            "resize-edges",
            "\"both\"",
            &["none", "start", "end", "both"],
        ),
        prop("editable", "boolean", "true"),
        prop("focusable", "boolean", "null"),
        choice("lock-style", "\"badge\"", LOCK),
        choice("global-lock-style", "\"none\"", LOCK),
        prop("selected", "track-selection", "null"),
    ],
    accepts_children: false,
    events: &[
        EventSpec {
            id: "clip-change",
            payload_type: "timeline-clip-change",
        },
        EventSpec {
            id: "seek",
            payload_type: "number",
        },
    ],
    state: "keyed timeline with local viewport and edits",
    theme_tokens: &[],
    a11y_role: "group",
};

pub static GANTT: WidgetSpec = WidgetSpec {
    id: "gantt",
    krate: "timeline",
    summary: "Dependency schedule with editable task bars and critical paths.",
    props: &[
        prop("tasks", "gantt-tasks", "[]"),
        choice("time-format", "\"days\"", &["seconds", "frames", "days"]),
        PropSpec {
            range: Some((1.0, 240.0)),
            ..prop("frames-per-second", "number", "30")
        },
        choice(
            "resize-edges",
            "\"both\"",
            &["none", "start", "end", "both"],
        ),
        prop("editable", "boolean", "true"),
        prop("focusable", "boolean", "null"),
        prop("key-step", "number", "null"),
        choice("lock-style", "\"badge\"", LOCK),
        choice("global-lock-style", "\"none\"", LOCK),
        prop("locked", "integer-array", "[]"),
        PropSpec {
            range: Some((0.0, u32::MAX as f64)),
            ..prop("selected", "integer", "null")
        },
    ],
    accepts_children: false,
    events: &[
        EventSpec {
            id: "reschedule",
            payload_type: "gantt-reschedule",
        },
        EventSpec {
            id: "resize",
            payload_type: "gantt-resize",
        },
    ],
    state: "keyed schedule with local viewport and edits",
    theme_tokens: &[],
    a11y_role: "group",
};

pub static DOPE_SHEET: WidgetSpec = WidgetSpec {
    id: "dope-sheet",
    krate: "timeline",
    summary: "Hierarchical keyframe timing editor with selectable channels and frame snapping.",
    props: &[
        prop("channels", "dope-channels", "[]"),
        choice("time-format", "\"seconds\"", &["seconds", "frames", "days"]),
        PropSpec {
            range: Some((1.0, 240.0)),
            ..prop("frames-per-second", "number", "30")
        },
        PropSpec {
            range: Some((0.001, f64::MAX)),
            ..prop("length", "number", "10")
        },
        prop("editable", "boolean", "true"),
        prop("focusable", "boolean", "null"),
        choice("lock-style", "\"badge\"", LOCK),
        prop("playhead", "number", "0"),
    ],
    accepts_children: false,
    events: &[
        EventSpec {
            id: "key-retime",
            payload_type: "dope-retime",
        },
        EventSpec {
            id: "selection-change",
            payload_type: "dope-selection",
        },
        EventSpec {
            id: "change",
            payload_type: "dope-change",
        },
        EventSpec {
            id: "channel-change",
            payload_type: "dope-channel",
        },
    ],
    state: "keyed timing editor with local viewport, selection and key edits",
    theme_tokens: &[],
    a11y_role: "group",
};

pub static BAR_CURVE: WidgetSpec = WidgetSpec {
    id: "bar-curve",
    krate: "timeline",
    summary: "Draw quantized values across a fixed set of bars.",
    props: &[
        prop("values", "number-array", "[]"),
        prop("minimum", "number", "0"),
        prop("maximum", "number", "1"),
        PropSpec {
            range: Some((1.0, u32::MAX as f64)),
            ..prop("steps", "integer", "null")
        },
        prop("bipolar", "boolean", "false"),
        prop("editable", "boolean", "true"),
        prop("focusable", "boolean", "null"),
    ],
    accepts_children: false,
    events: &[EventSpec {
        id: "change",
        payload_type: "bar-edit",
    }],
    state: "keyed plot with local viewport and bar edits",
    theme_tokens: &[],
    a11y_role: "group",
};

pub static CURVE_EDITOR: WidgetSpec = WidgetSpec {
    id: "curve-editor",
    krate: "timeline",
    summary: "Multi-series keyframe curve editor with editable Bezier tangents.",
    props: &[
        prop("curves", "curve-series", "[]"),
        prop("time-start", "number", "0"),
        prop("time-end", "number", "10"),
        prop("minimum", "number", "0"),
        prop("maximum", "number", "1"),
        prop("editable", "boolean", "true"),
        prop("focusable", "boolean", "null"),
        choice(
            "tangent-mode",
            "\"free\"",
            &["free", "aligned", "mirrored", "flat"],
        ),
        prop("selection", "track-selection", "null"),
        choice("fit", "\"none\"", &["none", "visible", "normalize"]),
    ],
    accepts_children: false,
    events: &[
        EventSpec {
            id: "key-move",
            payload_type: "curve-key-move",
        },
        EventSpec {
            id: "change",
            payload_type: "curve-series",
        },
        EventSpec {
            id: "selection-change",
            payload_type: "curve-selection",
        },
    ],
    state: "keyed curve editor with local viewport, selection and edits",
    theme_tokens: &[],
    a11y_role: "group",
};

pub static EVENT_TIMELINE: WidgetSpec = WidgetSpec {
    id: "event-timeline",
    krate: "timeline",
    summary: "Events on parallel tracks in slot-aligned chronological order, with branch and merge connectors.",
    props: &[
        prop("tracks", "event-tracks", "[]"),
        prop("events", "timeline-events", "[]"),
        prop("links", "event-links", "[]"),
        choice("orientation", "\"vertical\"", &["vertical", "horizontal"]),
        PropSpec {
            accepted: &[
                "dark", "gray", "red", "pink", "grape", "violet", "indigo", "blue", "cyan", "teal",
                "green", "lime", "yellow", "orange",
            ],
            token_backed: true,
            ..prop("color", "string", "null")
        },
        PropSpec {
            accepted: &["xs", "sm", "md", "lg", "xl"],
            token_backed: true,
            ..prop("size", "string", "null")
        },
        prop("label", "string", "\"Event timeline\""),
        prop("focusable", "boolean", "null"),
    ],
    accepts_children: false,
    events: &[
        EventSpec {
            id: "select",
            payload_type: "integer",
        },
        EventSpec {
            id: "activate",
            payload_type: "integer",
        },
        EventSpec {
            id: "error",
            payload_type: "string",
        },
    ],
    state: "keyed cursor, selection and viewport; the model is replaced when the data changes",
    theme_tokens: &["surfaces.border", "control_size"],
    a11y_role: "grid",
};

pub const WIDGETS: &[&WidgetSpec] = &[
    &TRACKS,
    &GANTT,
    &DOPE_SHEET,
    &BAR_CURVE,
    &CURVE_EDITOR,
    &EVENT_TIMELINE,
];
