//! Screen documents used by the showcase and the headless snapshot suite.

use std::sync::Arc;

use gpui_kit::{AnyView, App};
use guiraffe_ui::{DataUi, Document, NoLogic, Registry};

use crate::catalog::Entry;

fn render(raw: &str, cx: &mut App) -> AnyView {
    let document =
        Document::from_json(raw).unwrap_or_else(|error| panic!("invalid recipe: {error}"));
    let issues = guiraffe_ui::validation::Validation::default().check(&document);
    assert!(issues.is_empty(), "invalid recipe: {issues:?}");
    DataUi::new(&document, Arc::new(Registry::with_builtins()), NoLogic, cx)
        .unwrap_or_else(|error| panic!("invalid recipe: {error}"))
        .into()
}

macro_rules! recipes {
    ($( $file:literal => $name:literal, $summary:literal; )*) => {
        pub const DOCUMENTS: &[(&str, &str)] = &[
            $(($name, include_str!(concat!("../recipes/", $file, ".json")))),*
        ];

        pub(crate) fn entries() -> Vec<Entry> {
            vec![$(Entry {
                krate: "recipes",
                name: $name,
                summary: $summary,
                covers: &[],
                demo: |_, cx| render(include_str!(concat!("../recipes/", $file, ".json")), cx),
            }),*]
        }
    };
}

recipes! {
    "form" => "Recipe · Form", "Account form with valid and invalid fields.";
    "settings" => "Recipe · Settings", "Workspace preferences.";
    "dashboard" => "Recipe · Dashboard", "Service capacity and build queue.";
    "editor" => "Recipe · Editor", "Markdown editor layout with a toolbar and preview.";
    "split-pane" => "Recipe · Split-pane", "Two-column navigator and inspector.";
    "data" => "Recipe · Data screen", "Issue grid with filters.";
    "chat" => "Recipe · Chat", "Conversation and reply composer.";
    "wizard" => "Recipe · Wizard", "Member invitation step in workspace setup.";
}
