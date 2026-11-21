//! Tracks additions to public document authoring surfaces across component crates.

#[path = "catalogue_drift/source.rs"]
mod source;

use std::{collections::BTreeSet, path::Path};

fn kind(surface: &str) -> Option<&'static str> {
    Some(match surface {
        "code::CodeViewer" => "code-viewer",
        "components::Card" => "card",
        "components::Highlight" => "highlight",
        "components::Pill" => "pill",
        "components::RingProgress" => "ring",
        "components::Text" => "text",
        "components::Title" => "title",
        "inputs::ChipGroup" => "chip-group",
        "inputs::PasswordStrength" => "password-strength",
        "inputs::Dropzone" => "dropzone",
        "inputs::RangeSlider" => "range-slider",
        "inputs::TagsInput" => "tags-input",
        "layout::AspectRatio" => "aspect-ratio",
        "layout::Center" => "center",
        "layout::Space" => "space",
        "markdown::MarkdownView" => "markdown",
        "mermaid::MermaidView" => "mermaid",
        "overlays::Affix" => "affix",
        "overlays::LoadingOverlay" => "loading-overlay",
        "tree::TreeView" => "property-grid",
        _ => return None,
    })
}

// Exceptions deliberately name the owner and method. A new API must be classified explicitly.
const IMPERATIVE: &[(&str, &str)] = &[
    (
        "inputs::ChipGroup::selected",
        "Read the current selection from the value binding.",
    ),
    (
        "inputs::ChipGroup::toggle",
        "Host-side action. Document users drive the value binding or click a chip.",
    ),
    (
        "inputs::PasswordStrength::strength",
        "The meter derives this read-only score from its password.",
    ),
    (
        "markdown::MarkdownView::state",
        "A document supplies source data. An application-owned MarkdownState entity is a Rust API.",
    ),
    (
        "mermaid::MermaidView::state",
        "The document owns keyed state and supplies diagram source through a property.",
    ),
    (
        "mermaid::MermaidView::create_state",
        "Allocates an application-owned entity for callers building a view in Rust.",
    ),
    (
        "tree::TreeView::expand_loaded",
        "An imperative batch command. Documents name the desired expanded row ids.",
    ),
    (
        "tree::TreeView::collapse_all",
        "An imperative command. Setting expanded to an empty list closes every row.",
    ),
    (
        "inputs::Dropzone::disabled",
        "Read-only accessor for the disabled property.",
    ),
    (
        "inputs::Dropzone::receive",
        "Host-side delivery of native paths. Document users receive accept or reject events from the drop surface.",
    ),
    (
        "inputs::RangeSlider::values",
        "Read current bounds through the value binding.",
    ),
    (
        "inputs::RangeSlider::set",
        "Drive either thumb through the value pair.",
    ),
    (
        "inputs::TagsInput::input_for_test",
        "Test-only native editor reference.",
    ),
    (
        "inputs::TagsInput::tags",
        "Read the current list through the value binding.",
    ),
    (
        "inputs::TagsInput::add",
        "Update the value list or use the input.",
    ),
    (
        "inputs::TagsInput::remove",
        "Update the value list or use the remove control.",
    ),
];

fn covered(surface: &str) -> bool {
    if IMPERATIVE
        .iter()
        .any(|(api, reason)| *api == surface && !reason.is_empty())
    {
        return true;
    }
    if let Some(kind) = kind(surface) {
        return guiraffe_ui::catalog::lookup(kind).is_some();
    }
    let Some((owner, method)) = surface.rsplit_once("::") else {
        return false;
    };
    let Some(spec) = kind(owner).and_then(guiraffe_ui::catalog::lookup) else {
        return false;
    };
    match method {
        "new" => true, // Constructor parameters are declared by the kind's descriptor.
        "child" | "children" => spec.accepts_children,
        _ => {
            let property = match (owner, method) {
                ("components::Card", "p") => "padding".into(),
                ("components::Text", "fw") => "font-weight".into(),
                ("components::RingProgress", "section") => "sections".into(),
                ("layout::Space", "h") => "height".into(),
                ("layout::Space", "w") => "width".into(),
                ("tree::TreeView", "inspector_controls") => "searchable".into(),
                ("tree::TreeView", "row_style" | "set_row_style") => "density".into(),
                ("tree::TreeView", "row_heights" | "set_row_heights" | "replace_source") => {
                    "nodes".into()
                }
                (_, name) => name.strip_prefix("set_").unwrap_or(name).replace('_', "-"),
            };
            let event = method
                .strip_prefix("on_")
                .map(|name| name.replace('_', "-"));
            spec.props.iter().any(|p| p.name == property)
                || spec.events.iter().any(|e| event.as_deref() == Some(e.id))
        }
    }
}

#[test]
fn public_document_surface_does_not_regress() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let public = source::public_surface(&crates);
    let uncovered: BTreeSet<_> = public
        .iter()
        .filter(|api| !covered(api))
        .map(String::as_str)
        .collect();
    let known_debt: BTreeSet<_> = include_str!("catalogue_drift/known_gaps.txt")
        .lines()
        .collect();
    let missing: Vec<_> = uncovered.difference(&known_debt).collect();
    let stale: Vec<_> = known_debt.difference(&uncovered).collect();
    assert!(
        missing.is_empty() && stale.is_empty(),
        "Unclassified public DataUi APIs: {missing:#?}\nRemove resolved debt: {stale:#?}"
    );
}
