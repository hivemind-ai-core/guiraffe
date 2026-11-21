# Cache inventory (spec/23 §3, 2026-11-21)

Every cache / memo / pool / retained buffer that outlives a single function call, per crate, with its
bound, eviction rule and the test (if any) that locks the bound in. Audited on `perf-docs-probes` after
merging `release/0.2` (`ae79310`), i.e. against the post-rewrite chat, code, datagrid, flamegraph, flow,
markdown, mermaid, timeline and tree sources.

Method: scanned every `struct` field whose type is a collection or interior-mutable cell (`HashMap`,
`BTreeMap`, `VecDeque`, `Vec`, `BTreeSet`, `Option<...>`, `RefCell`, `Cell`), then read each hit at its
definition and at every insert / remove / `retain` / `clear` / `pop_front` site. "Bound" is the argument
that storage cannot grow unless user data grows.

Legend: **bounded** = hard cap or strictly viewport/scene-scoped; **doc-scoped** = grows with the user's
data and leaves with it; **UNBOUNDED** = flagged for review.

## Summary of flags

| crate | item | why flagged | risk |
|---|---|---|---|
| tree | `TreeModel.expanded` / `selected` | `HashSet<NodeId>` of ids; ids are never pruned when a node stops existing | moderate: drains over a long session with churn; the row index itself is incremental now, so this is the only per-id growth left |
| code | `Shared.bubbles` | markdown entity per bubble annotation *ever* placed in the viewport | moderate: bounded by the annotation set, not by the viewport |
| overlays | `ToastQueue.queued` | visible list is capped, backlog is not | low: a producer storm grows it without limit |
| overlays | `OverlayHost.contexts` | `HashMap<String, ContextBuilder>` per registered name | low: bounded by app registrations; re-register replaces; cleared on window close |
| devtools | `Shared.stores` | `Vec<Rc<dyn InspectStore>>`, `push` only | low: one per `inspect_store` call, no removal API |
| layout | `MemoryPersist.values` | `HashMap<String, String>` keyed by persist key, app-lifetime | low: one entry per splitter/pane key |
| charts | `Stream.chunks` | grows with every `append` until the caller calls `retain_last` | by design (rolling window is caller policy) |

Resolved since the pre-merge pass: chat per-message *text* state is now bounded (64 rows, evicted far
from the viewport); `flow` edge meshes are capped (8192 / 16 MiB) and deferred meshes are a bounded
cache; `datagrid` cell labels and shapes are two 1024-entry LRUs; `code`'s shape cache is capped by
entries *and* bytes and prunes out-of-viewport styled lines.

## charts

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `view::Frame.scene: Option<(Domain, Scene)>` (`view.rs:65`) | last compiled scene | **bounded**: exactly one | replaced when the domain/size changes; `invalidate()` clears it (`view.rs:111-115`); dropping `SceneRequest` cancels a superseded build | behavioural: `viewport_culls_and_downsamples_connected_runs` (`scene.rs`) |
| `view::Frame.previous_scene` (`view.rs:66`) | one previous scene, for zoom back-and-forth | **bounded**: one, and only while `Scene::cacheable()` | `publish_scene` (`view.rs:88-92`) drops the older scene on the next publish | none |
| `view::Frame.pending: Option<SceneRequest>` (`view.rs:67`) | in-flight off-thread scene build | **bounded**: one | the request's `AtomicBool` is set on `Drop`, so a superseded job stops | none |
| `view::PlotState.hidden: BTreeSet<SeriesId>` (`view.rs:105`) | hidden-series set | doc-scoped (series count) | user toggles | `categorical_facets_keep_secondary_domain_and_visibility` |
| `model::Run.levels: Vec<Vec<usize>>` (`model.rs:484`) | LOD pyramid | **bounded**: halts at ≤ 16 384 samples, each level ÷8 (`scene.rs:534-566`), so ≤ len/7 extra indices | rebuilt with the run | `assert!(!model.runs[0].levels.is_empty())` (`scene.rs:1242`) |
| `model::Run.line_breaks: Vec<usize>` (`model.rs:485`) | break indices | doc-scoped | rebuilt | `nan_splits_a_line_into_two_paths` |
| `data::Stream.chunks` (`data.rs:52`) | append chunks | **UNBOUNDED** unless `retain_last` is used | `retain_last` drains expired chunks (`data.rs:99`) | `rolling_window_preserves_snapshot_and_datum_keys`, `appends_preserve_snapshot_and_borrow_existing_rows` |
| `scene::Panel.draws` / `.ticks: [Vec<Tick>; 3]` (`scene.rs:118-119`) | per-scene primitives and axis ticks | scene-scoped | discarded with the scene | `left_margin_grows_for_long_axis_title` |

## chat

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `History::TextRows.recent: BTreeMap<usize, WeakEntity<MessageView>>` (`history.rs:11`) | rows whose Markdown/text state is retained | **bounded**: `RETAINED_TEXT_ROWS = 64` (`history.rs:7`) | `use_row` evicts the row farthest from the current index (`history.rs:34-47`); `defer_evict` calls `MessageView::evict_text` (`message.rs:110`) | `traversing_history_evicts_distant_text_and_restores_rows` (`gpui_tests.rs:317`, asserts ≤ 64 live) |
| `History::TextRows.selected: BTreeMap<usize, WeakEntity<MessageView>>` (`history.rs:12`) | rows pinned by a live text selection | grows only with the selection span; unpinned entries are re-evicted on the next `use_row` | `retain` in `use_row` (`history.rs:24-33`) | same test; `crates/chat/PRODUCTION.md` documents the exception |
| `ChatState.messages: HashMap<MessageId, Entity<MessageView>>` (`lib.rs:74`) | one lightweight view entity per message (data + optional text state) | doc-scoped: one entity shell per message, but the expensive `TextViewState` is evicted by `TextRows` above | only on edit-truncation (`lib.rs:193`) | `long_variable_history_virtualizes_and_jump_restores_tail` (`gpui_tests.rs:238`) |
| `History.rows: Vec<Entity<MessageView>>` (`history.rs:80`) | virtualised row list | doc-scoped (message count) | retained | same test |
| `MessageView.tools` / `.reasoning` / `_layout_subscriptions` (`message.rs:24-33`) | per-message child entities and subscriptions | per message | with the message | `tool_and_reasoning_toggle_with_keyboard_and_status_updates` |
| `Composer.matches` (`composer.rs:52`) | slash/mention match list | completion count | rebuilt per keystroke | `completion_keyboard_and_attachment_removal` |

## code

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `paint::ShapeCache.items` + `.order` (`paint.rs:39-40`) | shaped lines | **bounded twice**: `MAX_SHAPE_CACHE_ENTRIES = 2 048` and `MAX_SHAPE_CACHE_TEXT_BYTES = 256 KiB` (`paint.rs:32-33`), enforced in `insert` (`paint.rs:119-135`) | FIFO from `order` | `ten_million_line_document_shapes_only_a_viewport_and_reuses_cache` (`tests/interaction.rs:159`) |
| `paint::ShapeCache.styled_viewport` (`paint.rs:44`) | which styled rows are resident | viewport + `STYLE_OVERSCAN_ROWS` (8) | `retain_viewport_styles` (`paint.rs:47-86`) drops styled lines that scrolled out | same test |
| `paint::ShapeCache.frame: Rc<RefCell<Vec<PaintedRow>>>` (`paint.rs:42`) | reused paint-row buffer | viewport-scoped | `clear()` each paint, capacity kept | allocator gate (`bench/tests/allocator.rs`) |
| `paint::ShapeCache.gutter_label: String` (`paint.rs:43`) | reused gutter text buffer | one line | `mem::take` + reuse | `crates/code/PRODUCTION.md` allocation note |
| `surface::Geometry.shaped: HashMap<usize, Rc<ShapedLine>>` (`surface.rs:89`) | shaped lines for the current layout pass | viewport-scoped | `clear()` at layout start (`paint.rs:337`) | same allocator gate |
| `surface::Shared.hints` / `.annotation_ids` (`surface.rs:99,101`) | derived display lines / annotation id map | doc-scoped (`PreparedAnalysis`) | replaced on publish | `annotation_updates_preserve_source_and_plain_hover_does_not_shape` (`tests/interaction.rs:220`) |
| `surface::Shared.bubbles: BTreeMap<u64, Entity<MarkdownState>>` (`surface.rs:117`) | markdown entity per bubble annotation | **bounded by the annotation set, not the viewport**: filled as bubbles scroll into view | cleared when annotations or source change (`surface.rs:295,697`) | `annotation_updates_preserve_source_and_plain_hover_does_not_shape` |
| `surface::Shared.dependents: Vec<WeakEntity<TraceView>>` (`surface.rs:120`) | weak refs to trace overlays | attached viewers | `retain` on notify | none |
| `document::CodeDocument` (`document.rs:29`) | line-start index (`LineStarts`: uniform stride, packed `u32`, or `usize`) + 256-cell minimap | doc-scoped; compact lines cost 4 B, uniform costs nothing per line | with the document | `bench/tests/allocator.rs` exact index allocation; `crates/code/PRODUCTION.md` §3 |
| `model::IntervalIndex` (`model.rs:186`) | annotation interval index | annotation count | rebuilt on publish | property tests in `model.rs` |
| `model::PreparedAnalysis` (`model.rs:578`) | index + row map + folds + hints + id map | doc-scoped; buffers move on publication instead of being copied | rebuilt | `model.rs` property tests |

## datagrid

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `grid::Core.rows: RefCell<Vec<Entity<RowView>>>` (`grid.rs:720`) | row-view pool | **bounded**: worst-case viewport + overscan (`vw::pool_size`), grown only to `pool` | created lazily, reused by slot | `pool is O(viewport)` (`tests/interaction.rs:150`), `recycled_slots_show_their_new_rows` (`:1015`) |
| `grid::Core.shapes: Rc<RefCell<CellShapes>>` (`grid.rs:721`) | shaped cell text + line layouts | **bounded**: `CELL_CACHE_CAPACITY` = 1 024 entries (`grid.rs:3107`), `CellShapes = lru::LruCache` (`grid.rs:3115`) | LRU `put` during paint (`grid.rs:3327-3331`) | `long_multilingual_cells_have_bounded_labels_and_full_copy` (`tests/interaction.rs:258`) |
| `grid::Core.cell_text: RefCell<lru::LruCache<u64, SharedString>>` (`grid.rs:722`) | formatted cell text keyed by content hash | **bounded**: same 1 024-entry LRU | LRU | same test |
| `grid::Core.palette: RefCell<Option<(GuiraffeTheme, Palette)>>` (`grid.rs:723`) | resolved colour palette | **bounded**: one | replaced when the theme differs (`grid.rs:2635-2645`) | `palette_is_shared_across_row_recycling_and_refreshes_on_theme_change` (`tests/interaction.rs:220`) |
| `Model.undo: UndoLog` (`model/undo.rs:38`) | undo/redo cell diffs | **bounded**: `UNDO_LIMIT = 200` transactions (`model/undo.rs:44`), each ≤ `MAX_EDIT_CELLS = 100_000` | oldest dropped | undo unit tests (`model/undo.rs`) |
| `Model.layout: ColumnLayout` (`grid.rs:487`) | column placement | column count | recomputed on reorder/resize | `bands_and_display_order`, `reorder_moves` (`columns.rs`) |
| `order::RowOrder::Indices(Arc<[u32]>)` (`order.rs:58`) | view→source permutation | row count, 4 B/row | replaced on sort/filter; background build is cancellable (`order.rs:90`) | `cancelled_inverse_does_not_publish_a_partial_permutation`; allocator gate |
| `Model.sort` / `.filters` | view state | user-configured | user | — |

## devtools

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `model::FrameLog.samples` (`model.rs:7`) | rolling frame samples | **bounded**: `capacity` (panel uses 240, `view.rs:132`) | `pop_front` at capacity (`model.rs:31`) | `rolling_histogram_percentiles_and_empty_log` |
| `state::History.entries` (`state.rs:16`) | event snapshots (Arc-shared) | **bounded**: caller-supplied `capacity` (`inspect_store(.., capacity, ..)`) | oldest dropped by `Recorder` | `state.rs:229` (`capacity = 2`) |
| `state::StoreAdapter.cache: Option<(RowKey, Rc<Vec<EventRow>>)>` (`state.rs:140`) | last built row page | **bounded**: one | keyed by `(seq, len, offset, limit)` | none |
| `view::Shared.views: BTreeMap<SharedString, ViewInfo>` (`view.rs:93`) | instrumented view registry | **bounded**: template ids | `untrack` (`view.rs:266-271`) | `closed_tools_skip_tracking_and_opening_registers_views` |
| `view::Shared.stores: Vec<Rc<dyn InspectStore>>` (`view.rs:94`) | connected stores | **UNBOUNDED** (append-only) | none | none |

## dnd

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `board::SortableBoard.focus: HashMap<usize, FocusHandle>` (`board.rs:40`) | focus handles per card id | card count | built once at construction | `focused card keeps its handle` behaviour tests in `board.rs` |
| `dock::SplitResize.sizes: Vec<Pixels>` | drag scratch | pane count | reused | none |
| `sortable::Selection.indices`, `Reorder.original_indices` | drag state | selection size | dropped with the drag | none |

## flamegraph

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `model::FlameLayout` (`model.rs:266`) — `rows`, `locations`, `extent` | laid-out spans + id→(depth, offset) index | doc-scoped; `Locations` is `Sequential` (dense `u32` packing, `model.rs:56-57`) with an `Arbitrary` HashMap fallback (`model.rs:89-92`) | rebuilt with the layout | `sequential_index_falls_back_and_checks_duplicates`, `packed_locations_keep_wide_and_reserved_values` |
| `model::Search.rows` (`model.rs:553`) | search hit rows | hit count | rebuilt per query | bucket-search property test (`model.rs:732`) |
| `view::LabelCache` (`view.rs:119`) | shaped frame labels | **bounded**: `LABEL_SLOTS = 128` direct-mapped slots, labels ≤ 256 B (`view.rs:42-43`) | slot replacement by hash | `crates/flamegraph/PRODUCTION.md` label-cache table |
| `view::FlameGraph.search` / `.tooltip` / `.pending_layout` | last search, tooltip entity, in-flight layout | **bounded**: one each | replaced / dropped | `crates/flamegraph/PRODUCTION.md` |

## flow / flow-core

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `paint::MeshCache.entries: BTreeMap<EdgeId, (Arc<EdgePath>, EdgeMesh)>` (`paint.rs:130`) | tessellated edge meshes | **bounded**: `MAX_EDGE_MESHES = 8192` and `MAX_EDGE_MESH_BYTES = 16 MiB` (`paint.rs:16-17`), checked in `get` (`paint.rs:204-216`); pruned to the visible set when admission-limited and the viewport moves (`paint.rs:188-201`) | `retain` on scene change (`sync`, `paint.rs:176-187`) plus those two limits | `review2_real_second_frame_over_2048_visible_edges_is_cached` (`paint.rs:1556`), `review_10_second_frame_over_2048_edges_has_no_misses` (`:1401`) |
| `paint::DeferredMeshCache.entries` + `.recency` (`paint.rs:70-71`) | tessellation for deferred (overview) scenes | **bounded**: `DEFERRED_MESH_ENTRIES = 1 024` / `DEFERRED_MESH_BYTES = 1 MiB` (`paint.rs:57-58`) | LRU by the `recency` tree (`paint.rs:115-125`); whole cache reset when the scene switches to deferred | `intermediate_zoom_uses_overview_only_for_dense_viewports` (`paint.rs:1346`) |
| `paint::MeshCache.edge_buffer: Vec<EdgeId>` (`paint.rs:133`) | reused edge-id buffer | visible edges | transferred/cleared each frame, capacity kept | `review_12_frame_transfers_visible_edge_buffer` (`paint.rs:1423`) |
| `paint::MeshCache.minimap` / `.grid` (`paint.rs:144-145`) | minimap meshes / grid path | **bounded**: one each, keyed by scene+size | replaced | `dense_minimap_bounds_submitted_vertices` (`paint.rs:1304`) |
| `FlowViewport.nodes: BTreeMap<NodeId, Entity<NodeCard>>` (`lib.rs:104`) | node card entities | **bounded**: visible nodes only | `retain` when a node leaves the viewport or the graph (`lib.rs:613-625`) | `ten_million_nodes_keep_detailed_viewport_work_constant` (`paint.rs:1227`) |
| `FlowViewport.ordered_ids`, `.last_query_ids` (`lib.rs:117,121`) | visible ordering cache | visible nodes | replaced when the query result changes | same test |
| `FlowViewport.layout_preview` / `.order_scene` / `.order_selection` (`lib.rs:118-120`) | Arc snapshots used as change stamps | **bounded**: one each | replaced / `take()` | `background_layout_previews_then_commits_one_undoable_edit` (`tests.rs:792`) |
| `FlowViewport.index_job` (`lib.rs:112`) | in-flight background spatial-index build | **bounded**: one, with a cancellation token | dropped / superseded | `cancelling_background_layout_keeps_the_document` (`tests.rs:833`) |
| `FlowViewport.theme_cache` (`lib.rs:116`) | resolved theme | **bounded**: one | replaced on theme change | none |
| `interaction::Interaction.scratch: Vec<NodeId>` (`interaction.rs:99`) | hit-test and z-order scratch | visible nodes | reused, capacity kept | `review_12_retained_node_and_edge_buffer_survive_camera_frame` (`tests.rs:389`) |
| `flow-core::Editor.undo` / `.redo` (`editor.rs:12-13`) | full graph snapshots | **bounded**: `limit = 100` (`editor.rs:75`), trimmed on push and by `history_limit` (`editor.rs:109-115`) | oldest `pop_front` | `shrinking_history_limit_bounds_redo_as_well_as_undo` (`tests/behavior.rs:330`) |
| `flow-core::Scene` indexes (`scene.rs:616`) — `paths`, `incident`, `depths`, `revisions`, `children` (`im::OrdMap`) plus `SpatialIndex` (`scene.rs:20`) | derived geometry and BVHs | doc-scoped, structurally shared; edits recompute only changed keys | with the scene | `spatial_updates_match_linear_scan` (`tests/review_2.rs:174`), `review2_8_selection_only_edit_has_no_history_or_scene_change` |
| `flow-core::Visible.nodes` / `.edges` (`scene.rs:525`) | last visibility result | visible only | replaced | — |

## icons / inputs / cli / macros / mocket / motion / router

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `router::History.entries` (`history.rs:5`) | navigation history | **bounded**: `cap` (default 64, `router.rs:125`) | oldest dropped on push | `push_back_forward_and_truncation` |
| `motion::Animated.start` | animation clock | **bounded**: one | cleared when settled | `Animated` tests |
| others | none found (icons embed assets at compile time; inputs/kbd/macros/mocket hold only per-call state) | — | — | — |

## layout

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `splitter::MemoryPersist.values: HashMap<String, String>` (`splitter.rs:296`) | persisted splitter sizes | **UNBOUNDED** (one entry per persist key, app-lifetime store) | none | `store.save/load` doc test (`splitter.rs:240-246`) |
| `splitter::SplitterState.reported: RefCell<Vec<Pixels>>` (`splitter.rs:175`) | last reported pane sizes | pane count | overwritten | reported-dedup assertions (`splitter.rs`) |

## markdown

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `MarkdownState.document: Arc<Document>` + `.settled` (`view.rs:56,59`) | parsed outline (headings/fences/tasks) and the settled byte prefix | doc-scoped; only the unsettled tail is analysed (`view.rs:233-262`) | merged incrementally; replaced on `Change::Replace` | `incremental_metadata_matches_one_shot_at_every_character_boundary` (`streaming.rs`), `appended_chunks_extend_the_document_without_replacing_the_text_view` (`tests.rs`) |
| `MarkdownViewState.extensions` (`view.rs:70`) | text-element extensions | **bounded**: built once | never (feature-gated) | none |
| `Document.AnchorSlugs.seen` (`document.rs:306`) | slug de-duplication | heading count, per parse | with the parse | anchor tests |
| Kit text layout cache (vendored, outside this workspace) | shaped layouts across all text views | **bounded**: 4 096 per UI thread | Kit's own | documented in `crates/markdown/PRODUCTION.md` |

## mermaid

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `engine::Engine.cache: VecDeque<Entry>` (`engine.rs:259`) | drawn diagrams (source + SVG + parsed tree + raster) | **bounded**: `CACHE_ENTRIES = 16` and `CACHE_BYTES = 64 MiB` (`engine.rs:31-33`), enforced at `engine.rs:370` | FIFO `pop_back` after each render | `cache_evicts_beyond_its_entry_limit`, `cache_eviction_keeps_the_most_recent_entry` (`engine.rs:711-727`) |
| `view::MermaidViewState.engine` (`view.rs:68`) | the cache above, lent to the background task | **bounded**: one | `take()`/restore around each render | `engine_rasterizes_and_the_second_render_reuses_layout_and_raster` |
| `view::MermaidViewState.published` / `.preview` (`view.rs:70-71`) | current raster image / transient overview preview | **bounded**: one each | replaced or dropped on refinement | `crates/mermaid/PRODUCTION.md` cache-bound table |
| `preview::Overview` buffers (`preview.rs:14-16`) | overview geometry | **bounded**: `MAX_PRIMITIVES = 2 048` (`preview.rs:11`) | rebuilt per preview | `crates/mermaid/PRODUCTION.md` |
| `source::CacheKey` | identity (hash + theme + font size) | n/a | n/a | `cache_key_separates_theme_and_font_size` |

## overlays

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `host::OverlayHost.dialogs/dialog_order/confirms/focus_chain` (`host.rs:51-56`) | open managed dialogs | **bounded**: number of open overlays | removed on close/dismiss (`host.rs:205-206`) | dialog tests |
| `host::OverlayHost.contexts: HashMap<String, ContextBuilder>` (`host.rs:57`) | registered overlay builders | **UNBOUNDED** (per name) | replaced on re-register; cleared on window close (`host.rs:96`) | none |
| `host::OverlayHost.cards: HashMap<ToastId, ToastCard>` + `owned_notes` (`host.rs:59,63`) | toast cards | **bounded**: `retain` against the live notes each sync (`host.rs:474,488`) | `retain` | toast tests |
| `toast::ToastQueue.visible` (`toast.rs:59`) | visible toasts | **bounded**: `limit` (default 5) | `promote`/`advance` | `fifo_and_fresh_clock`, `set_limit` tests |
| `toast::ToastQueue.queued` (`toast.rs:60`) | backlog | **UNBOUNDED** | only by visible/remove | `update_visible_and_queued` (does not test the bound) |
| `search::SearchIndex` (`search.rs:120-126`) | normalised command text + reusable DP buffers + last result | **bounded**: entry count, `scores` pre-sized, one cached query | rebuilt via `new`; result cache replaced per query | `unicode_and_rejection`, `contiguous_preferred`, `ranking_keywords_disabled_and_limit`, `review_9_best_alignment_can_skip_an_earlier_match` |
| `spotlight::Spotlight.matches` | last match list | `limit` (50) | replaced | none |
| `stack::OverlayStack.entries` | open overlay stack | **bounded**: open overlays | `retain` on dismiss | dismissal tests |

## showcase / gallery

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `showcase::shell::Showcase.demos: HashMap<usize, AnyView>` (`shell.rs:46`) | visited demo views | **bounded**: demo count | never (session app) | none |
| `showcase::events::EventLines.lines: VecDeque` (`events.rs:19`) | event log | **bounded**: `CAPACITY = 5` (`events.rs:14`) | `truncate` | `keeps_newest_first_and_counts_all` |
| `gallery::shell::Gallery.pages: HashMap<Page, AnyView>` (`shell.rs:22`) | visited pages | **bounded**: page count | never (session app) | — |

## state

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `store::MemoSlot.last_input` (`store.rs:430`) | memo input snapshot (skips recompute) | **bounded**: one per memoised selection | replaced | memo tests in `store.rs` |
| `store::Inner.slots: Vec<Rc<dyn Slot<S>>>` | live selector/memo slots | live selections | `retain(alive)` before push and on flush (`store.rs:274,395`) | `state` GPUI tests |
| `store::Inner.debounce: HashMap<&'static str, u64>` | per-key debounce counters | statically-known keys | never (bounded by key literals) | none |
| `engine::Undo.past` / `.future` (`engine.rs:256`) | state snapshots | **bounded**: caller `limit`, trimmed on push | `pop_front` | `undo_limit_zero_disables_and_reenabling_clears` |
| `engine::Engine.queue` (`engine.rs:254`) | event queue | per flush; `cascade_limit` (10 000) caps a dispatch loop | drained each dispatch | `cascade_limit_stops_dispatch_loops` |

## tree

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `TreeModel.rows: Rows` (`model.rs:38`), `Rows` (`rows.rs:123`) — `blocks` / `free` arena, `IdIndex.pages` (`rows.rs:40`), `wide_siblings` | implicit-treap row blocks with a per-id rank index | **bounded**: visible rows | rebuilt on `refresh`; edits splice incrementally instead of rebuilding the index | `cross_block_splices_preserve_ranks_positions_and_order` (`rows.rs:432`) |
| `TreeModel.snapshot: OnceLock<Vec<Row>>` (`model.rs:40`) | materialised contiguous row copy | **bounded**: visible rows, only after a structural edit | `take()` on `refresh` (`model.rs:59`) | none |
| `TreeModel.expanded` / `.selected: HashSet<NodeId>` (`model.rs:41,43`) | expansion and selection by id | **UNBOUNDED**: ids are not pruned when a node disappears from the source | only by user action | `keyboard_expand_scroll_and_validation`, `hidden_and_unchanged_expansion_do_not_notify` |
| `TreeModel.pending` (`model.rs:42`) | ids awaiting an async child load | requested loads | `retain` on refresh (`model.rs:60-61`) | `keyboard_expand_scroll_and_validation` |

## ui / core / commands / windows / test

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `ui::controls::ControlCache.scopes` + `.nodes` (`controls.rs:28`) | native control state per (node, item scope) | **bounded**: live template nodes × live items | `remove_scope` on item disposal (`host.rs:387`), `clear` on template swap (`host.rs:904`) | none directly |
| `ui::host::Shared.bound` / `.repeats` / `.items` (`host.rs:70-73`) | bound-node entities, repeat models, item scopes | **bounded**: live template/repeat items | `dispose_item` (`host.rs:381-394`), `clear` on template swap | none |
| `ui::host::Shared.renders` / `.buf` / `.changed` (`host.rs:78-81`) | per-node render counters, reused patch buffer, changed-slot list | template size | `clear` after each apply (`host.rs:148`) | none |
| `ui::repeat::KeyedList.candidates` (`repeat.rs:42`) | unkeyed fallback positions | rows of the current list | pruned when emptied (`repeat.rs:96-101,192-194`) | `positional_field_edits_keep_unkeyed_and_duplicate_input_state`, `duplicate_candidates_stay_live_across_mixed_splices` (`repeat.rs`) |
| `ui::slots::SlotTable` (`slots.rs:62`) | slot values, versions, styles | template size | replaced by patches | `patches_apply_and_noops_dont_count`, `migrate_keeps_values_by_name` |
| `ui::template::Template.ids` / `slot_nodes` / `Names.slot_index` / `.event_index` (`template.rs:94-95,66-68`) | name→id maps and slot→node indices | template size | with the template | compile tests |
| `ui::widget::Registry.widgets` (`widget.rs:271`) | built-in widget registry | registered kinds | never (static set) | catalogue tests |
| `ui::macro_support::Cached` / `CacheView` (`macro_support.rs:103`) | keyed view per call site via `window.use_keyed_state` (`:140`) | call sites | framework-managed with the window | render-counter tests |
| `core::variant::CACHE` (thread-local, `variant.rs:164`, `SLOTS = 4` at `:163`) | resolved colour variants | **bounded**: 4 contexts × `VARIANTS`(7) × `COLORS` cells | round-robin slot replacement | `all_variants_legible` |
| `commands::model::Registry.index` (`model.rs:142`) | canonical binding → command ids | command count | `index_remove` on unregister/update (`model.rs:179`) | `binding_index_follows_every_mutation`, `mutations_are_change_checked_and_reindex` |
| `commands::service::RegistryState.rows` / `.flat_rows` (`service.rs:60-61`) | menu projections | **bounded**: one per revision × 2 shapes | reset on revision bump (`service.rs:84-85`) | no test (`service.rs` has no test module) |
| `commands::service::Configuration.routes` (`service.rs:132`) | route → binding count | live bindings | decremented, removed at zero (`swap_routes`, `service.rs:297`) | no test |
| `commands::context::ContextStack.frames` | context stack | nesting depth | `pop` / scope drop | `deepest_matching_context_wins_over_catalog_order` |
| `windows::manager::WindowManager.entries` / `.order` / `.bus` (`manager.rs:120-123`) | open windows, order, cross-window subscriptions | **bounded**: open windows / subscribers | `remove` prunes all three (`manager.rs:389-406,455-468`) | no unit test in `manager.rs` |
| `windows::layout_store::LayoutStore` | on-disk window layout | one file | overwritten atomically | `missing_then_roundtrip`, `corrupt_file_is_set_aside_each_time_without_clobbering` |
| `test::*` (`NodeIndex`, `Frame`, `EventSink`, `CallSocket`) | inspection/test scaffolding | per call | dropped with the report | — (test support) |

## Notes

- Every crate with a `PRODUCTION.md` (charts, chat, code, datagrid, flamegraph, flow, markdown, mermaid,
  timeline, tree) states per-item overhead claims under §3; the **bounded** rows here match those claims.
  The flagged rows are not mentioned there yet.
- `flow::MeshCache.entries` and `flamegraph::FlameLayout` are not item-capped because the scene/document
  *is* the bound, but both are either byte-capped (flow: 8192 meshes / 16 MiB) or structurally shared, so
  neither can drift. That is the property spec/23 §3 asks for.
- Nothing in this inventory changed code; it is a document-only pass.

## Link to the PERF-0.2 diagnoses

Numbers are the foreman's quiet-machine baseline in [`plans/PERF-0.2.md` §Baseline](PERF-0.2.md).

- **Chat, 4.49 GiB / 130 s mount.** The retained-state picture changed with the release merge: the
  expensive per-message `TextViewState` is now bounded (64 rows, `history.rs:7`) and rebuilt from raw text
  on return, locked by `traversing_history_evicts_distant_text_and_restores_rows`. What is still
  doc-scoped is one lightweight `MessageView` entity per history row (`ChatState.messages`). So the 4.5 GiB
  is no longer "a full markdown tree per message"; re-measuring before attributing the remainder is
  worthwhile, and the entity-per-row growth wants a measured per-message overhead figure.
- **Markdown, 799 MiB / 4.7 s mount.** `MarkdownState` analyses only the unsettled tail
  (`view.rs:233-262`, covered at every character split by the streaming property test). No unbounded
  retained cache exists on this path: `code::ShapeCache` is capped at 2 048 entries / 256 KiB and Kit's
  per-thread layout cache at 4 096. The 799 MiB is live text/layout state and the published render tree.
- **Mermaid, 33 MiB, 31.8 s mount.** `Engine.cache` is capped (16 entries / 64 MiB) with eviction tests,
  and the preview primitives are capped at 2 048; the mount cost is third-party layout. Zoom re-raster is
  a new raster for the same entry, not a new cache entry.
- **Flow, RSS +30 % over 10 s.** Both mesh caches are capped (8192 meshes / 16 MiB, and 1 024 deferred
  meshes / 1 MiB) and node cards are `retain`ed to the visible set, so no unbounded retained cache was
  found in `flow`/`flow-core`. The growth is more likely GPUI paint-side state retained by the element
  tree, which is outside this inventory's scope.
