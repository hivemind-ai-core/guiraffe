# Cache inventory (spec/23 §3, 2026-11-21)

Every cache / memo / pool / retained buffer that outlives a single function call, per crate, with its
bound, eviction rule and the test (if any) that locks the bound in.

Scope and method: `grep` for `cache|memo|pool|reuse|scratch|retain` plus a scan of every `struct`
field collection type (`HashMap`, `BTreeMap`, `VecDeque`, `Vec`, `Option<...>`); each hit was read at
the definition and at every insert/remove/`retain`/`clear` site. "Bound" is the argument that storage
cannot grow without user data growing. `UNBOUNDED` means nothing caps it; some of those are legitimate
(document-scoped stores) and are marked as such.

Legend: **bounded** = hard cap or strictly viewport/scene-scoped; **doc-scoped** = grows with the
user's data and leaves with it; **UNBOUNDED** = flagged for review.

## Summary of flags

| crate | item | why flagged | risk |
|---|---|---|---|
| chat | `ChatState.messages` (`Entity<MessageView>` per message) | one entity + `TextViewState` per message, never dropped while history holds it | **high** so far: 100k messages ⇒ 100k live entities. Justified by "markdown survives scroll-out", but not O(viewport); needs a bound or a measured per-message overhead test |
| tree | `TreeModel.expanded` / `selected` | `HashSet<NodeId>` of ids; never pruned when a node stops existing | moderate: drains over a long session with churn |
| code | `Shared.bubbles` | markdown entity per annotation *ever* placed in the viewport, removed only when annotations change | moderate: bounded by annotation count, not by viewport |
| overlays | `ToastQueue.queued` | visible list is capped, queue is not | low: a producer storm grows it without limit |
| overlays | `OverlayHost.contexts` | `HashMap<String, ContextBuilder>` per registered name, never removed | low: bounded by app registrations; re-register replaces |
| devtools | `Shared.stores` | `Vec<Rc<dyn InspectStore>>`, `push` only | low: one per `inspect_store` call, no removal API |
| layout | `MemoryPersist.values` | `HashMap<String, String>` keyed by persist key, never evicted | low: one entry per splitter/pane key |
| charts | `Stream.chunks` | grows with every `append` until the caller calls `retain_last` | by design (rolling window is caller policy); document it |

## charts

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `view::Frame.scene: Option<(Domain, Scene)>` (`view.rs:64`) | last compiled scene | **bounded**: exactly one | replaced when the domain/size changes; `invalidate()` sets `None` (`view.rs:87`); drop of `SceneRequest` cancels a superseded build | none (behavioural: `viewport_culls_and_downsamples_connected_runs`) |
| `view::Frame.pending: Option<SceneRequest>` | in-flight off-thread scene build | **bounded**: one | the request's `AtomicBool` is set on `Drop`, so a superseded job stops (`view.rs:71`) | none |
| `view::PlotState.hidden: BTreeSet<SeriesId>` | hidden-series set | doc-scoped (series count) | user toggles | `categorical_facets_keep_secondary_domain_and_visibility` |
| `model::Run.levels: Vec<Vec<usize>>` (`model.rs:303`) | LOD pyramid | **bounded**: halts at ≤ 16 384 samples, each level ÷8 (`scene.rs:359-388`), so ≤ len/7 extra indices | rebuilt with the run | `!model.runs[0].levels.is_empty()` (`scene.rs:904`) |
| `model::Run.line_breaks: Vec<usize>` | break indices | doc-scoped | rebuilt | `nan_splits_a_line_into_two_paths` |
| `data::Stream.chunks` (`data.rs:52`) | append chunks | **UNBOUNDED** unless `retain_last` is used | `retain_last` drains expired chunks (`data.rs:99`) | `rolling_window_preserves_snapshot_and_datum_keys`, `appends_preserve_snapshot_and_borrow_existing_rows` |
| `scene::Panel.ticks: [Vec<Tick>; 3]` | axis ticks per scene | scene-scoped | discarded with the scene | `left_margin_grows_for_long_axis_title` |

## chat

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `ChatState.messages: HashMap<MessageId, Entity<MessageView>>` (`lib.rs:74`) | one retained view (markdown state + tools + menu) per message | **UNBOUNDED** (one per message in history) | only on edit-truncation (`lib.rs:193`); otherwise lives as long as the history | `long_variable_history_virtualizes_and_jump_restores_tail` proves *rendering* is O(viewport) (`rendered < 40`), not that memory is |
| `History.rows: Vec<Entity<MessageView>>` (`history.rs:7`) | virtualised row list | doc-scoped (message count) | retained | same test |
| `MessageView.tools`, `reasoning`, `_layout_subscriptions` (`message.rs:24-32`) | per-message child entities/subscriptions | per message | with the message | none |
| `Composer.matches` (`composer.rs:52`) | slash/mention match list | completion count | rebuilt per keystroke | none |

## code

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `paint::ShapeCache.items` + `.order` (`paint.rs:33-34`) | shaped lines | **bounded**: 2 048 entries (`paint.rs:67`) | FIFO from `order` | `ten_million_line_document_shapes_only_a_viewport_and_reuses_cache` (`tests/interaction.rs:159`); asserts `cache_hits` grow on return |
| `paint::ShapeCache.frame: Rc<RefCell<Vec<PaintedRow>>>` (`paint.rs:35`) | reused paint-row buffer | viewport-scoped | `clear()` each paint (`paint.rs:292`), capacity kept | allocator gate (warm paint) |
| `paint::ShapeCache.gutter_label: String` (`paint.rs:36`) | reused gutter text buffer | one line | `mem::take` + reuse (`paint.rs:304,418`) | `PRODUCTION.md` allocation note |
| `surface::Geometry.shaped` (`surface.rs:89`) | shaped lines for the current layout pass | viewport-scoped | `clear()` at layout start (`paint.rs:272`) | same as above |
| `surface::Shared.hints`, `.annotation_ids` (`surface.rs:99,101`) | derived display lines / id map | doc-scoped (`PreparedAnalysis`) | replaced on publish (`surface.rs:292-293`) | none |
| `surface::Shared.bubbles: BTreeMap<u64, Entity<MarkdownState>>` (`surface.rs:117`) | markdown entity per bubble annotation | **UNBOUNDED** in practice: filled for every annotation scrolled through, cleared only when annotations/source change (`surface.rs:295,694`) | see left | none |
| `surface::Shared.dependents` (`surface.rs:120`) | weak refs to trace overlays | viewers | `retain` on publish (`surface.rs:222`) | none |
| `document::CodeDocument.starts` / `.hashes` (`document.rs:31-32`) | line index + per-line hash | doc-scoped, 16 B/line | with the document | `bench/tests/allocator.rs` exact index allocation; `PRODUCTION.md` §3 |
| `model::PreparedAnalysis.hints/.ids/.folds` (`model.rs:583-585`) | derived analysis | doc-scoped | rebuilt | property tests in `model.rs` |

## datagrid

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `grid::Core.rows: RefCell<Vec<Entity<RowView>>>` (`grid.rs:711`) | row-view pool | **bounded**: worst-case viewport + overscan (`vw::pool_size`), grown only to `pool` (`grid.rs:1199`) | created lazily, reused by slot | `pool is O(viewport)` (`tests/interaction.rs:150`), slot reuse (`:967-998`) |
| `grid::Core.palette: Option<(GuiraffeTheme, Palette)>` (`grid.rs:712`) | resolved colour palette | **bounded**: one | replaced when the theme differs (`grid.rs:2581-2591`) | `palette_resolutions` assertions (`tests/interaction.rs:206-210`) |
| `Model.undo: UndoLog` (`model/undo.rs:38`) | undo/redo cell diffs | **bounded**: `UNDO_LIMIT = 200` transactions (`undo.rs:44`), each ≤ `MAX_EDIT_CELLS = 100_000` | oldest dropped | undo unit tests (`model/undo.rs:169-215`) |
| `Model.layout: ColumnLayout` (`grid.rs:480`) | column placement | column count | recomputed on reorder/resize | `columns.rs` layout tests |
| `order::RowOrder::Indices(Arc<[u32]>)` (`order.rs:58`) | view→source permutation | row count, 4 B/row | replaced on sort/filter; background build is cancellable (`order.rs:90`) | `cancelled_inverse_does_not_publish_a_partial_permutation`; allocator gate |
| `Model` sort/filter `Vec<SortKey>`, `Vec<(usize, Filter)>` | view state | user-configured | user | — |

## devtools

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `model::FrameLog.samples` (`model.rs:7`) | rolling frame samples | **bounded**: `capacity` (panel uses 240, `view.rs:132`) | `pop_front` at capacity (`model.rs:31`) | `rolling_histogram_percentiles_and_empty_log` |
| `state::History.entries` (`state.rs:16`) | event snapshots (Arc-shared) | **bounded**: caller-supplied `capacity` (`inspect_store(.., capacity, ..)`) | oldest dropped by `Recorder` | `state.rs:229` (`capacity = 2`) |
| `state::StoreAdapter.cache: Option<(RowKey, Rc<Vec<EventRow>>)>` (`state.rs:140`) | last built row page | **bounded**: one | keyed by `(seq, len, offset, limit)` | none |
| `view::Shared.views: BTreeMap<SharedString, ViewInfo>` (`view.rs:93`) | instrumented view registry | **bounded**: template ids | `untrack` (`view.rs:266-271`) | `opening registers tracked views` (`view.rs:898`) |
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
| `model::FlameLayout.rows` + `.locations: HashMap<FrameId, (usize, usize)>` (`model.rs:55-56`) | laid-out spans + id lookup | doc-scoped; `locations` pre-sized to `source.len()` (`model.rs:64`) | rebuilt with the layout | `FlameLayout::new` invariants in `model.rs` tests |
| `model::Search.rows: Vec<Vec<usize>>` (`model.rs:280`) | search hit rows | hit count | rebuilt per query | search tests |
| `view::FlameGraph.search: Option<Arc<Search>>`, `tooltip` | last search / tooltip | **bounded**: one each | replaced | none |

## flow / flow-core

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `paint::MeshCache.entries` (`paint.rs:17`) | tessellated edge paths | **scene-scoped**: one entry per edge of the current scene, dropped when the scene's path changes (`paint.rs:46-54`); no FIFO cliff (commented) | `sync` retains only ids present in the scene | `review_12_retained_node_and_edge_buffer_survive_camera_frame` (`tests.rs:391`) |
| `paint::MeshCache.edge_buffer` (`paint.rs:18`) | reused edge-id buffer | visible edges | `clear` + reuse | same test (asserts pointer stability) |
| `paint::MeshCache.minimap` / `.grid` (`paint.rs:25-26`) | minimap meshes / grid path | **bounded**: one each, keyed by scene+size | replaced | none |
| `FlowViewport.nodes: BTreeMap<NodeId, Entity<NodeCard>>` (`lib.rs:103`) | node card entities | **bounded**: visible nodes only, `retain` on every query (`lib.rs:544`) | `retain` when a node leaves the viewport | none |
| `FlowViewport.ordered_ids`, `last_query_ids` (`lib.rs:115,119`) | visible ordering cache | visible nodes | replaced when the query result changes | none |
| `FlowViewport.order_scene/_selection`, `layout_preview` (`lib.rs:116-118`) | Arc snapshots to detect change | **bounded**: one each | replaced / `take()` | none |
| `FlowViewport.theme_cache` (`lib.rs:114`) | resolved theme | **bounded**: one | replaced on theme change | none |
| `interaction::Interaction.scratch: Vec<NodeId>` (`interaction.rs:99`) | hit-test scratch | visible nodes | reused (`clear`), capacity kept | none |
| `flow-core::Editor.undo/redo: VecDeque<Graph>` (`editor.rs:12-13`) | full graph snapshots | **bounded**: `limit = 100` (`editor.rs:63`), trimmed on push (`:163-165`) | oldest `pop_front` | undo property tests in `flow-core/tests` |
| `flow-core::Scene.paths/incident/depths/revisions/children: im::OrdMap` (`scene.rs:445-450`) | derived scene indexes | doc-scoped, structurally shared | with the scene | `changed_keys` reuse tests |
| `flow-core::SpatialIndex.root/locations` (`scene.rs:18-22`) | BVH over nodes/edges | scene-scoped | rebuilt on edit | `BVH updates match a linear scan` property test |
| `flow-core::Visible.nodes/edges` | last visibility result | visible only | replaced | — |

## icons / inputs / cli / macros / mocket / motion / router

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `router::History.entries` (`history.rs:5`) | navigation history | **bounded**: `cap` (default 64, `router.rs:125`) | oldest dropped in `from_parts`/push | history tests |
| `motion::Animated.start` | animation clock | **bounded**: one | cleared when settled | `Animated` tests |
| others | none found (icons embed assets; inputs/kbd/macros/mocket hold only per-call state) | — | — | — |

## layout

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `splitter::MemoryPersist.values: HashMap<String, String>` (`splitter.rs:261`) | persisted splitter sizes | **UNBOUNDED** (one entry per persist key, app-lifetime store) | none | `store.save/load` doc test (`splitter.rs:244`) |
| `splitter::SplitterState.reported: RefCell<Vec<Pixels>>` (`splitter.rs:170`) | last reported pane sizes | pane count | overwritten | `reported` dedup assertions (`splitter.rs:205-220`) |

## markdown

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `MarkdownState.document: Arc<Document>`, `.settled`, `.pushed` (`view.rs:56-58`) | parsed outline (headings/fences/tasks) | doc-scoped; incremental parse covers only the unsettled tail | replaced on `Change::Replace` (`view.rs:160-166`) | `appended_chunks_extend_the_document_without_replacing_the_text_view`, `a_mid_document_rewrite_replaces_the_source` |
| `view::MarkdownViewState.extensions` (`view.rs:69`) | text-element extensions | **bounded**: built once | never | none |
| `document::AnchorSlugs.seen: Vec<String>` (`document.rs:282`) | slug de-duplication | heading count, per parse | with the parse | anchor tests |

## mermaid

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `engine::Engine.cache: VecDeque<Entry>` (`engine.rs:171`) | drawn diagrams (SVG + raster) | **bounded**: `CACHE_ENTRIES = 16` and `CACHE_BYTES = 64 MiB` (`engine.rs:26-28`), enforced in `render_inner` (`:281-283`) | FIFO `pop_back` after each render | `cache_evicts_beyond_its_entry_limit`, `cache_eviction_keeps_the_most_recent_entry` (`engine.rs:438-457`) |
| `view::MermaidViewState.engine: Option<Engine>` (`view.rs:52`) | the cache above, lent to the background task | **bounded**: one | `take()`/restore around each render | `engine_rasterizes_and_the_second_render_reuses_layout_and_raster` |
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
| `store::MemoSlot.last_input` (`store.rs:430-434`) | memo input snapshot (skips recompute) | **bounded**: one per memoised selection | replaced | memo tests in `store.rs` |
| `store::Inner.slots: Vec<Rc<dyn Slot<S>>>` | live selector/memo slots | live selections | `retain(alive)` before push and on flush (`store.rs:274,395`) | `state` GPUI tests |
| `store::Inner.debounce: HashMap<&'static str, u64>` | per-key debounce counters | statically-known keys | never (bounded by key literals) | none |
| `engine::Undo.past/future` (`engine.rs:245-247`) | state snapshots | **bounded**: caller `limit` (`enable_undo`), trimmed on push (`engine.rs:443-446`) | `pop_front` | `undo_limit_zero_disables_and_reenabling_clears` |
| `engine::Engine.queue` (`engine.rs:254`) | event queue | **bounded**: per flush; `cascade_limit` (10 000) caps a loop | drained each dispatch | `cascade_limit_stops_dispatch_loops` |

## tree

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `TreeModel.rows`, `.sibling_positions`, `.indices` (`model.rs:41-43`) | visible rows + id index | **bounded**: visible rows | rebuilt on `refresh` | `refresh` tests |
| `TreeModel.expanded` / `.selected` / `.pending: HashSet<NodeId>` (`model.rs:44-46`) | expansion/selection by id | **UNBOUNDED** for `expanded`/`selected`: ids persist after the node disappears | `selected`/`expanded` only by user action; `pending` pruned on refresh (`model.rs:64`) | navigation tests; no bound test |

## ui / core / commands / windows / test

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `ui::controls::ControlCache.scopes` + `.nodes` (`controls.rs:28-30`) | native control state per (node, item scope) | **bounded**: live template nodes × live items | `remove_scope` on item disposal (`host.rs:387`), `clear` on template swap (`host.rs:904`) | none directly |
| `ui::host::Shared.bound`, `.repeats`, `.items` (`host.rs:70-73`) | bound-node entities, repeat models, item scopes | **bounded**: live template/repeat items | `dispose_item` (`host.rs:382-398`), `clear` on template swap | none |
| `ui::host::Shared.buf: PatchBuf`, `.changed`, `.renders` (`host.rs:76-81`) | reused patch buffer / changed-slot list / per-node render counters | template size | `clear` after each apply (`host.rs:148`) | none |
| `ui::repeat::KeyedList.candidates` (`repeat.rs:42`) | unkeyed fallback positions | rows of the current list | pruned when emptied (`repeat.rs:95-101,193-195`) | `keyed_splice_*` tests (`repeat.rs`) |
| `ui::slots::SlotTable.values/versions/styles` (`slots.rs:63-67`) | slot values | template size | replaced by patches | `slots.rs` tests |
| `ui::template::Template.ids/slot_index/event_index` (`template.rs:95,66-68`) | name→id maps | template size | with the template | compile tests |
| `ui::widget::Registry.widgets` (`widget.rs:271`) | built-in widget registry | registered kinds | never (static set) | catalogue tests |
| `ui::macro_support::Cached` (`macro_support.rs:103`) | keyed view per call site via `window.use_keyed_state` (`:140`) | call sites | framework-managed with the window | render-counter tests |
| `core::variant::CACHE` (thread-local, `variant.rs:164-170`) | resolved colour variants | **bounded**: 4 contexts × (`VARIANTS` 7 × `COLORS`) cells | round-robin slot replacement | `variant` tests |
| `commands::model::Registry.index` (`model.rs:142`) | canonical binding → command ids | command count | `index_remove` on unregister/update (`model.rs:179-187`) | keybinding tests |
| `commands::service::RegistryState.rows/flat_rows: OnceCell` (`service.rs:60-61`) | menu projections | **bounded**: one per revision × 2 shapes | reset on revision bump (`service.rs:84-85`) | menu tests |
| `commands::service::Configuration.routes` (`service.rs:132`) | route → binding count | live bindings | decremented, removed at zero | service tests |
| `commands::context::ContextStack.frames` | context stack | nesting depth | `pop`/scope drop | context tests |
| `windows::manager::WindowManager.entries/order/bus` (`manager.rs:120-123`) | open windows, order, cross-window subscriptions | **bounded**: open windows / subscribers | `remove` prunes all three (`manager.rs:389-406,455-468`) | manager tests |
| `windows::layout_store::LayoutStore` | on-disk window layout | one file | overwritten atomically | `layout_store` tests |
| `test::*` (`NodeIndex`, `Frame`, `EventSink`, `CallSocket`) | inspection/test scaffolding | per call | dropped with the report | — (test support) |

## Notes

- The five crates with a `PRODUCTION.md` already state their per-item overhead claims (§3). Items above
  marked **bounded** are consistent with those claims; the flagged ones are not yet mentioned there.
- Two caches deliberately have no hard cap because the scene/document *is* the bound
  (`flow::MeshCache.entries`, `flamegraph::FlameLayout`). They are documented as scene-scoped, which is
  the property the spec asks for (no FIFO working-set cliff, no per-frame churn).
- Nothing in this inventory was changed; this is a document-only pass.
