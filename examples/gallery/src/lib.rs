//! The GUIraffe gallery: a reference app that exercises the whole toolkit together.

mod chat;
pub mod pages;
pub mod shell;
pub mod todos;

use guiraffe_router::routes;

routes! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum Page {
        Overview = "/",
        Components = "/components",
        State = "/state",
        Motion = "/motion",
        Flow = "/flow",
        Overlays = "/overlays",
        Windows = "/windows",
        Chat = "/chat",
    }
}

impl Page {
    pub const ALL: [Page; 8] = [
        Page::Overview,
        Page::Components,
        Page::State,
        Page::Motion,
        Page::Flow,
        Page::Overlays,
        Page::Windows,
        Page::Chat,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Page::Overview => "Overview",
            Page::Components => "Components",
            Page::State => "State",
            Page::Motion => "Motion",
            Page::Flow => "Flow",
            Page::Overlays => "Overlays",
            Page::Windows => "Windows",
            Page::Chat => "Agent chat",
        }
    }

    pub fn icon(self) -> gpui_kit::assets::IconName {
        use gpui_kit::assets::IconName as I;
        match self {
            Page::Overview => I::LayoutDashboard,
            Page::Components => I::Palette,
            Page::State => I::SquareTerminal,
            Page::Motion => I::Redo2,
            Page::Flow => I::Map,
            Page::Overlays => I::Bell,
            Page::Windows => I::Frame,
            Page::Chat => I::MessagesSquare,
        }
    }
}

/// Initialize every crate the gallery uses (idempotent).
pub fn init(cx: &mut gpui_kit::App) {
    guiraffe_components::init(cx);
    guiraffe_chat::init(cx);
    guiraffe_overlays::init(cx);
    guiraffe_router::init(cx);
    guiraffe_state::init(cx);
    #[cfg(feature = "devtools")]
    guiraffe_devtools::init(cx);
}
