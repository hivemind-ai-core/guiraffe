# Cache inventory (spec/23 §3, 2026-11-21)

Every cache / memo / pool / retained buffer that outlives a single function call, per crate, with its
bound, eviction rule and the test (if any) that locks the bound in. Audited on `perf-docs-probes` after
merging `release/0.2`, i.e. against the post-rewrite chat, code (incl. the new `decorations` module),
datagrid, flamegraph, flow, markdown, mermaid, timeline and tree sources.

References are **symbol-based** (the source file plus the type or field name) rather than line numbers: the
release branch is still moving, and line numbers rot within hours. Grep the symbol.

Method: scanned every `struct` field whose type is a collection or interior-mutable cell (`HashMap`,
`BTreeMap`, `VecDeque`, `Vec`, `BTreeSet`, `Option<...>`, `RefCell`, `Cell`), then read each hit at its
definition and at every insert / remove / `retain` / `clear` / `pop_front` site. "Bound" is the argument
that storage cannot grow unless user data grows.

Legend: **bounded** = hard cap or strictly viewport/scene-scoped; **doc-scoped** =
grows with caller data and leaves with it. Registrations and active interactions
follow their owning lifetimes.

The eight previously flagged entries are covered in the per-crate sections.

## charts

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `Frame.scene: Option<(Domain, Scene)>` (`view.rs`) | last compiled scene | **bounded**: exactly one | replaced when the domain/size changes; `invalidate()` clears it (`view.rs`); dropping `SceneRequest` cancels a superseded build | behavioural: `viewport_culls_and_downsamples_connected_runs` (`scene.rs`) |
| `Frame.previous_scene` (`view.rs`) | one previous scene, for zoom back-and-forth | **bounded**: one, and only while `Scene::cacheable()` | `publish_scene` (`view.rs`) drops the older scene on the next publish | none |
| `Frame.pending: Option<SceneRequest>` (`view.rs`) | in-flight off-thread scene build | **bounded**: one | the request's `AtomicBool` is set on `Drop`, so a superseded job stops | none |
| `PlotState.hidden: BTreeSet<SeriesId>` (`view.rs`) | hidden-series set | doc-scoped (series count) | user toggles | `categorical_facets_keep_secondary_domain_and_visibility` |
| `Run.levels: Vec<Vec<usize>>` (`model.rs`) | LOD pyramid | **bounded**: halts at ≤ 16 384 samples, each level ÷8 (`scene.rs`), so ≤ len/7 extra indices | rebuilt with the run | `assert!(!model.runs[0].levels.is_empty())` (`scene.rs`) |
| `Run.line_breaks: Vec<usize>` (`model.rs`) | break indices | doc-scoped | rebuilt | `nan_splits_a_line_into_two_paths` |
| `Stream.chunks` (`data.rs`) | caller-owned chart samples | one chunk per nonempty append; at most `n` chunks after `retain_last(n)`, with up to one partial boundary chunk | trim fully expired chunks; old snapshots release on drop | `rolling_window_bounds_chunks_under_append_churn` |
| `Panel.draws` / `.ticks: [Vec<Tick>; 3]` (`scene.rs`) | per-scene primitives and axis ticks | scene-scoped | discarded with the scene | `left_margin_grows_for_long_axis_title` |

## chat

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `RowCache.recent` (`chat/src/history.rs`, `struct RowCache`) | mounted message rows whose Markdown state is live | **bounded**: `RETAINED_ROWS = 64` | `RowCache::use_row` evicts the row farthest from the current index; `History::evict` demotes it to `Row::Cold` | `traversing_history_evicts_distant_text_and_restores_rows` |
| `RowCache.pinned` (`history.rs`) | active interaction state | offscreen rows with a live stream, menu, or selection | row and text notifications release pins; re-entry and truncation remove entries | `ended_pins_evict_without_entering_another_row` |
| `Row::Cold` / `Row::Mounted` (`chat/src/history.rs`) | per-message storage: cold rows keep data only, mounted rows add `Entity<MessageView>` | **bounded**: cold rows are O(data) per message by design; mounted rows ≤ 64 + pins | `History::evict` on eviction | `long_variable_history_virtualizes_and_jump_restores_tail` |
| `ChatState.messages: HashMap<MessageId, usize>` (`chat/src/lib.rs`) | message id → history index | doc-scoped, one `usize` per message | entry removed on edit-truncation | `conversations_are_isolated_and_ids_are_unique` |
| `History.rows: Vec<Row>` (`chat/src/history.rs`) | virtualised row list | doc-scoped (message count) | retained | `long_variable_history_virtualizes_and_jump_restores_tail` |
| `MessageView.tools` / `.reasoning` / `_layout_subscriptions` (`chat/src/message.rs`) | per-message child entities and subscriptions, created only for mounted rows | per mounted row | dropped by `History::evict` | `tool_and_reasoning_toggle_with_keyboard_and_status_updates` |
| `Composer.matches` (`chat/src/composer.rs`) | slash/mention match list | completion count | rebuilt per keystroke | `completion_keyboard_and_attachment_removal` |

## code

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `paint::ShapeCache.items` + `.order` (`paint.rs`) | shaped lines | **bounded twice**: `MAX_SHAPE_CACHE_ENTRIES = 2_048` and `MAX_SHAPE_CACHE_TEXT_BYTES = 256 KiB`, enforced in `ShapeCache::insert` | FIFO from `order` | `ten_million_line_document_shapes_only_a_viewport_and_reuses_cache` (`tests/interaction.rs`) |
| `paint::ShapeCache.styled_viewport` (`paint.rs`) | which styled rows are resident | viewport + `STYLE_OVERSCAN_ROWS` (8) | `ShapeCache::retain_viewport_styles` drops styled lines that scrolled out | same test |
| `paint::ShapeCache.frame` (`paint.rs`) | reused paint-row buffer | viewport-scoped | cleared each paint, capacity kept | allocator gate (`bench/tests/allocator.rs`) |
| `paint::ShapeCache.gutter_label` (`paint.rs`) | reused gutter text buffer | one line | `mem::take` + reuse | `crates/code/PRODUCTION.md` allocation note |
| `Geometry.shaped` (`surface.rs`) | shaped lines for the current layout pass | viewport-scoped | cleared at layout start | same allocator gate |
| `Shared.hints` / `Shared.annotation_ids` (`surface.rs`) | derived display lines / annotation id map | doc-scoped (`PreparedAnalysis`) | replaced on publish | `annotation_updates_preserve_source_and_plain_hover_does_not_shape` (`tests/interaction.rs`) |
| `Shared.bubbles` (`surface.rs`) | Markdown entities for placed cards | **bounded** by viewport height divided by three line heights | retain placed cards only; clear on source or annotation change | `bubble_cache_evicts_cards_when_they_leave_the_viewport` |
| `Shared.dependents` (`surface.rs`) | weak refs to trace overlays | attached viewers | `retain` on notify | none |
| `Decorations.layers` / `.styles` / `.style_ids` / `.paint_order` (`decorations.rs`) | interned decoration styles and per-layer records | doc-scoped: one style per distinct style value, plus the caller's layers | `Decorations` replaced wholesale on publish | `crates/code/PRODUCTION.md`; decoration tests in `decorations.rs` |
| `DecorationLayer.index: Arc<DecorationIndex>` (`decorations.rs`) | compact interval index (`records`, `nodes`, `by_start`, `by_end`, `diagnostics`, `changes`) | doc-scoped, capacity-checked against `u32` (`DecorationError::Capacity`); built once per layer, shared by `Arc`, queries borrow with no scratch | rebuilt when the layer's records change | `decorations.rs` index tests |
| `CodeDocument` (`document.rs`) | line-start index (`LineStarts`: uniform stride / packed `u32` / `usize`) + 256-cell minimap | doc-scoped; compact lines cost 4 B, uniform costs nothing per line | with the document | `bench/tests/allocator.rs` exact index allocation; `crates/code/PRODUCTION.md` §3 |
| `IntervalIndex` (`model.rs`) | annotation interval index | annotation count | rebuilt on publish | property tests in `model.rs` |
| `PreparedAnalysis` (`model.rs`) | index + row map + folds + hints + id map | doc-scoped; buffers move on publication instead of being copied | rebuilt | `model.rs` property tests |

## datagrid

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `grid::Core.rows: RefCell<Vec<Entity<RowView>>>` (`grid.rs`) | row-view pool | **bounded**: worst-case viewport + overscan (`vw::pool_size`), grown only to `pool` | created lazily, reused by slot | `pool is O(viewport)` (`tests/interaction.rs`), `recycled_slots_show_their_new_rows` |
| `grid::Core.shapes: Rc<RefCell<CellShapes>>` (`grid.rs`) | shaped cell text + line layouts | **bounded**: `CELL_CACHE_CAPACITY` = 1 024 entries (`grid.rs`), `CellShapes = lru::LruCache` (`grid.rs`) | LRU `put` during paint (`grid.rs`) | `long_multilingual_cells_have_bounded_labels_and_full_copy` (`tests/interaction.rs`) |
| `grid::Core.cell_text: RefCell<lru::LruCache<u64, SharedString>>` (`grid.rs`) | formatted cell text keyed by content hash | **bounded**: same 1 024-entry LRU | LRU | same test |
| `grid::Core.palette: RefCell<Option<(GuiraffeTheme, Palette)>>` (`grid.rs`) | resolved colour palette | **bounded**: one | replaced when the theme differs (`grid.rs`) | `palette_is_shared_across_row_recycling_and_refreshes_on_theme_change` (`tests/interaction.rs`) |
| `Model.undo: UndoLog` (`model/undo.rs`) | undo/redo cell diffs | **bounded**: `UNDO_LIMIT = 200` transactions (`model/undo.rs`), each ≤ `MAX_EDIT_CELLS = 100_000` | oldest dropped | undo unit tests (`model/undo.rs`) |
| `Model.layout: ColumnLayout` (`grid.rs`) | column placement | column count | recomputed on reorder/resize | `bands_and_display_order`, `reorder_moves` (`columns.rs`) |
| `order::RowOrder::Indices(Arc<[u32]>)` (`order.rs`) | view→source permutation | row count, 4 B/row | replaced on sort/filter; background build is cancellable (`order.rs`) | `cancelled_inverse_does_not_publish_a_partial_permutation`; allocator gate |
| `Model.sort` / `.filters` | view state | user-configured | user | — |

## devtools

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `FrameLog.samples` (`model.rs`) | rolling frame samples | **bounded**: `capacity` (panel uses 240, `view.rs`) | `pop_front` at capacity (`model.rs`) | `rolling_histogram_percentiles_and_empty_log` |
| `History.entries` (`state.rs`) | event snapshots (Arc-shared) | **bounded**: caller-supplied `capacity` (`inspect_store(.., capacity, ..)`) | oldest dropped by `Recorder` | `state.rs` (capacity = 2) |
| `state::StoreAdapter.cache: Option<(RowKey, Rc<Vec<EventRow>>)>` (`state.rs`) | last built row page | **bounded**: one | keyed by `(seq, len, offset, limit)` | none |
| `Shared.views: BTreeMap<SharedString, ViewInfo>` (`view.rs`) | instrumented view registry | **bounded**: template ids | `untrack` (`view.rs`) | `closed_tools_skip_tracking_and_opening_registers_views` |
| `Shared.stores` (`view.rs`) | store registrations | active caller registrations | `Devtools::remove_store` disconnects an adapter | `disconnected_stores_release_adapters_under_churn` |

## dnd

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `board::SortableBoard.focus: HashMap<usize, FocusHandle>` (`board.rs`) | focus handles per card id | card count | built once at construction | `focused card keeps its handle` behaviour tests in `board.rs` |
| `dock::SplitResize.sizes: Vec<Pixels>` | drag scratch | pane count | reused | none |
| `sortable::Selection.indices`, `Reorder.original_indices` | drag state | selection size | dropped with the drag | none |

## flamegraph

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `FlameLayout` (`model.rs`) — `rows`, `locations`, `extent` | laid-out spans + id→(depth, offset) index | doc-scoped; `Locations` is `Sequential` (dense `u32` packing, `model.rs`) with an `Arbitrary` HashMap fallback (`model.rs`) | rebuilt with the layout | `sequential_index_falls_back_and_checks_duplicates`, `packed_locations_keep_wide_and_reserved_values` |
| `Search.rows` (`model.rs`) | search hit rows | hit count | rebuilt per query | bucket-search property test (`model.rs`) |
| `LabelCache` (`view.rs`) | shaped frame labels | **bounded**: `LABEL_SLOTS = 128` direct-mapped slots, labels ≤ 256 B (`view.rs`) | slot replacement by hash | `crates/flamegraph/PRODUCTION.md` label-cache table |
| `FlameGraph.search` / `FlameGraph.tooltip` / `FlameGraph.pending_layout` | last search, tooltip entity, in-flight layout | **bounded**: one each | replaced / dropped | `crates/flamegraph/PRODUCTION.md` |

## flow / flow-core

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `paint::MeshCache.entries: BTreeMap<EdgeId, (Arc<EdgePath>, EdgeMesh)>` (`paint.rs`) | tessellated edge meshes | **bounded**: `MAX_EDGE_MESHES = 8192` and `MAX_EDGE_MESH_BYTES = 16 MiB` (`paint.rs`), checked in `get` (`paint.rs`); pruned to the visible set when admission-limited and the viewport moves (`paint.rs`) | `retain` on scene change (`sync`, `paint.rs`) plus those two limits | `review2_real_second_frame_over_2048_visible_edges_is_cached` (`paint.rs`), `review_10_second_frame_over_2048_edges_has_no_misses` |
| `paint::DeferredMeshCache.entries` + `.recency` (`paint.rs`) | tessellation for deferred (overview) scenes | **bounded**: `DEFERRED_MESH_ENTRIES = 1 024` / `DEFERRED_MESH_BYTES = 1 MiB` (`paint.rs`) | LRU by the `recency` tree (`paint.rs`); whole cache reset when the scene switches to deferred | `intermediate_zoom_uses_overview_only_for_dense_viewports` (`paint.rs`) |
| `paint::MeshCache.edge_buffer: Vec<EdgeId>` (`paint.rs`) | reused edge-id buffer | visible edges | transferred/cleared each frame, capacity kept | `review_12_frame_transfers_visible_edge_buffer` (`paint.rs`) |
| `paint::MeshCache.minimap` / `.grid` (`paint.rs`) | minimap meshes (≤ 8,100 vertices per layer, one retained representation) / one grid mesh per canvas size, spacing and background | **bounded**: one each, keyed by scene+size (`crates/flow/PRODUCTION.md` `Cache` table) | replaced | `dense_minimap_bounds_submitted_vertices` (`paint.rs`) |
| `FlowViewport.nodes: BTreeMap<NodeId, Entity<NodeCard>>` (`lib.rs`) | node card entities | **bounded**: visible nodes only | `retain` when a node leaves the viewport or the graph (`lib.rs`) | `ten_million_nodes_keep_detailed_viewport_work_constant` (`paint.rs`) |
| `FlowViewport.ordered_ids`, `.last_query_ids` (`lib.rs`) | visible ordering cache | visible nodes | replaced when the query result changes | same test |
| `FlowViewport.layout_preview` / `.order_scene` / `.order_selection` (`lib.rs`) | Arc snapshots used as change stamps | **bounded**: one each | replaced / `take()` | `background_layout_previews_then_commits_one_undoable_edit` (`tests.rs`) |
| `FlowViewport.index_job` (`lib.rs`) | in-flight background spatial-index build | **bounded**: one, with a cancellation token | dropped / superseded | `cancelling_background_layout_keeps_the_document` (`tests.rs`) |
| `FlowViewport.theme_cache` (`lib.rs`) | resolved theme | **bounded**: one | replaced on theme change | none |
| `interaction::Interaction.scratch: Vec<NodeId>` (`interaction.rs`) | hit-test and z-order scratch | visible nodes | reused, capacity kept | `review_12_retained_node_and_edge_buffer_survive_camera_frame` (`tests.rs`) |
| `flow-core::Editor.undo` / `.redo` (`editor.rs`) | full graph snapshots | **bounded**: `limit = 100` (`editor.rs`), trimmed on push and by `history_limit` (`editor.rs`) | oldest `pop_front` | `shrinking_history_limit_bounds_redo_as_well_as_undo` (`tests/behavior.rs`) |
| `flow-core::Scene` indexes (`scene.rs`) — `paths`, `incident`, `depths`, `revisions`, `children` (`im::OrdMap`) plus `SpatialIndex` (`scene.rs`) | derived geometry and BVHs | doc-scoped, structurally shared; edits recompute only changed keys | with the scene | `spatial_updates_match_linear_scan` (`tests/review_2.rs`), `review2_8_selection_only_edit_has_no_history_or_scene_change` |
| `flow-core::Name::common()` static (`model.rs`) | interned `Arc<str>` for the four names every document uses (`""`, `in`, `out`, `default`) | **bounded**: a fixed 4-entry `OnceLock` for the process lifetime | never (immutable static; values dropped when the last `Arc` clone goes) | `crates/flow-core/tests/memory.rs` (live-heap budget for the 50k/100k document) |
| `flow-core::Handles::defaults()` static (`model.rs`) | one shared `Arc<Vec<Handle>>` for the default `in`/`out` pair, copy-on-write on mutation | **bounded**: a single `OnceLock` for the process | never (immutable; `Handles` clones share the `Arc` until someone mutates) | same live-heap test; `Handles::ptr_eq` is the sharing predicate |
| `flow-core::Visible.nodes` / `.edges` (`scene.rs`) | last visibility result | visible only | replaced | — |

## icons / inputs / cli / macros / mocket / motion / router

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `router::History.entries` (`history.rs`) | navigation history | **bounded**: `cap` (default 64, `router.rs`) | oldest dropped on push | `push_back_forward_and_truncation` |
| `motion::Animated.start` | animation clock | **bounded**: one | cleared when settled | `Animated` tests |
| others | none found (icons embed assets at compile time; inputs/kbd/macros/mocket hold only per-call state) | — | — | — |

## layout

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `splitter::MemoryPersist.values` (`splitter.rs`) | caller-owned splitter settings | one value per saved key | overwritten by save, forgotten by `remove`, or released with store | `retired_keys_release_saved_values_under_churn` |
| `splitter::SplitterState.reported: RefCell<Vec<Pixels>>` (`splitter.rs`) | last reported pane sizes | pane count | overwritten | reported-dedup assertions (`splitter.rs`) |

## markdown

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `MarkdownState.document: Arc<Document>` + `.settled` (`view.rs`) | parsed outline (headings/fences/tasks) and the settled byte prefix | doc-scoped; only the unsettled tail is analysed (`view.rs`) | merged incrementally; replaced on `Change::Replace` | `incremental_metadata_matches_one_shot_at_every_character_boundary` (`streaming.rs`), `appended_chunks_extend_the_document_without_replacing_the_text_view` (`tests.rs`) |
| `MarkdownViewState.extensions` (`view.rs`) | text-element extensions | **bounded**: built once | never (feature-gated) | none |
| `Document.AnchorSlugs.seen` (`document.rs`) | slug de-duplication | heading count, per parse | with the parse | anchor tests |
| Kit text layout cache (vendored, outside this workspace) | shaped layouts across all text views | **bounded**: 4 096 per UI thread | Kit's own | documented in `crates/markdown/PRODUCTION.md` |

## mermaid

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `engine::Engine.cache: VecDeque<Entry>` (`engine.rs`) | drawn diagrams (source + SVG + parsed tree + raster) | **bounded**: `CACHE_ENTRIES = 16` and `CACHE_BYTES = 64 MiB` (`engine.rs`), enforced at `engine.rs` | FIFO `pop_back` after each render | `cache_evicts_beyond_its_entry_limit`, `cache_eviction_keeps_the_most_recent_entry` (`engine.rs`) |
| `MermaidViewState.engine` (`view.rs`) | the cache above, lent to the background task | **bounded**: one | `take()`/restore around each render | `engine_rasterizes_and_the_second_render_reuses_layout_and_raster` |
| `MermaidViewState.published` / `.preview` (`view.rs`) | current raster image / transient overview preview | **bounded**: one each | replaced or dropped on refinement | `crates/mermaid/PRODUCTION.md` cache-bound table |
| `Overview` buffers (`preview.rs`) | overview geometry | **bounded**: `MAX_PRIMITIVES = 2 048` (`preview.rs`) | rebuilt per preview | `crates/mermaid/PRODUCTION.md` |
| `source::CacheKey` | identity (hash + theme + font size) | n/a | n/a | `cache_key_separates_theme_and_font_size` |

## overlays

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `host::OverlayHost.dialogs/dialog_order/confirms/focus_chain` (`host.rs`) | open managed dialogs | **bounded**: number of open overlays | removed on close/dismiss (`host.rs`) | dialog tests |
| `host::OverlayHost.contexts` (`host.rs`) | named builder registrations | one per registered window/name pair | replaced by registration, released by `remove_context` or window close | `removed_contexts_release_builders_under_churn` |
| `host::OverlayHost.cards: HashMap<ToastId, ToastCard>` + `owned_notes` (`host.rs`) | toast cards | **bounded**: `retain` against the live notes each sync (`host.rs`) | `retain` | toast tests |
| `toast::ToastQueue.visible` (`toast.rs`) | visible toasts | **bounded**: `limit` (default 5) | `promote`/`advance` | `fifo_and_fresh_clock`, `set_limit` tests |
| `toast::ToastQueue.queued` (`toast.rs`) | caller-requested backlog | one entry per pending unique toast ID outside the visible limit | FIFO promotion, explicit removal or clear | `backlog_tracks_pending_requests_through_churn` |
| `search::SearchIndex` (`search.rs`) | normalised command text + reusable DP buffers + last result | **bounded**: entry count, `scores` pre-sized, one cached query | rebuilt via `new`; result cache replaced per query | `unicode_and_rejection`, `contiguous_preferred`, `ranking_keywords_disabled_and_limit`, `review_9_best_alignment_can_skip_an_earlier_match` |
| `spotlight::Spotlight.matches` | last match list | `limit` (50) | replaced | none |
| `stack::OverlayStack.entries` | open overlay stack | **bounded**: open overlays | `retain` on dismiss | dismissal tests |

## showcase / gallery

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `Showcase.demos: HashMap<usize, AnyView>` (`shell.rs`) | visited demo views | **bounded**: demo count | never (session app) | none |
| `EventLines.lines: VecDeque` (`events.rs`) | event log | **bounded**: `CAPACITY = 5` (`events.rs`) | `truncate` | `keeps_newest_first_and_counts_all` |
| `gallery::shell::Gallery.pages: HashMap<Page, AnyView>` (`shell.rs`) | visited pages | **bounded**: page count | never (session app) | — |

## state

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `store::MemoSlot.last_input` (`store.rs`) | memo input snapshot (skips recompute) | **bounded**: one per memoised selection | replaced | memo tests in `store.rs` |
| `store::Inner.slots: Vec<Rc<dyn Slot<S>>>` | live selector/memo slots | live selections | `retain(alive)` before push and on flush (`store.rs`) | `state` GPUI tests |
| `store::Inner.debounce: HashMap<&'static str, u64>` | per-key debounce counters | statically-known keys | never (bounded by key literals) | none |
| `engine::Undo.past` / `.future` (`engine.rs`) | state snapshots | **bounded**: caller `limit`, trimmed on push | `pop_front` | `undo_limit_zero_disables_and_reenabling_clears` |
| `engine::Engine.queue` (`engine.rs`) | event queue | per flush; `cascade_limit` (10 000) caps a dispatch loop | drained each dispatch | `cascade_limit_stops_dispatch_loops` |

## timeline

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `DopeModel.summaries: BTreeMap<usize, Vec<f64>>` (`dope_model.rs`) | per-group-channel key summary | one sorted vector per *group* channel only; rebuilt by `rebuild_summaries` (individual channels are not indexed) | rebuilt on any structural edit | `summaries_do_not_index_group_metadata` |
| `DopeModel.rows: Vec<ChannelRow>` (`dope_model.rs`) | flattened visible channel rows | doc-scoped (visible channels) | rebuilt on structural edit | `hierarchy_and_atomic_selection_moves` |
| `GanttView.geometry: Rc<Geometry>` (`gantt_view.rs`) — `rows`, `deps` | per-row schedule snapshot and dependency pairs, shared with render closures by `Rc` (no per-frame copies) | doc-scoped (task count) | rebuilt only when tasks or the schedule change | `crates/timeline/PRODUCTION.md` |
| `TracksView.tracks` / `.locked` (`tracks_view.rs`) | track data and lock set | doc-scoped | user edits | `crates/timeline/PRODUCTION.md` |
| `BarCurveView.extrema: Vec<(f64, f64)>` (`bar_view.rs`) | per-column min/max pairs for the dense bar editor | **bounded**: `min(width.ceil(), 16_384)` columns (`bar_view.rs`), 256 KiB of `f64` pairs | `reserve_exact` to the current column count; cleared when the scale changes | `crates/timeline/PRODUCTION.md` (§3 bullet) |
| Dope sheet header views | one cached header view per visible row (including partial rows) | **bounded**: visible rows | reused across pan/zoom/key edits | `crates/timeline/PRODUCTION.md` |
| Tracks overview geometry (`tracks_view.rs`) | no retained overview geometry: it reads the sorted disjoint clip intervals and emits at most one mark per logical pixel column | **bounded**: 16,384 pixel columns, built per paint | n/a (nothing retained between frames) | `crates/timeline/PRODUCTION.md` |
| `SplitterState`-style drag scratch in views (`drag`, `pending_fit`, `hover`, `selected`) | interaction state | **bounded**: one each per view | cleared when the gesture ends | view interaction tests |
| `EventModel.placements` / `.incident` / `.max_end` / `.lanes` (`event_layout.rs`) | validation/index pass over events and links (`id → Placement`, per-event incident link slots, per-lane placement runs) | doc-scoped, built once per `EventModel::new` and immutable afterwards | replaced with the model | `interval_index_retains_connectors_with_offscreen_endpoints` (`event_layout.rs` tests) |
| `EventTimelineState.theme_cache: Option<Rc<TimelineTheme>>` (`event_view.rs`) | resolved theme plus per-`ColorName` track colours | **bounded**: one, and only while `GuiraffeTheme` is unchanged | replaced when the theme differs | `crates/timeline/PRODUCTION.md` |


## tree

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `TreeModel.rows: Rows` (`model.rs`), `Rows` (`rows.rs`) — `blocks` / `free` arena, `IdIndex.pages` (`rows.rs`), `wide_siblings` | implicit-treap row blocks with a per-id rank index | **bounded**: visible rows | rebuilt on `refresh`; edits splice incrementally instead of rebuilding the index | `cross_block_splices_preserve_ranks_positions_and_order` (`rows.rs`) |
| `TreeModel.snapshot: OnceLock<Vec<Row>>` (`model.rs`) | materialised contiguous row copy | **bounded**: visible rows, only after a structural edit | `take()` on `refresh` (`model.rs`) | none |
| `TreeModel.expanded` / `.selected` (`model.rs`) | user state by ID | expanded or selected IDs whose removal the owner has yet to report | collapse/deselect or batch `forget_removed`; custom sources must report permanent deletion to reclaim IDs; DataUi preserves absent state only beneath an unloaded ancestor | `deleted_ids_release_state_under_churn`, `refresh_preserves_state_when_nodes_temporarily_disappear`, `document_tree_and_property_grid_release_replaced_rows`, `document_rows_keep_only_state_vouched_for_by_unloaded_ancestors` |
| `TreeModel.pending` (`model.rs`) | ids awaiting an async child load | requested loads | `retain` on refresh (`model.rs`) | `keyboard_expand_scroll_and_validation` |

## ui / core / commands / windows / test

| type / field | caches | bound | eviction | test |
|---|---|---|---|---|
| `ui::controls::ControlCache.scopes` + `.nodes` (`controls.rs`) | native control state per (node, item scope) | **bounded**: live template nodes × live items | `remove_scope` on item disposal (`host.rs`), `clear` on template swap (`host.rs`) | none directly |
| `ui::host::Shared.bound` / `.repeats` / `.items` (`host.rs`) | bound-node entities, repeat models, item scopes | **bounded**: live template/repeat items | `dispose_item` (`host.rs`), `clear` on template swap | none |
| `ui::host::Shared.renders` / `.buf` / `.changed` (`host.rs`) | per-node render counters, reused patch buffer, changed-slot list | template size | `clear` after each apply (`host.rs`) | none |
| `ui::repeat::KeyedList.candidates` (`repeat.rs`) | unkeyed fallback positions | rows of the current list | pruned when emptied (`repeat.rs`) | `positional_field_edits_keep_unkeyed_and_duplicate_input_state`, `duplicate_candidates_stay_live_across_mixed_splices` (`repeat.rs`) |
| `ui::slots::SlotTable` (`slots.rs`) | slot values, versions, styles | template size | replaced by patches | `patches_apply_and_noops_dont_count`, `migrate_keeps_values_by_name` |
| `ui::template::Template.ids` / `slot_nodes` / `Names.slot_index` / `.event_index` (`template.rs`) | name→id maps and slot→node indices | template size | with the template | compile tests |
| `ui::widget::Registry.widgets` (`widget.rs`) | built-in widget registry | registered kinds | never (static set) | catalogue tests |
| `ui::macro_support::Cached` / `CacheView` (`macro_support.rs`) | keyed view per call site via `window.use_keyed_state` | call sites | framework-managed with the window | render-counter tests |
| `core::variant::CACHE` (thread-local, `variant.rs`, `SLOTS = 4`) | resolved colour variants | **bounded**: 4 contexts × `VARIANTS`(7) × `COLORS` cells | round-robin slot replacement | `all_variants_legible` |
| `Registry.index` (`model.rs`) | canonical binding → command ids | command count | `index_remove` on unregister/update (`model.rs`) | `binding_index_follows_every_mutation`, `mutations_are_change_checked_and_reindex` |
| `commands::service::RegistryState.rows` / `.flat_rows` (`service.rs`) | menu projections | **bounded**: one per revision × 2 shapes | reset on revision bump (`service.rs`) | no test (`service.rs` has no test module) |
| `commands::service::Configuration.routes` (`service.rs`) | route → binding count | live bindings | decremented, removed at zero (`swap_routes`, `service.rs`) | no test |
| `commands::context::ContextStack.frames` | context stack | nesting depth | `pop` / scope drop | `deepest_matching_context_wins_over_catalog_order` |
| `windows::manager::WindowManager.entries` / `.order` / `.bus` (`manager.rs`) | open windows, order, cross-window subscriptions | **bounded**: open windows / subscribers | `remove` prunes all three (`manager.rs`) | no unit test in `manager.rs` |
| `windows::layout_store::LayoutStore` | on-disk window layout | one file | overwritten atomically | `missing_then_roundtrip`, `corrupt_file_is_set_aside_each_time_without_clobbering` |
| `test::*` (`NodeIndex`, `Frame`, `EventSink`, `CallSocket`) | inspection/test scaffolding | per call | dropped with the report | — (test support) |

## Notes

- Every crate with a `PRODUCTION.md` (charts, chat, code, datagrid, flamegraph, flow, markdown, mermaid,
  timeline, tree) states per-item overhead claims under §3; the **bounded** rows here match those claims.
- `flow::MeshCache.entries` and `flamegraph::FlameLayout` are not item-capped because the scene/document
  *is* the bound, but both are either byte-capped (flow: 8192 meshes / 16 MiB) or structurally shared, so
  neither can drift. That is the property spec/23 §3 asks for.

## Link to the PERF-0.2 diagnoses

Numbers are the foreman's quiet-machine baseline in [`plans/PERF-0.2.md` §Baseline](PERF-0.2.md).

- **Chat, 4.49 GiB / 130 s mount.** The retained-state picture changed with the release merge: the
  expensive per-message `TextViewState` is now bounded (64 rows, `history.rs`) and rebuilt from raw text
  on return, locked by `traversing_history_evicts_distant_text_and_restores_rows`. What remains per
  message is `Row::Cold { data, .. }` plus one `usize` in `ChatState.messages`, so the 4.5 GiB is no
  longer "a full markdown tree per message"; re-measuring before attributing the remainder is worthwhile,
  and the cold-row cost per message wants a measured overhead figure (`Message` clones its text).
- **Markdown, 799 MiB / 4.7 s mount.** `MarkdownState` analyses only the unsettled tail
  (`view.rs`, covered at every character split by the streaming property test). No unbounded
  retained cache exists on this path: `code::ShapeCache` is capped at 2 048 entries / 256 KiB and Kit's
  per-thread layout cache at 4 096. The 799 MiB is live text/layout state and the published render tree.
- **Mermaid, 33 MiB, 31.8 s mount.** `Engine.cache` is capped (16 entries / 64 MiB) with eviction tests,
  and the preview primitives are capped at 2 048; the mount cost is third-party layout. Zoom re-raster is
  a new raster for the same entry, not a new cache entry.
- **Flow, RSS +30 % over 10 s.** Both mesh caches are capped (8192 meshes / 16 MiB, and 1 024 deferred
  meshes / 1 MiB) and node cards are `retain`ed to the visible set, so no unbounded retained cache was
  found in `flow`/`flow-core`. The growth is more likely GPUI paint-side state retained by the element
  tree, which is outside this inventory's scope.
