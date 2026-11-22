//! Calendar metadata shared with the authoring catalogue generator.

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

const fn event(id: &'static str, payload_type: &'static str) -> EventSpec {
    EventSpec { id, payload_type }
}

pub static CALENDAR: WidgetSpec = WidgetSpec {
    id: "calendar",
    krate: "calendar",
    summary: "Year, month, week and day views over colored event sources, with business hours, a now-line and proposed edits.",
    props: &[
        prop("items", "calendar-items", "[]"),
        prop("sources", "calendar-sources", "[]"),
        choice("view", "\"month\"", &["year", "month", "week", "day"]),
        prop("date", "iso-date", "null"),
        prop("now", "string", "null"),
        prop("zone", "string", "\"UTC\""),
        choice("locale", "\"en-US\"", &["en-US", "en-GB"]),
        choice(
            "week-start",
            "\"locale\"",
            &[
                "locale",
                "sunday",
                "monday",
                "tuesday",
                "wednesday",
                "thursday",
                "friday",
                "saturday",
            ],
        ),
        choice("time-format", "\"locale\"", &["locale", "12h", "24h"]),
        prop("iso-weeks", "boolean", "false"),
        prop("business-hours", "boolean", "true"),
        prop("now-line", "boolean", "true"),
        PropSpec {
            range: Some((0.0, 1_440.0)),
            ..prop("scroll-minutes", "integer", "480")
        },
        prop("editable", "boolean", "true"),
        prop("focusable", "boolean", "null"),
        prop("show-toolbar", "boolean", "true"),
    ],
    accepts_children: false,
    events: &[
        event("navigate", "iso-date"),
        event("view-change", "string"),
        event("date-select", "iso-date"),
        event("item-select", "integer"),
        event("overflow", "iso-date"),
        event("edit-request", "calendar-change"),
        event("source-visibility", "calendar-source-visibility"),
        event("announce", "string"),
        event("error", "string"),
    ],
    state: "keyed view, visible date, selection, cursor, scroll and edit preview; the model is replaced when items or sources change",
    theme_tokens: &[
        "surfaces.border",
        "surfaces.track",
        "surfaces.surface",
        "density.space",
    ],
    a11y_role: "grid",
};

pub const WIDGETS: &[&WidgetSpec] = &[&CALENDAR];
