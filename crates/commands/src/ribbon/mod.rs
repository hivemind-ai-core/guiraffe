//! The command ribbon (ROADMAP 0.3 P2 #13): a keyboard-first, contextual command bar over
//! the same registry as shortcuts, menus and Spotlight. See `docs/design/command-ribbon.md`.
//!
//! ```
//! use guiraffe_commands::{Command, CommandId, Registry};
//! use guiraffe_commands::ribbon::{Navigator, RibbonCategory, RibbonEntry, RibbonLayout,
//!     RibbonMenu, RibbonModel, Step};
//! use std::collections::BTreeMap;
//!
//! let save = CommandId::parse("file.save")?;
//! let mut registry = Registry::default();
//! registry.register(Command::new(save.clone(), "Save", ()))?;
//! let layout = RibbonLayout::new("root")
//!     .menu(RibbonMenu::new("root", "Commands")
//!         .category(RibbonCategory::new("Global").entry(RibbonEntry::menu("file", "f", "file"))))
//!     .menu(RibbonMenu::new("file", "File")
//!         .category(RibbonCategory::new("File").entry(RibbonEntry::command("save", "s", save.clone()))));
//! let model = RibbonModel::build(&layout, &registry, &BTreeMap::new())?;
//! let mut nav = Navigator::default();
//! let cx = Default::default();
//! assert_eq!(nav.press("f", &model, &registry, &cx), Ok(Step::Descended));
//! assert_eq!(nav.press("s", &model, &registry, &cx), Ok(Step::Command(save)));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
mod model;
mod overrides;
mod view;

pub use model::{
    Item, KeyOutcome, NavError, Navigator, RibbonCategory, RibbonEntry, RibbonError, RibbonKey,
    RibbonLayout, RibbonMenu, RibbonModel, RibbonTarget, Section, Step,
};
pub use overrides::{Loaded, MAX_OVERRIDE_BYTES, OverrideFile, Overrides};
pub use view::{
    CommandRibbon, RIBBON_CONTEXT, RIBBON_LEADER, RibbonButton, RibbonEvent, RibbonPlacement,
    RibbonProps, RibbonState, ribbon_scope,
};

/// Ribbon entries for every payload-free shared action of the controls registered on this
/// window's action bus, one submenu per control (keys 1–9, a–z in path order). Merge the
/// returned menus into a layout and reference them from a category.
pub fn control_menus(
    window: &gpui_kit::Window,
    cx: &gpui_kit::App,
) -> Vec<(RibbonEntry, RibbonMenu)> {
    use guiraffe_core::protocol::{ActionBus, names};
    const NEEDS_PAYLOAD: &[&str] = &[names::SET_VALUE, names::SELECT, names::SCROLL_TO];
    let keys: Vec<char> = "123456789abcdefghijklmnopqrstuvwxyz".chars().collect();
    let mut controls: Vec<(String, guiraffe_core::protocol::Capabilities)> =
        ActionBus::controls(window, cx)
            .filter_map(|(id, _, caps)| id.path(cx).map(|p| (p.to_owned(), caps)))
            .collect();
    controls.sort_by(|a, b| a.0.cmp(&b.0));
    controls
        .into_iter()
        .zip(keys.iter())
        .map(|((path, caps), key)| {
            let menu_id = format!("control:{path}");
            let mut category = RibbonCategory::new(path.clone());
            for (action, key) in caps
                .action_names()
                .filter(|name| !NEEDS_PAYLOAD.contains(name))
                .zip(keys.iter())
            {
                category = category.entry(RibbonEntry::action(
                    format!("{menu_id}/{action}"),
                    key.to_string(),
                    action.replace('_', " "),
                    path.clone(),
                    action,
                ));
            }
            (
                RibbonEntry::menu(format!("open:{menu_id}"), key.to_string(), menu_id.clone()),
                RibbonMenu::new(menu_id, path).category(category),
            )
        })
        .collect()
}

#[cfg(test)]
mod gpui_tests;
