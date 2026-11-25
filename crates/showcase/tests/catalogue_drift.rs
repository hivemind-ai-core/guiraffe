//! Tracks additions to public document authoring surfaces across component crates.

#[path = "catalogue_drift/source.rs"]
mod source;

use std::{collections::BTreeSet, path::Path};

fn kind(surface: &str) -> Option<&'static str> {
    Some(match surface {
        "chat::Chat" | "chat::ChatState" | "chat::Composer" | "chat::ChatProps" => "chat",
        "charts::ChartView" | "charts::ChartOptions" | "charts::ChartSpec" => "chart",
        "code::CodeViewer" | "code::CodeOptions" => "code-viewer",
        "datagrid::DataGrid" | "datagrid::GridOptions" | "datagrid::GridEvents" => "data-grid",
        "dnd::SortableBoard" => "sortable-board",
        "dnd::fn new_dock" | "dnd::fn new_native_dock" => "dock",
        "flamegraph::FlameGraph" => "flame-graph",
        "flow::Flow" | "flow::FlowCanvas" | "flow::FlowState" | "flow::FlowViewport"
        | "flow::FlowOptions" => "flow",
        "code::DecorationLayerOptions" => "code-viewer",
        "flow-core::LayoutOptions" => "flow",
        "components::CardSection" => "card-section",
        "components::ScopedButton" => "button",
        "components::SpeedDial" => "speed-dial",
        "components::Spoiler" => "spoiler",
        "layout::AppShell" | "layout::AppShellState" => "app-shell",
        "layout::Grid" | "layout::Col" => "responsive-grid",
        "commands::CommandScope" => "command-scope",
        "commands::Menu" | "commands::MenuProps" => "command-menu",
        "commands::CommandRibbon"
        | "commands::fn ribbon_scope"
        | "commands::RibbonButton"
        | "commands::RibbonProps"
        | "commands::RibbonState" => "command-ribbon",
        "components::ActionIcon" => "action-icon",
        "components::ThemeIcon" => "theme-icon",
        "components::fn close_button" => "close-button",
        "components::CopyButton" => "copy-button",
        "components::Step" | "components::Stepper" => "stepper",
        "components::Accordion" | "components::AccordionOptions" => "accordion",
        "components::AvatarGroup" => "avatar-group",
        "components::Burger" => "burger",
        "components::Card" => "card",
        "components::Blockquote" => "blockquote",
        "components::ColorSwatch" => "color-swatch",
        "components::ExpandingList" => "expanding-list",
        "components::Highlight" => "highlight",
        "components::Indicator" => "indicator",
        "components::KbdGroup" => "kbd-group",
        "components::Loader" => "loader",
        "components::Mark" => "mark",
        "components::Paper" => "paper",
        "components::Pill" => "pill",
        "components::RingProgress" => "ring",
        "components::SegmentedControl" => "segmented-control",
        "components::Text" => "text",
        "components::Title" => "title",
        "components::Timeline" | "components::TimelineItem" => "activity-timeline",
        "inputs::ChipGroup" => "chip-group",
        "inputs::PasswordStrength" => "password-strength",
        "inputs::Dropzone" => "dropzone",
        "inputs::RangeSlider" => "range-slider",
        "inputs::TagsInput" => "tags-input",
        "inputs::InputWrapper" => "input-wrapper",
        "inputs::Picker"
        | "inputs::PickerProps"
        | "inputs::DatePicker"
        | "inputs::DateRangePicker"
        | "inputs::TimePicker"
        | "inputs::DateTimePicker"
        | "inputs::DateTimeRangePicker" => "picker",
        "layout::AspectRatio" => "aspect-ratio",
        "layout::Center" => "center",
        "layout::Container" => "container",
        "layout::Divider" => "divider",
        "layout::Flex" => "flex",
        "layout::Group" => "group",
        "layout::ScrollArea" => "scroll-area",
        "layout::SimpleGrid" => "simple-grid",
        "layout::Space" => "space",
        "layout::Splitter" | "layout::SplitterState" => "splitter",
        "layout::Stack" => "stack",
        "markdown::MarkdownView" | "markdown::MarkdownState" => "markdown",
        "mermaid::Host" | "mermaid::MermaidView" | "mermaid::MermaidViewState" => "mermaid",
        "overlays::Modals" | "overlays::Modal" => "modal",
        "overlays::Drawer" => "drawer",
        "overlays::OverlayHost" => "toast-tray",
        "overlays::Popover" => "popover",
        "overlays::HoverCard" => "hover-card",
        "overlays::Tooltip" => "tooltip",
        "overlays::PopupMenu" => "popup-menu",
        "overlays::ContextMenu" | "overlays::ContextMenuExt" => "context-menu",
        "overlays::DropdownMenu" => "dropdown-menu",
        "overlays::Affix" => "affix",
        "overlays::FloatingTooltip" | "overlays::FloatingOptions" => "floating-tooltip",
        "overlays::LoadingOverlay" => "loading-overlay",
        "overlays::Spotlight" | "overlays::SpotlightState" => "spotlight",
        "tree::TreeView" | "tree::PropertyGrid" | "tree::RowStyle" | "tree::PropertyOptions" => {
            "property-grid"
        }
        "tree::TreeList" => "tree-list",
        "sheet::Sheet" | "sheet::SheetState" => "sheet",
        "timeline::TracksView" => "tracks-view",
        "timeline::GanttView" => "gantt",
        "timeline::DopeSheet" => "dope-sheet",
        "timeline::BarCurveView" => "bar-curve",
        "timeline::CurveView" => "curve-editor",
        "timeline::EventTimeline" | "timeline::EventTimelineState" => "event-timeline",
        "calendar::Calendar" | "calendar::CalendarProps" | "calendar::CalendarState" => "calendar",
        _ => return None,
    })
}

// Exceptions deliberately name the owner and method. A new API must be classified explicitly.
const IMPERATIVE: &[(&str, &str)] = &[
    (
        "commands::RibbonState::activate",
        "Activates an entry by index for pointer hosts; documents use chord keys.",
    ),
    (
        "commands::RibbonState::back",
        "Navigation step driven by Backspace or the Parent button, not a document property.",
    ),
    (
        "commands::RibbonState::root",
        "Navigation step driven by Escape or the Root button, not a document property.",
    ),
    (
        "commands::RibbonState::press",
        "Feeds one chord key; keyboard input, not a document property.",
    ),
    (
        "commands::RibbonState::submit",
        "Fills the prompted parameter; prompt input, not a document property.",
    ),
    (
        "commands::RibbonState::breadcrumbs",
        "Read-only navigation state for hosts and tests.",
    ),
    (
        "commands::RibbonState::depth",
        "Read-only navigation state for hosts and tests.",
    ),
    (
        "commands::RibbonState::message",
        "Read-only refusal text for hosts and tests.",
    ),
    (
        "commands::RibbonState::prompting",
        "Read-only prompt state for hosts and tests.",
    ),
    (
        "commands::RibbonState::model",
        "The compiled layout is built by the host from its registry.",
    ),
    (
        "commands::RibbonState::set_model",
        "The compiled layout is built by the host from its registry.",
    ),
    (
        "overlays::Popover::track_focus",
        "A caller supplies a native FocusHandle. Document triggers use the component default.",
    ),
    (
        "overlays::Tooltip::action",
        "A native GPUI action supplies its shortcut. The explicit key-binding string is the declarative equivalent.",
    ),
    (
        "overlays::PopupMenu::action_context",
        "A native FocusHandle controls how Rust actions dispatch. Documents receive select IDs.",
    ),
    (
        "overlays::PopupMenu::is_empty",
        "Read-only menu state query for native callers.",
    ),
    (
        "layout::AppShellState::toggle_navbar",
        "Imperative toggle of local state. Documents drive navbar-collapsed.",
    ),
    (
        "layout::AppShellState::toggle_aside",
        "Imperative toggle of local state. Documents drive aside-collapsed.",
    ),
    (
        "overlays::OverlayHost::close_all",
        "Cross-document window operation. Documents toggle each owned modal request.",
    ),
    (
        "overlays::OverlayHost::entity",
        "Window-local native state handle used by Rust hosts.",
    ),
    (
        "overlays::OverlayHost::register_context",
        "Requires a native Modal factory. Documents open previously registered factories by context ID.",
    ),
    (
        "overlays::OverlayHost::remove_context",
        "Host-side inverse of registering a native context builder.",
    ),
    (
        "overlays::OverlayHost::stack",
        "Read-only query of the entire native overlay stack.",
    ),
    (
        "overlays::OverlayHost::toasts",
        "Read-only query of the entire native notification queue.",
    ),
    (
        "layout::Grid::layout",
        "Pure placement query for Rust. Documents configure placement records.",
    ),
    (
        "layout::Col::span_at",
        "Pure breakpoint resolution query. Documents configure spans.",
    ),
    (
        "layout::Col::offset_at",
        "Pure breakpoint resolution query. Documents configure offsets.",
    ),
    (
        "components::Spoiler::id",
        "Rust element identity. Documents key the native entity with node identity.",
    ),
    (
        "components::Spoiler::toggle",
        "Imperative inversion of the current value. Documents bind expanded or invoke the native action.",
    ),
    (
        "components::CopyButton::copied",
        "Read-only native copy confirmation.",
    ),
    (
        "components::CopyButton::copy",
        "Imperative clipboard write. Documents use the native action and copy event.",
    ),
    (
        "overlays::SpotlightState::open",
        "Opens the palette in the host window's overlay stack with focus restore. Documents mount the same panel inline.",
    ),
    (
        "overlays::SpotlightState::close",
        "Closes the window overlay opened from Rust. The inline document panel has no overlay to close.",
    ),
    (
        "overlays::SpotlightState::toggle",
        "Toggles the window overlay opened from Rust. The inline document panel has no overlay to toggle.",
    ),
    (
        "overlays::SpotlightState::is_open",
        "Read-only overlay state for Rust hosts.",
    ),
    (
        "overlays::SpotlightState::transition",
        "Overlay open/close animation. The inline document panel does not animate in.",
    ),
    (
        "overlays::SpotlightState::event Opened",
        "Overlay lifecycle event; the inline document panel is never opened as an overlay.",
    ),
    (
        "overlays::SpotlightState::event Closed",
        "Overlay lifecycle event; the inline document panel is never opened as an overlay.",
    ),
    (
        "overlays::SpotlightState::filter",
        "Takes a native ranking closure. Documents use the built-in fuzzy filter.",
    ),
    (
        "overlays::SpotlightState::load",
        "Takes a native future. Documents supply actions and loading directly.",
    ),
    (
        "overlays::SpotlightState::input",
        "Native InputState handle for Rust focus management.",
    ),
    (
        "overlays::SpotlightState::model",
        "Read-only access to the pure row model for Rust tests and hosts.",
    ),
    (
        "overlays::SpotlightState::selected_action",
        "Read-only query. Documents receive selection-change.",
    ),
    (
        "dnd::SortableBoard::is_dragging",
        "Read-only drag state query for native callers.",
    ),
    (
        "dnd::BoardDragOverlay::is_pointer_dragging",
        "Read-only pointer drag state of the native drag surface.",
    ),
    (
        "inputs::Picker::calendar_for_test",
        "Test hook exposing the inner Kit calendar state.",
    ),
    (
        "inputs::Picker::time_fields_for_test",
        "Test hook exposing the inner Kit time field states.",
    ),
    (
        "inputs::Picker::cursor",
        "Read-only keyboard day cursor. Documents receive change.",
    ),
    (
        "inputs::Picker::props",
        "Read-only access to the applied native props.",
    ),
    (
        "calendar::Calendar::event_block_renderer",
        "Takes a native element closure. Documents show each item's title, time, color and icon.",
    ),
    (
        "calendar::Calendar::month_chip_renderer",
        "Takes a native element closure. Documents show each item's title, time, color and icon.",
    ),
    (
        "timeline::EventTimelineState::item_content",
        "Takes a native element closure. Documents show each event's label, content and tags.",
    ),
    (
        "timeline::EventTimelineState::selected",
        "Read-only selection query. Documents receive select.",
    ),
    (
        "overlays::NotificationCenter::event DesktopEvent",
        "Platform notification daemon replies for the application process. Documents show in-window toasts through toast-tray.",
    ),
    (
        "tray::TrayOptions::field badge_color",
        "Process-wide system tray icon registered with the desktop shell, not content of a document view.",
    ),
    (
        "tray::TrayOptions::field double_click",
        "Process-wide system tray icon registered with the desktop shell, not content of a document view.",
    ),
    (
        "tray::TrayOptions::field icon",
        "Process-wide system tray icon registered with the desktop shell, not content of a document view.",
    ),
    (
        "tray::TrayOptions::field id",
        "Process-wide system tray icon registered with the desktop shell, not content of a document view.",
    ),
    (
        "tray::TrayOptions::field menu",
        "Process-wide system tray icon registered with the desktop shell, not content of a document view.",
    ),
    (
        "tray::TrayOptions::field title",
        "Process-wide system tray icon registered with the desktop shell, not content of a document view.",
    ),
    (
        "tray::TrayOptions::field tooltip_body",
        "Process-wide system tray icon registered with the desktop shell, not content of a document view.",
    ),
    (
        "tray::TrayOptions::field tooltip_title",
        "Process-wide system tray icon registered with the desktop shell, not content of a document view.",
    ),
    (
        "tray::TrayService::event Click",
        "Process-wide system tray icon events from the desktop shell, not content of a document view.",
    ),
    (
        "tray::TrayService::event DoubleClick",
        "Process-wide system tray icon events from the desktop shell, not content of a document view.",
    ),
    (
        "tray::TrayService::event Error",
        "Process-wide system tray icon events from the desktop shell, not content of a document view.",
    ),
    (
        "tray::TrayService::event HostAvailable",
        "Process-wide system tray icon events from the desktop shell, not content of a document view.",
    ),
    (
        "tray::TrayService::event MenuItem",
        "Process-wide system tray icon events from the desktop shell, not content of a document view.",
    ),
    (
        "tray::TrayService::event MiddleClick",
        "Process-wide system tray icon events from the desktop shell, not content of a document view.",
    ),
    (
        "tray::TrayService::event Ready",
        "Process-wide system tray icon events from the desktop shell, not content of a document view.",
    ),
    (
        "tray::TrayService::event Scroll",
        "Process-wide system tray icon events from the desktop shell, not content of a document view.",
    ),
    (
        "windows::NotificationCard",
        "Content of a desktop notification window, mounted by the notification stack outside any document.",
    ),
    (
        "windows::NotificationCard::height",
        "Content of a desktop notification window, mounted by the notification stack outside any document.",
    ),
    (
        "windows::NotificationCard::new",
        "Content of a desktop notification window, mounted by the notification stack outside any document.",
    ),
    (
        "windows::NotificationCard::on_action",
        "Content of a desktop notification window, mounted by the notification stack outside any document.",
    ),
    (
        "windows::NotificationCard::on_click",
        "Content of a desktop notification window, mounted by the notification stack outside any document.",
    ),
    (
        "windows::NotificationCard::on_close",
        "Content of a desktop notification window, mounted by the notification stack outside any document.",
    ),
    (
        "windows::NotificationCard::test_id",
        "Content of a desktop notification window, mounted by the notification stack outside any document.",
    ),
    (
        "windows::NotificationStack::event WindowNotificationEvent",
        "Events of the application's desktop notification windows, outside any document.",
    ),
    (
        "windows::StackView",
        "Root view of a desktop notification window, outside any document.",
    ),
    (
        "core::TextMenuLayer",
        "Window-owned menu layer shared by native text controls.",
    ),
    (
        "devtools::DevtoolsPanel",
        "Feature-gated diagnostics for the application's own window and instrumented view registry.",
    ),
    (
        "devtools::DevtoolsRoot",
        "Window host infrastructure that mounts diagnostic content around an application view.",
    ),
    (
        "markdown::MarkdownState::reveal_heading",
        "Host command that takes a parsed Heading returned by the Rust view state.",
    ),
    (
        "mermaid::Host::request_redraw",
        "The document renderer responds to source and theme changes automatically.",
    ),
    (
        "mermaid::MermaidViewState::fit",
        "Native viewport action available from the diagram controls and double click.",
    ),
    (
        "mermaid::MermaidViewState::reset_zoom",
        "Native viewport action available from the diagram controls.",
    ),
    (
        "mermaid::MermaidViewState::zoom_in",
        "Native zoom action exposed by the diagram controls.",
    ),
    (
        "mermaid::MermaidViewState::zoom_out",
        "Native zoom action exposed by the diagram controls.",
    ),
    (
        "ui::BoundNode",
        "An internal view allocated for a document node that reads a binding.",
    ),
    (
        "ui::DataUi",
        "The Rust host owns a complete compiled document and its application logic.",
    ),
    (
        "ui::DataUi::events_handled",
        "Read-only host event counter for instrumentation.",
    ),
    (
        "ui::DataUi::from_template",
        "Rust constructor for a host that compiles or shares a template before mounting it.",
    ),
    (
        "ui::DataUi::handle",
        "Returns the Rust integration handle for patching a mounted document.",
    ),
    (
        "ui::DataUi::host_renders",
        "Read-only host render counter for instrumentation.",
    ),
    (
        "ui::DataUi::new",
        "Rust constructor joining document, application registry and application logic.",
    ),
    (
        "ui::DataUi::node_renders",
        "Read-only node render counter for instrumentation.",
    ),
    (
        "ui::DataUi::register",
        "Rust host registration on the action bus; document events become control actions.",
    ),
    (
        "ui::DataUi::set_document",
        "Rust hot-reload operation that swaps the document mounted by an existing host.",
    ),
    (
        "ui::DataUi::slot_value",
        "Read the desired data through a document binding.",
    ),
    (
        "ui::DataUi::stale_events",
        "Read-only host counter for events rejected across document generations.",
    ),
    (
        "ui::DataUi::template",
        "Returns the Rust host's compiled and shared template.",
    ),
    (
        "components::AvatarGroup::id",
        "Set the document node id to identify the group.",
    ),
    (
        "components::Burger::id",
        "Set the document node id to identify the navigation toggle.",
    ),
    (
        "components::Burger::toggle",
        "Interaction command. Documents supply the value and receive a change event.",
    ),
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
        "markdown::MarkdownState::append",
        "Streaming producers append to the bound source string, which updates the native view.",
    ),
    (
        "markdown::MarkdownState::context_menu",
        "Returns the native host menu assembled for the current text selection.",
    ),
    (
        "markdown::MarkdownState::document",
        "Returns the retained parsed document to Rust callers integrating analysis.",
    ),
    (
        "markdown::MarkdownState::extensions",
        "Returns the fixed parser extension set used by the viewer.",
    ),
    (
        "markdown::MarkdownState::headings",
        "Read-only extraction from the retained Markdown source.",
    ),
    (
        "markdown::MarkdownState::shown_len",
        "Returns the internal display buffer length for streaming instrumentation.",
    ),
    (
        "markdown::MarkdownState::stats",
        "Read-only renderer instrumentation.",
    ),
    (
        "markdown::MarkdownState::text",
        "Returns the internal native text entity for Rust hosts.",
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
        "mermaid::MermaidViewState::status",
        "Read-only status of the asynchronous diagram renderer.",
    ),
    (
        "mermaid::MermaidViewState::has_preview",
        "Reports an internal distinction between the raster preview and the final diagram.",
    ),
    (
        "mermaid::MermaidViewState::image",
        "Returns the native image object from the last successful render.",
    ),
    (
        "mermaid::MermaidViewState::svg",
        "Read-only export of the last successful render.",
    ),
    (
        "mermaid::MermaidViewState::viewport",
        "Returns the current viewport for Rust hosts that build a separate toolbar.",
    ),
    (
        "timeline::GanttView::axis",
        "Returns the native viewport axis computed by the schedule view.",
    ),
    (
        "timeline::GanttView::schedule",
        "Returns the derived schedule or graph errors for Rust hosts.",
    ),
    (
        "timeline::GanttView::is_locked",
        "Read the controlled locked ids from the document input.",
    ),
    (
        "timeline::GanttView::grab",
        "Native hit testing converts pointer coordinates into an editing gesture.",
    ),
    (
        "timeline::GanttView::press",
        "Starts an input gesture from native pointer coordinates.",
    ),
    (
        "timeline::GanttView::drag_to",
        "Continues an active input gesture from native pointer coordinates.",
    ),
    (
        "timeline::GanttView::release",
        "Finishes the active native input gesture.",
    ),
    (
        "timeline::GanttView::nudge",
        "An imperative keyboard action. Documents supply key-step and task input.",
    ),
    (
        "timeline::GanttView::reschedule",
        "A Rust edit command. Update task inputs to drive scheduling and bind the reschedule event to receive user edits.",
    ),
    (
        "timeline::GanttView::resize_edge",
        "A Rust edit command. Update task duration directly or bind the resize event to receive user edits.",
    ),
    (
        "timeline::DopeSheet::axis",
        "Returns the native viewport axis. Documents supply the initial length and time format.",
    ),
    (
        "timeline::DopeSheet::model",
        "Returns the Rust view model and derived row and selection state.",
    ),
    (
        "timeline::DopeSheet::select_all",
        "Native editor command. The selection-change event exposes updates to application logic.",
    ),
    (
        "timeline::DopeSheet::delete_selected",
        "Native editor command. Documents supply channel keyframes and receive change events.",
    ),
    (
        "timeline::DopeSheet::duplicate_selected",
        "Native editor command. Documents supply channel keyframes and receive change events.",
    ),
    (
        "timeline::DopeSheet::nudge",
        "Native editor command. Documents can bind the changed channel keys.",
    ),
    (
        "timeline::DopeSheet::press",
        "Starts a native pointer gesture against one rendered row.",
    ),
    (
        "timeline::DopeSheet::press_with",
        "Starts a native pointer gesture with the current selection modifier.",
    ),
    (
        "timeline::DopeSheet::drag_to",
        "Continues the active native input gesture.",
    ),
    (
        "timeline::DopeSheet::release",
        "Finishes the active native input gesture.",
    ),
    (
        "timeline::BarCurveView::draw_to",
        "Native pointer gesture. Documents supply bar values and receive indexed changes.",
    ),
    (
        "timeline::BarCurveView::release",
        "Finishes the active native input gesture.",
    ),
    (
        "timeline::CurveView::axis",
        "Returns the current viewport axis. The document supplies time-start and time-end.",
    ),
    (
        "timeline::CurveView::range",
        "Returns the current viewport range. The document supplies minimum and maximum.",
    ),
    (
        "timeline::CurveView::insert_at",
        "Imperative editor command that interpolates a new key. Documents supply the desired key data.",
    ),
    (
        "timeline::CurveView::remove_selected",
        "Imperative editor command. A document can replace its curve key list.",
    ),
    (
        "timeline::CurveView::split_segment",
        "Imperative edit preserving curve continuity. A document supplies complete keys and handles.",
    ),
    (
        "timeline::CurveView::remove_segment",
        "Imperative edit. A document can replace its curve key list.",
    ),
    (
        "timeline::CurveView::press",
        "Begins a native pointer gesture from editor coordinates.",
    ),
    (
        "timeline::CurveView::drag_to",
        "Continues the native pointer gesture.",
    ),
    (
        "timeline::CurveView::release",
        "Finishes the native pointer gesture.",
    ),
    (
        "tree::TreeView::forget_removed",
        "The data owner reports permanent deletion. DataUi reconciles removals when it replaces document rows.",
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
    (
        "layout::ScrollArea::track_scroll",
        "Host-owned native scroll handle. DataUi keeps scroll state by node identity.",
    ),
    (
        "layout::SimpleGrid::container_width",
        "Host-owned measurement entity. DataUi uses its grid's measured width.",
    ),
    (
        "layout::SimpleGrid::cols_at",
        "Read-only evaluation of column count at a supplied width.",
    ),
    (
        "layout::SimpleGrid::spacing_at",
        "Read-only evaluation of spacing at a supplied width.",
    ),
    (
        "layout::SimpleGrid::vertical_spacing_at",
        "Read-only evaluation of row spacing at a supplied width.",
    ),
    (
        "flamegraph::FlameGraph::is_loading",
        "Read-only state of the background indexing job.",
    ),
    (
        "flamegraph::FlameGraph::load_failure",
        "Read-only issue detail shown by the widget.",
    ),
    (
        "flamegraph::FlameGraph::full_plot_height",
        "Computed geometry for Rust hosts.",
    ),
    (
        "flamegraph::FlameGraph::reset",
        "The graph provides an interactive viewport reset command.",
    ),
    (
        "timeline::TracksView::add_layer",
        "A native plot-layer trait implementation is application-owned Rust code.",
    ),
    (
        "timeline::BarCurveView::add_layer",
        "A native plot-layer trait implementation is application-owned Rust code.",
    ),
    (
        "timeline::CurveView::add_layer",
        "A native plot-layer trait implementation is application-owned Rust code.",
    ),
    (
        "timeline::DopeSheet::add_layer",
        "A native plot-layer trait implementation is application-owned Rust code.",
    ),
    (
        "timeline::GanttView::add_layer",
        "A native plot-layer trait implementation is application-owned Rust code.",
    ),
    (
        "timeline::TracksView::axis",
        "Computed axis geometry for Rust callers.",
    ),
    (
        "timeline::TracksView::drag_to",
        "The rendered clip handles pointer dragging.",
    ),
    (
        "timeline::TracksView::grab",
        "The view computes its own hit target during input.",
    ),
    (
        "timeline::TracksView::is_locked",
        "The clip record supplies its lock state.",
    ),
    (
        "timeline::TracksView::nudge",
        "The timeline provides its own keyboard and pointer editing.",
    ),
    (
        "timeline::TracksView::press",
        "The rendered clip handles pointer interaction.",
    ),
    (
        "timeline::TracksView::release",
        "The rendered clip handles pointer interaction.",
    ),
    (
        "dnd::SortableBoard::drag_overlay",
        "A Rust host can mount an explicit overlay entity. Document boards mount their own drag layer.",
    ),
    (
        "dnd::SortableBoard::lists",
        "Read ordered card ids from the reorder event.",
    ),
    (
        "charts::ChartView::canvas_paint_count",
        "Diagnostic counter for native tests.",
    ),
    (
        "charts::ChartView::hovered",
        "Read-only state of the interactive pointer.",
    ),
    (
        "charts::ChartView::replace",
        "Accepts an application-owned prepared buffer. A document changes layers, axes, or labels.",
    ),
    (
        "charts::ChartView::scene_build_count",
        "Diagnostic counter for native tests.",
    ),
    (
        "charts::ChartView::series_visible",
        "Read the current state through series-visibility events.",
    ),
    (
        "charts::ChartView::toggle_series",
        "The interactive legend controls visibility.",
    ),
    (
        "charts::ChartView::viewport",
        "Read the current bounds through viewport-change events.",
    ),
    (
        "devtools::fn root",
        "Window-root instrumentation is installed by the application.",
    ),
    (
        "ui::fn cached",
        "Internal ui! cache helper. The document host owns its render cache.",
    ),
    (
        "ui::fn keyed",
        "Internal ui! repeater helper. DataUi uses the document key field.",
    ),
    (
        "chat::MessageView",
        "Private history row type controlled by the chat adapter.",
    ),
    (
        "chat::ChatState::cancel_edit",
        "Native composer command performed via its Cancel control.",
    ),
    (
        "chat::ChatState::composer",
        "Native composer entity owned by ChatState.",
    ),
    (
        "chat::ChatState::edit_last_user",
        "Native chat command performed on the last user message.",
    ),
    (
        "chat::ChatState::is_streaming",
        "Read-only state with a document streaming-message binding.",
    ),
    (
        "chat::ChatState::jump_to_latest",
        "Interactive scroll command owned by the chat view.",
    ),
    (
        "chat::ChatState::message",
        "Read provider-owned messages through the messages binding.",
    ),
    (
        "chat::ChatState::message_count",
        "Read the current transcript length from the messages binding.",
    ),
    (
        "chat::ChatState::stop",
        "Native Stop control sends the stop event.",
    ),
    (
        "chat::ChatState::submit",
        "Native Send control sends the submit event.",
    ),
    (
        "chat::Composer::input",
        "Native textarea entity. The document drives the draft string.",
    ),
    (
        "chat::Composer::is_busy",
        "Read-only native state derived from streaming-message.",
    ),
    (
        "flow::FlowState::cancel_layout",
        "Host-side layout task cancellation.",
    ),
    (
        "flow::FlowState::connection_validator",
        "A Rust callback is required for custom validation; use the declarative connection policy for structural constraints.",
    ),
    (
        "flow::FlowState::edit",
        "Native API for undoable edits; documents receive the changed graph through change.",
    ),
    (
        "flow::FlowState::fit",
        "The user can fit the graph through the native viewport controls.",
    ),
    (
        "flow::FlowState::gesture_entity",
        "Internal native pointer gesture layer.",
    ),
    (
        "flow::FlowState::last_error",
        "The renderer reports errors through the error event.",
    ),
    (
        "flow::FlowState::node_renderer",
        "Application-owned custom view factory. The document renders the default node content.",
    ),
    (
        "flow::FlowState::paint_timings",
        "Instrumentation for native rendering tests.",
    ),
    (
        "flow::FlowState::redo",
        "Native undo history can be redone through viewport keyboard shortcuts.",
    ),
    (
        "flow::FlowState::selected_edges",
        "Receive selected edge IDs through selection-change.",
    ),
    (
        "flow::FlowState::selected_nodes",
        "Receive selected node IDs through selection-change.",
    ),
    (
        "flow::FlowState::size",
        "Computed viewport geometry for the Rust host.",
    ),
    (
        "flow::FlowState::undo",
        "Native history can be undone through viewport keyboard shortcuts.",
    ),
    (
        "flow::FlowState::viewport",
        "The viewport-change event carries camera values.",
    ),
    (
        "flow::FlowState::viewport_entity",
        "Internal native viewport entity.",
    ),
    (
        "flow::FlowState::visible_counts",
        "Instrumentation for native rendering tests.",
    ),
    (
        "flow::FlowViewport::connection_validator",
        "A Rust callback is required for custom validation; use the declarative connection policy for structural constraints.",
    ),
    (
        "flow::FlowViewport::edit",
        "Native API for undoable edits; documents receive the changed graph through change.",
    ),
    (
        "flow::FlowViewport::fit",
        "The user can fit the graph through the native viewport controls.",
    ),
    (
        "flow::FlowViewport::last_error",
        "The renderer reports errors through the error event.",
    ),
    (
        "flow::FlowViewport::node_renderer",
        "Application-owned custom view factory. The document renders the default node content.",
    ),
    (
        "flow::FlowViewport::paint_timings",
        "Instrumentation for native rendering tests.",
    ),
    (
        "flow::FlowViewport::redo",
        "Native undo history can be redone through viewport keyboard shortcuts.",
    ),
    (
        "flow::FlowViewport::selected_edges",
        "Receive selected edge IDs through selection-change.",
    ),
    (
        "flow::FlowViewport::selected_nodes",
        "Receive selected node IDs through selection-change.",
    ),
    (
        "flow::FlowViewport::undo",
        "Native history can be undone through viewport keyboard shortcuts.",
    ),
    (
        "flow::FlowViewport::viewport",
        "The viewport-change event carries camera values.",
    ),
    (
        "flow::FlowViewport::visible_counts",
        "Instrumentation for native rendering tests.",
    ),
    (
        "flow::FlowGesture",
        "Native pointer gesture layer mounted by the flow canvas.",
    ),
    (
        "sheet::SheetState::with_engine",
        "A Rust FormulaEngine trait object replaces the built-in engine. Documents use the built-in formulas.",
    ),
    (
        "sheet::SheetState::model",
        "Read-only model query for native callers. Documents receive changed cells through edit.",
    ),
    (
        "sheet::SheetState::calculation",
        "Read-only query for the finished background calculation, for native callers. Documents receive changed cells through edit.",
    ),
    (
        "dnd::DockDragOverlay",
        "Native drag surface mounted by a managed dock.",
    ),
    (
        "dnd::BoardDragOverlay",
        "Native drag surface mounted by a sortable board.",
    ),
];

fn covered(surface: &str) -> bool {
    if IMPERATIVE
        .iter()
        .any(|(api, reason)| *api == surface && !reason.is_empty())
    {
        return true;
    }
    if let Some((kind, prop)) = match surface {
        "overlays::OverlayHost::close" | "overlays::OverlayHost::open" => Some(("modal", "open")),
        "overlays::OverlayHost::confirm" => Some(("modal", "ok-label")),
        "overlays::OverlayHost::open_context" => Some(("modal", "context")),
        "overlays::OverlayHost::clear_toasts"
        | "overlays::OverlayHost::notify"
        | "overlays::OverlayHost::dismiss_toast" => Some(("toast-tray", "toasts")),
        _ => None,
    } {
        return guiraffe_ui::catalog::lookup(kind)
            .is_some_and(|spec| spec.props.iter().any(|property| property.name == prop));
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
    if let Some(event) = method.strip_prefix("event ") {
        let id = match (owner, event) {
            ("tree::TreeView", "LoadChildren") => "load-children",
            ("tree::TreeView", "PickPath") => "pick-path",
            ("tree::TreeView", "Checked") => "check",
            ("tree::TreeView", "Reorder") | ("dnd::SortableBoard", "SortEvent") => "reorder",
            ("charts::ChartView", "Brushed") => "brush",
            ("charts::ChartView", "SeriesVisibility") => "series-visibility",
            ("inputs::Dropzone", "Accepted") => "accept",
            ("inputs::Dropzone", "BrowseRequested") => "browse",
            ("flamegraph::FlameGraph", "FrameClicked") => "frame-click",
            ("components::Accordion", "AccordionChange")
            | ("components::ExpandingList", "ExpandingListChange") => "change",
            ("timeline::TracksView", "ClipChanged") => "clip-change",
            ("timeline::TracksView", "Seek") => "seek",
            ("timeline::GanttView", "Rescheduled") => "reschedule",
            ("timeline::GanttView", "Resized") => "resize",
            ("timeline::DopeSheet", "KeyRetimed") => "key-retime",
            ("timeline::DopeSheet", "ChannelChanged") => "channel-change",
            ("components::SpeedDial", "Open") => "change",
            ("components::SpeedDial", "Action") => "action",
            ("layout::AppShellState", "NavbarCollapsed") => "navbar-collapsed",
            ("layout::AppShellState", "AsideCollapsed") => "aside-collapsed",
            ("layout::AppShellState", "NavbarResized") => "navbar-resized",
            ("layout::AppShellState", "AsideResized") => "aside-resized",
            ("layout::SplitterState", "Resized") => "resize",
            ("timeline::BarCurveView", "BarEvent") => "change",
            ("timeline::CurveView", "KeyMoved") => "key-move",
            ("chat::ChatState", "Submitted") => "submit",
            ("chat::ChatState", "Retry") => "retry",
            ("chat::ChatState", "Stopped") => "stop",
            ("chat::ChatState", "AttachmentRequested") => "attachment-request",
            ("chat::ChatState", "Finished") => "finish",
            ("overlays::SpotlightState", "Activated") => "activate",
            ("commands::RibbonState", "Opened") => "opened",
            ("commands::RibbonState", "Closed") => "closed",
            ("commands::RibbonState", "Ran") => "ran",
            ("commands::RibbonState", "Refused") => "refused",
            ("timeline::EventTimelineState", "Activated") => "activate",
            ("inputs::Picker", "Change") => "change",
            ("inputs::Picker", "OpenChange") => "open-change",
            ("overlays::SpotlightState", "QueryChanged") => "query-change",
            ("datagrid::GridEvents", "SortChanged") => "sort-change",
            ("datagrid::GridEvents", "CellEdited") => "cell-edit",
            ("datagrid::GridEvents", "EditCancelled") => "edit-cancel",
            ("datagrid::GridEvents", "Copied") => "copy",
            ("datagrid::GridEvents", "EditRejected") => "edit-reject",
            ("datagrid::GridEvents", "RowActivated") => "row-activate",
            ("datagrid::GridEvents", "ColumnResized") => "column-resize",
            ("datagrid::GridEvents", "ColumnReordered") => "column-reorder",
            ("datagrid::GridEvents", "CellsChanged") => "cells-change",
            ("datagrid::GridEvents", "CellsRejected") => "cells-reject",
            ("calendar::CalendarState", "Announcement") => "announce",
            ("calendar::CalendarState", "DateSelected") => "date-select",
            ("calendar::CalendarState", "EditRequested") => "edit-request",
            ("calendar::CalendarState", "ItemSelected") => "item-select",
            ("calendar::CalendarState", "Navigated") => "navigate",
            ("calendar::CalendarState", "OverflowRequested") => "overflow",
            ("calendar::CalendarState", "SourceVisibilityChanged") => "source-visibility",
            ("calendar::CalendarState", "ViewChanged") => "view-change",
            (_, "Changed") => "change",
            (_, "Error") => "error",
            (_, "Edited") => "edit",
            (_, "Rejected") => "reject",
            (_, "Selected") => "select",
            (_, "SelectionChanged") => "selection-change",
            (_, "ViewportChanged") => "viewport-change",
            _ => return false,
        };
        return spec.events.iter().any(|event| event.id == id);
    }
    if let Some(field) = method.strip_prefix("field ") {
        let name = match (owner, field) {
            ("charts::ChartSpec", "x" | "y" | "right") => "axes".into(),
            (_, field) => field.replace('_', "-"),
        };
        if owner == "code::DecorationLayerOptions" {
            let name = match field {
                "merge_identical" => "mergeIdentical",
                "z_index" => "zIndex",
                _ => return false,
            };
            return spec
                .props
                .iter()
                .find(|prop| prop.name == "decoration-layers")
                .is_some_and(|prop| prop.schema()["items"]["properties"].get(name).is_some());
        }
        if owner == "flow-core::LayoutOptions" {
            let name = if field == "crossing_weight" {
                "crossingWeight"
            } else {
                field
            };
            return spec
                .props
                .iter()
                .find(|prop| prop.name == "layout")
                .is_some_and(|prop| prop.schema()["properties"].get(name).is_some());
        }
        if owner == "tree::PropertyOptions" {
            let name = match field {
                "multiline_rows" => "multilineRows",
                "reset_button" => "resetButton",
                _ => field,
            };
            return spec
                .props
                .iter()
                .find(|prop| prop.name == "nodes")
                .is_some_and(|nodes| {
                    nodes.schema()["items"]["properties"]["property"]["properties"]
                        .get(name)
                        .is_some()
                });
        }
        return spec.props.iter().any(|prop| prop.name == name);
    }
    if owner == "components::TimelineItem" && matches!(method, "description" | "color" | "bullet") {
        return spec
            .props
            .iter()
            .find(|prop| prop.name == "items")
            .is_some_and(|prop| prop.schema()["items"]["properties"].get(method).is_some());
    }
    if owner == "components::Step" && matches!(method, "description" | "icon") {
        return spec
            .props
            .iter()
            .find(|prop| prop.name == "steps")
            .is_some_and(|prop| prop.schema()["items"]["properties"].get(method).is_some());
    }
    if owner == "layout::Col" && matches!(method, "span" | "offset") {
        return spec
            .props
            .iter()
            .find(|prop| prop.name == "placements")
            .is_some_and(|prop| prop.schema()["items"]["properties"].get(method).is_some());
    }
    if method == "content" && matches!(owner, "overlays::Popover" | "overlays::HoverCard") {
        return spec.accepts_children;
    }
    match method {
        "first" | "second" | "pane" | "panes" if owner == "layout::Splitter" => {
            spec.accepts_children
        }
        "new" => true, // Constructor parameters are declared by the kind's descriptor.
        "child" | "children" => spec.accepts_children,
        "more" if owner == "components::AvatarGroup" => spec.accepts_children,
        _ => {
            let property = match (owner, method) {
                ("dnd::SortableBoard", "with_grid") => "cards-per-row".into(),
                ("flamegraph::FlameGraph", "from_source") => "frames".into(),
                ("timeline::TracksView", "seek") => "playhead".into(),
                ("timeline::TracksView", "select") => "selected".into(),
                ("timeline::TracksView", "set_locked") => "tracks".into(),
                ("timeline::GanttView", "select") => "selected".into(),
                ("timeline::DopeSheet", "set_locked") => "channels".into(),
                ("timeline::BarCurveView", "model" | "set_model") => "values".into(),
                (
                    "timeline::CurveView",
                    "curve" | "add_curve" | "add_key" | "set_curve_visible" | "set_interpolation"
                    | "set_tangent",
                ) => "curves".into(),
                ("timeline::CurveView", "select_key") => "selection".into(),
                ("timeline::CurveView", "fit_view") => "fit".into(),
                ("chat::ChatState", "begin_reply") => "streaming-message".into(),
                ("chat::ChatState", "add_attachment") => "attachments".into(),
                ("chat::ChatState", "with_props" | "props" | "set_props") => "title".into(),
                ("chat::ChatState", "push_message" | "set_reasoning" | "update_tool") => {
                    "messages".into()
                }
                ("chat::Composer", "set_completions") => "commands".into(),
                ("charts::ChartView", "from_spec" | "replace_spec") => "layers".into(),
                ("flow::FlowState" | "flow::FlowViewport", "options") => "background".into(),
                ("flow::FlowState" | "flow::FlowViewport", "connection_policy") => {
                    "allow-self".into()
                }
                ("flow::FlowState" | "flow::FlowViewport", "set_viewport") => "zoom".into(),
                ("flow::FlowState", "from_editor") => "graph".into(),
                ("components::Card" | "components::Paper", "p") => "padding".into(),
                ("components::Card", "section") => "section-borders".into(),
                ("components::ScopedButton", "focusable") => "tab-stop".into(),
                ("components::SpeedDial", "is_open") => "open".into(),
                ("components::Spoiler", "is_expanded") => "expanded".into(),
                ("components::Spoiler", "set_content") => "text".into(),
                ("components::Spoiler", "set_timing") => "duration-ms".into(),
                ("layout::Grid", "container_width") => "width-source".into(),
                ("layout::AppShell", "header" | "navbar" | "main" | "aside" | "footer") => {
                    "slots".into()
                }
                ("flow::FlowState", "arrange") => "layout".into(),
                ("overlays::Popover" | "overlays::HoverCard", "trigger") => "trigger-label".into(),
                ("overlays::Popover" | "overlays::HoverCard", "trigger_style") => {
                    "trigger-full-width".into()
                }
                ("overlays::Popover" | "overlays::HoverCard", "on_open_change") => "change".into(),
                ("overlays::HoverCard", "open_delay") => "open-delay-ms".into(),
                ("overlays::HoverCard", "close_delay") => "close-delay-ms".into(),
                ("overlays::Tooltip", "element" | "build") => "text".into(),
                ("overlays::ContextMenuExt", "context_menu") => "items".into(),
                ("overlays::DropdownMenu", "dropdown_menu") => "items".into(),
                ("overlays::DropdownMenu", "dropdown_menu_with_anchor") => "anchor".into(),
                ("overlays::PopupMenu", "min_w") => "min-width".into(),
                ("overlays::PopupMenu", "max_w") => "max-width".into(),
                ("overlays::PopupMenu", "max_h") => "max-height".into(),
                (
                    "overlays::PopupMenu",
                    "build"
                    | "rebuild"
                    | "item"
                    | "label"
                    | "separator"
                    | "submenu"
                    | "submenu_with_icon"
                    | "link"
                    | "link_with_disabled"
                    | "link_with_icon"
                    | "menu"
                    | "menu_with_enable"
                    | "menu_with_disabled"
                    | "menu_with_icon"
                    | "menu_with_icon_and_disabled"
                    | "menu_with_check"
                    | "menu_with_check_and_disabled"
                    | "menu_element"
                    | "menu_element_with_disabled"
                    | "menu_element_with_icon"
                    | "menu_element_with_check",
                ) => "items".into(),
                ("overlays::Modal", "content") => "message".into(),
                ("overlays::Modal", "footer") => "ok-label".into(),
                ("overlays::Modal", "w") => "width".into(),
                ("layout::AppShellState", "collapse_navbar" | "expand_navbar") => {
                    "navbar-collapsed".into()
                }
                ("commands::Menu", "props") => "show-categories".into(),
                ("commands::RibbonState", "props") => "rows-per-column".into(),
                ("commands::RibbonState", "open" | "close" | "toggle" | "is_open") => "open".into(),
                ("components::ActionIcon" | "components::ThemeIcon", "from_asset") => "icon".into(),
                ("layout::Splitter", "pane_size") => "pane-sizes".into(),
                ("components::Loader", "set_period") => "period-ms".into(),
                ("components::Accordion", "open" | "set_controlled") => "value".into(),
                ("components::Accordion", "set_options") => "variant".into(),
                ("components::ExpandingList", "open" | "set_open") => "value".into(),
                ("components::Burger", "is_open" | "set_open" | "set_open_instant") => {
                    "value".into()
                }
                ("components::Burger", "set_labels") => "closed-label".into(),
                ("components::SegmentedControl", "select" | "selected") => "value".into(),
                ("components::ColorSwatch", "from_color") => "color".into(),
                ("components::Paper", "p_0") => "no-padding".into(),
                ("components::Text", "fw") => "font-weight".into(),
                ("components::RingProgress", "section") => "sections".into(),
                ("layout::Divider", "vertical") => "orientation".into(),
                ("layout::ScrollArea", "both") => "scroll-x".into(),
                ("layout::Space", "h") => "height".into(),
                ("layout::Space", "w") => "width".into(),
                ("overlays::FloatingTooltip", "options") => "side".into(),
                ("tree::TreeView", "inspector_controls") => "searchable".into(),
                ("overlays::SpotlightState", "group") => "actions".into(),
                ("timeline::EventTimelineState", "replace") => "events".into(),
                ("calendar::Calendar", "props") => "show-toolbar".into(),
                ("inputs::Picker", "apply_preset") => "presets".into(),
                ("inputs::Picker", "clear") => "value".into(),
                ("inputs::Picker", "is_open") => "open".into(),
                ("inputs::Picker", "with_props" | "update_props") => "label".into(),
                ("overlays::SpotlightState", "recent_ids") => "recents".into(),
                ("tree::TreeView", "row_style" | "set_row_style") => "density".into(),
                ("sheet::SheetState", "freeze" | "frozen") => "frozen-rows".into(),
                ("sheet::SheetState", "edit") => "cells".into(),
                ("layout::Stack", "animate_layout_with") => "animate-layout".into(),
                ("tree::TreeView", "row_heights" | "set_row_heights" | "replace_source") => {
                    "nodes".into()
                }
                (_, name) => name.strip_prefix("set_").unwrap_or(name).replace('_', "-"),
            };
            let event = method
                .strip_prefix("on_")
                .map(|name| name.replace('_', "-"));
            spec.props.iter().any(|p| p.name == property)
                || spec.events.iter().any(|e| {
                    event.as_deref() == Some(e.id) || method.starts_with("on_") && property == e.id
                })
        }
    }
}

#[test]
fn public_document_surface_does_not_regress() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let public = source::public_surface(&crates);
    for (api, reason) in IMPERATIVE {
        assert!(
            public.contains(*api),
            "stale imperative exception for {api}"
        );
        assert!(
            !reason.trim().is_empty(),
            "document the exception for {api}"
        );
    }

    let uncovered: BTreeSet<_> = public
        .iter()
        .filter(|api| !covered(api))
        .map(String::as_str)
        .collect();
    assert!(
        uncovered.is_empty(),
        "Unclassified public DataUi APIs: {uncovered:#?}"
    );
}
