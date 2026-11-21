//! Tracks additions to public document authoring surfaces across component crates.

#[path = "catalogue_drift/source.rs"]
mod source;

use std::{collections::BTreeSet, path::Path};

fn kind(surface: &str) -> Option<&'static str> {
    Some(match surface {
        "chat::Chat" | "chat::ChatState" | "chat::Composer" | "chat::ChatProps" => "chat",
        "charts::ChartView" | "charts::ChartOptions" | "charts::ChartSpec" => "chart",
        "code::CodeViewer" | "code::CodeOptions" => "code-viewer",
        "datagrid::DataGrid" | "datagrid::GridOptions" => "data-grid",
        "dnd::SortableBoard" => "sortable-board",
        "dnd::fn new_dock" | "dnd::fn new_native_dock" => "dock",
        "flamegraph::FlameGraph" => "flame-graph",
        "flow::Flow" | "flow::FlowCanvas" | "flow::FlowState" | "flow::FlowViewport"
        | "flow::FlowOptions" => "flow",
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
        "layout::Container" => "container",
        "layout::Divider" => "divider",
        "layout::Flex" => "flex",
        "layout::Group" => "group",
        "layout::ScrollArea" => "scroll-area",
        "layout::SimpleGrid" => "simple-grid",
        "layout::Space" => "space",
        "layout::Stack" => "stack",
        "markdown::MarkdownView" | "markdown::MarkdownState" => "markdown",
        "mermaid::MermaidView" | "mermaid::MermaidViewState" => "mermaid",
        "overlays::Affix" => "affix",
        "overlays::FloatingTooltip" | "overlays::FloatingOptions" => "floating-tooltip",
        "overlays::LoadingOverlay" => "loading-overlay",
        "tree::TreeView" | "tree::PropertyGrid" | "tree::RowStyle" | "tree::PropertyOptions" => {
            "property-grid"
        }
        "tree::TreeList" => "tree-list",
        "timeline::TracksView" => "tracks-view",
        "timeline::GanttView" => "gantt",
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
        "flow::FlowState::arrange",
        "A host-owned layout engine is a Rust trait implementation; documents provide graph positions.",
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
    if let Some(kind) = kind(surface) {
        return guiraffe_ui::catalog::lookup(kind).is_some();
    }
    let Some((owner, method)) = surface.rsplit_once("::") else {
        return false;
    };
    let Some(spec) = kind(owner).and_then(guiraffe_ui::catalog::lookup) else {
        return false;
    };
    if let Some(field) = method.strip_prefix("field ") {
        let name = match (owner, field) {
            ("charts::ChartSpec", "x" | "y" | "right") => "axes".into(),
            (_, field) => field.replace('_', "-"),
        };
        if owner == "tree::PropertyOptions" {
            let name = if field == "multiline_rows" {
                "multilineRows"
            } else {
                field
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
    match method {
        "new" => true, // Constructor parameters are declared by the kind's descriptor.
        "child" | "children" => spec.accepts_children,
        _ => {
            let property = match (owner, method) {
                ("dnd::SortableBoard", "with_grid") => "cards-per-row".into(),
                ("flamegraph::FlameGraph", "from_source") => "frames".into(),
                ("timeline::TracksView", "seek") => "playhead".into(),
                ("timeline::TracksView", "select") => "selected".into(),
                ("timeline::TracksView", "set_locked") => "tracks".into(),
                ("timeline::GanttView", "select") => "selected".into(),
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
                ("components::Card", "p") => "padding".into(),
                ("components::Text", "fw") => "font-weight".into(),
                ("components::RingProgress", "section") => "sections".into(),
                ("layout::Divider", "vertical") => "orientation".into(),
                ("layout::ScrollArea", "both") => "scroll-x".into(),
                ("layout::Space", "h") => "height".into(),
                ("layout::Space", "w") => "width".into(),
                ("overlays::FloatingTooltip", "options") => "side".into(),
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
