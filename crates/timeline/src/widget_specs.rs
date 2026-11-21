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

pub const WIDGETS: &[&WidgetSpec] = &[&TRACKS, &GANTT];
