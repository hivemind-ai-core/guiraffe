//! Sheet widget metadata shared with the authoring catalogue generator.

use super::widget_spec::{EventSpec, PropSpec, WidgetSpec};

const fn prop(name: &'static str, type_name: &'static str, default_json: &'static str) -> PropSpec {
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

/// Largest row or column count a document may declare (the model itself is sparse).
const MAX_EXTENT: f64 = 1_000_000_000.0;

pub static SHEET: WidgetSpec = WidgetSpec {
    id: "sheet",
    krate: "sheet",
    summary: "Virtualized sparse spreadsheet with A1 formulas, fill, clipboard, undo and frozen panes.",
    props: &[
        PropSpec {
            range: Some((1.0, MAX_EXTENT)),
            ..prop("rows", "integer", "1000")
        },
        PropSpec {
            range: Some((1.0, MAX_EXTENT)),
            ..prop("cols", "integer", "26")
        },
        prop("cells", "sheet-cells", "[]"),
        prop("read-only", "boolean", "false"),
        PropSpec {
            range: Some((0.0, MAX_EXTENT)),
            ..prop("frozen-rows", "integer", "0")
        },
        PropSpec {
            range: Some((0.0, MAX_EXTENT)),
            ..prop("frozen-cols", "integer", "0")
        },
        PropSpec {
            range: Some((48.0, 4096.0)),
            ..prop("height", "number", "320")
        },
    ],
    accepts_children: false,
    events: &[EventSpec {
        id: "edit",
        payload_type: "sheet-edit",
    }],
    state: "sparse cell model with selection, undo history and background recalculation; document cells are applied as a diff",
    theme_tokens: &[],
    a11y_role: "grid",
};

pub const WIDGETS: &[&WidgetSpec] = &[&SHEET];
