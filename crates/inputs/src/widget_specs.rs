//! Metadata shared by the inputs renderer and the authoring catalogue generator.

use super::widget_spec::{PropSpec, WidgetSpec};

pub static PASSWORD_STRENGTH: WidgetSpec = WidgetSpec {
    id: "password-strength",
    krate: "inputs",
    summary: "Four-segment password strength meter.",
    props: &[PropSpec {
        name: "password",
        type_name: "string",
        default_json: "\"\"",
        accepted: &[],
        range: None,
        token_backed: false,
        required: false,
    }],
    accepts_children: false,
    events: &[],
    state: "stateless",
    theme_tokens: &[],
    a11y_role: "meter",
};

pub const WIDGETS: &[&WidgetSpec] = &[
    &PASSWORD_STRENGTH,
    &CHIP_GROUP,
    &RANGE_SLIDER,
    &DROPZONE,
    &TAGS_INPUT,
    &DATE_PICKER,
    &TIME_FIELD,
    &PICKER,
    &INPUT_WRAPPER,
];

const SIZES: &[&str] = &["xs", "sm", "md", "lg", "xl"];
const COLORS: &[&str] = &[
    "dark", "gray", "red", "pink", "grape", "violet", "indigo", "blue", "cyan", "teal", "green",
    "lime", "yellow", "orange",
];

const fn choice(
    name: &'static str,
    default_json: &'static str,
    accepted: &'static [&'static str],
) -> PropSpec {
    PropSpec {
        accepted,
        ..option(name, "string", default_json)
    }
}

const fn token(name: &'static str, accepted: &'static [&'static str]) -> PropSpec {
    PropSpec {
        accepted,
        token_backed: true,
        ..option(name, "string", "null")
    }
}

/// GUIraffe's own date and time picker ([`crate::dates::Picker`]); `mode` selects the value
/// type. Values are local ISO strings: `2026-11-21`, `13:30`, `2026-11-21T13:30`, or a
/// `[start, end]` pair for the range modes.
pub static PICKER: WidgetSpec = WidgetSpec {
    id: "picker",
    krate: "inputs",
    summary: "Themed date, date-range, time, date-time or date-time-range picker with presets and locale.",
    props: &[
        choice(
            "mode",
            "\"date\"",
            &["date", "date-range", "time", "date-time", "date-time-range"],
        ),
        option("value", "picker-value", "null"),
        option("default-value", "picker-value", "null"),
        option("open", "boolean", "null"),
        option("placeholder", "string", "null"),
        option("label", "string", "null"),
        option("description", "string", "null"),
        option("error", "string", "null"),
        option("required", "boolean", "false"),
        option("min-date", "iso-date", "null"),
        option("max-date", "iso-date", "null"),
        option("exclude-date", "array", "[]"),
        option("min-time", "iso-time", "null"),
        option("max-time", "iso-time", "null"),
        option("default-time", "iso-time", "\"00:00\""),
        choice(
            "first-day-of-week",
            "null",
            &["sun", "mon", "tue", "wed", "thu", "fri", "sat"],
        ),
        choice("locale", "\"en-US\"", &["en-US", "en-GB", "de", "fr"]),
        choice("hour-format", "null", &["h12", "h24"]),
        option("with-seconds", "boolean", "false"),
        option("value-format", "string", "null"),
        option("clearable", "boolean", "false"),
        option("presets", "picker-presets", "[]"),
        PropSpec {
            range: Some((1.0, 12.0)),
            ..option("number-of-months", "integer", "1")
        },
        token("size", SIZES),
        choice("variant", "\"default\"", &["default", "filled", "unstyled"]),
        token("radius", SIZES),
        token("color", COLORS),
        option("disabled", "boolean", "false"),
        option("read-only", "boolean", "false"),
        option("focusable", "boolean", "null"),
        option("today", "iso-date", "null"),
    ],
    accepts_children: false,
    events: &[
        super::widget_spec::EventSpec {
            id: "change",
            payload_type: "picker-value",
        },
        super::widget_spec::EventSpec {
            id: "open-change",
            payload_type: "boolean",
        },
    ],
    state: "keyed picker; value and open apply when the document changes them",
    theme_tokens: &["surfaces.body", "surfaces.border", "control_size"],
    a11y_role: "combobox",
};

/// Label, description, required marker and error around any child control.
pub static INPUT_WRAPPER: WidgetSpec = WidgetSpec {
    id: "input-wrapper",
    krate: "inputs",
    summary: "Label, description, required marker and error message around a child control.",
    props: &[
        option("label", "string", "null"),
        option("description", "string", "null"),
        option("error", "string", "null"),
        option("required", "boolean", "false"),
        PropSpec {
            accepted: SIZES,
            token_backed: true,
            ..option("size", "string", "\"sm\"")
        },
    ],
    accepts_children: true,
    events: &[],
    state: "stateless",
    theme_tokens: &["surfaces.text"],
    a11y_role: "group",
};

const fn option(
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

pub static CHIP_GROUP: WidgetSpec = WidgetSpec {
    id: "chip-group",
    krate: "inputs",
    summary: "A row of toggleable chips. Selection is a list of zero-based option indices.",
    props: &[
        option("options", "array", "[]"),
        option("multiple", "boolean", "false"),
        option("value", "integer-array", "[]"),
        option("disabled", "boolean", "false"),
        option("focusable", "boolean", "null"),
        PropSpec {
            name: "color",
            type_name: "string",
            default_json: "null",
            accepted: &[
                "dark", "gray", "red", "pink", "grape", "violet", "indigo", "blue", "cyan", "teal",
                "green", "lime", "yellow", "orange",
            ],
            range: None,
            token_backed: true,
            required: false,
        },
    ],
    accepts_children: false,
    events: &[super::widget_spec::EventSpec {
        id: "change",
        payload_type: "integer-array",
    }],
    state: "controlled or uncontrolled selection",
    theme_tokens: &[],
    a11y_role: "group",
};

pub static RANGE_SLIDER: WidgetSpec = WidgetSpec {
    id: "range-slider",
    krate: "inputs",
    summary: "Two-thumb slider. The value is an array of low and high bounds.",
    props: &[
        option("min", "number", "0"),
        option("max", "number", "100"),
        option("step", "number", "1"),
        option("min-distance", "number", "0"),
        option("value", "range-value", "[0,100]"),
        option("label", "string", "\"Range\""),
        option("disabled", "boolean", "false"),
        option("focusable", "boolean", "null"),
    ],
    accepts_children: false,
    events: &[super::widget_spec::EventSpec {
        id: "change",
        payload_type: "range-value",
    }],
    state: "controlled or uncontrolled range",
    theme_tokens: &[],
    a11y_role: "slider",
};

pub static DROPZONE: WidgetSpec = WidgetSpec {
    id: "dropzone",
    krate: "inputs",
    summary: "File-drop target with extension or MIME-family filters and a browse event.",
    props: &[
        option("accept", "array", "[]"),
        option("label", "string", "\"Drop files\""),
        option("disabled", "boolean", "false"),
        option("focusable", "boolean", "null"),
    ],
    accepts_children: false,
    events: &[
        super::widget_spec::EventSpec {
            id: "accept",
            payload_type: "string-array",
        },
        super::widget_spec::EventSpec {
            id: "reject",
            payload_type: "string-array",
        },
        super::widget_spec::EventSpec {
            id: "browse",
            payload_type: "null",
        },
    ],
    state: "keyed focus and drag target",
    theme_tokens: &[],
    a11y_role: "button",
};

pub static TAGS_INPUT: WidgetSpec = WidgetSpec {
    id: "tags-input",
    krate: "inputs",
    summary: "Editable string tags with duplicate and length rules.",
    props: &[
        option("value", "array", "[]"),
        option("placeholder", "string", "\"\""),
        option("allow-duplicates", "boolean", "false"),
        option("split-chars", "string", "\",\""),
        PropSpec {
            range: Some((0.0, 65536.0)),
            ..option("max-tags", "number", "null")
        },
        PropSpec {
            range: Some((0.0, 65536.0)),
            ..option("max-len", "number", "64")
        },
    ],
    accepts_children: false,
    events: &[
        super::widget_spec::EventSpec {
            id: "change",
            payload_type: "string-array",
        },
        super::widget_spec::EventSpec {
            id: "reject",
            payload_type: "tag-rejection",
        },
    ],
    state: "controlled or uncontrolled tags",
    theme_tokens: &[],
    a11y_role: "group",
};

pub static DATE_PICKER: WidgetSpec = WidgetSpec {
    id: "date-picker",
    krate: "gpui-kit",
    summary: "Native date or date-range picker using ISO dates and local date-times.",
    props: &[
        option("value", "calendar-value", "null"),
        option("range", "boolean", "false"),
        option("placeholder", "string", "\"\""),
        option("cleanable", "boolean", "false"),
        option("appearance", "boolean", "true"),
        option("disabled", "boolean", "false"),
        PropSpec {
            accepted: &["xs", "sm", "md", "lg", "xl"],
            token_backed: true,
            ..option("size", "string", "null")
        },
        option("focus-ring", "boolean", "true"),
        PropSpec {
            range: Some((1.0, 12.0)),
            ..option("months", "number", "1")
        },
        option("date-format", "string", "null"),
        PropSpec {
            accepted: &["date", "minute", "second"],
            ..option("precision", "string", "\"date\"")
        },
        PropSpec {
            accepted: &["h12", "h23"],
            ..option("hour-cycle", "string", "\"h23\"")
        },
        option("default-time", "iso-time", "\"00:00\""),
        PropSpec {
            accepted: &["sun", "mon", "tue", "wed", "thu", "fri", "sat"],
            ..option("first-day", "string", "\"sun\"")
        },
        option("disabled-weekdays", "integer-array", "[]"),
        option("min-date", "iso-date", "null"),
        option("max-date", "iso-date", "null"),
        option("years", "integer-array", "null"),
        option("presets", "calendar-presets", "[]"),
    ],
    accepts_children: false,
    events: &[super::widget_spec::EventSpec {
        id: "change",
        payload_type: "calendar-value",
    }],
    state: "controlled or uncontrolled date, with keyed native popup state",
    theme_tokens: &[],
    a11y_role: "combobox",
};

pub static TIME_FIELD: WidgetSpec = WidgetSpec {
    id: "time-field",
    krate: "gpui-kit",
    summary: "Native segmented time editor using a local ISO time.",
    props: &[
        option("value", "iso-time", "\"00:00\""),
        PropSpec {
            accepted: &["minute", "second"],
            ..option("precision", "string", "\"minute\"")
        },
        PropSpec {
            accepted: &["h12", "h23"],
            ..option("hour-cycle", "string", "\"h23\"")
        },
        option("disabled", "boolean", "false"),
        option("invalid", "boolean", "false"),
        PropSpec {
            accepted: &["xs", "sm", "md", "lg", "xl"],
            token_backed: true,
            ..option("size", "string", "null")
        },
    ],
    accepts_children: false,
    events: &[super::widget_spec::EventSpec {
        id: "change",
        payload_type: "iso-time",
    }],
    state: "controlled or uncontrolled time, with keyed native segment focus",
    theme_tokens: &[],
    a11y_role: "group",
};
