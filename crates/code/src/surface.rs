use crate::paint::ShapeCache;
use crate::*;
use crate::{Decorations, Position};
use gpui_kit::*;
use guiraffe_core::{GuiraffeTheme, Scheme, Size, StyleContext};
use guiraffe_markdown::{MarkdownState, MarkdownView};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

/// Pointer action over a layered decoration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecorationAction {
    Hover,
    Leave,
    Click,
}

/// A stable identity and typed payload for one decoration interaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecorationEvent {
    pub action: DecorationAction,
    pub layer: DecorationLayerId,
    pub decoration: DecorationId,
    pub style: DecorationStyle,
}

pub type DecorationHandler = Rc<dyn Fn(DecorationEvent, &mut App)>;

/// Payload delivered when a visible inter-line widget needs content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockWidget {
    pub layer: DecorationLayerId,
    pub decoration: DecorationId,
    pub payload: Arc<str>,
}

pub type BlockWidgetRenderer = Rc<dyn Fn(BlockWidget, &mut Window, &mut App) -> AnyElement>;

#[derive(Default)]
struct BlockLayout {
    rows: BTreeMap<u64, usize>,
    keys: BTreeMap<u64, (DecorationLayerId, DecorationId)>,
    measured: std::collections::BTreeSet<u64>,
    pending: BTreeMap<u64, usize>,
    generation: u64,
    size: Option<(u32, u32)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DecorationTarget {
    Text,
    Gutter(u8),
    Blame,
    Lens,
    Outside,
}

enum AnalysisRequest {
    Source(Arc<str>, Arc<str>, bool),
    Document(Arc<CodeDocument>, Arc<str>, bool),
}
impl AnalysisRequest {
    fn compute(self, cancelled: impl Fn() -> bool) -> Option<Highlighted> {
        let (document, language, dark) = match self {
            Self::Source(source, language, dark) => (
                Arc::new(CodeDocument::build(source, &cancelled)?),
                language,
                dark,
            ),
            Self::Document(document, language, dark) => (document, language, dark),
        };
        Highlighted::compute_cancellable(document, &language, dark, cancelled)
    }
}

// Presentation supplied after set_source belongs to that source, even while the
// previously published document is still visible. Each replacement source resets it.
type DocumentPresentation = (PreparedAnalysis, Option<Arc<Decorations>>);
type BuildDocumentDecorations = Box<dyn FnOnce(Arc<CodeDocument>) -> DocumentPresentation>;

struct PendingPresentation {
    revision: u64,
    source: Arc<str>,
    annotations: Arc<[Annotation]>,
    folds: Vec<Fold>,
    traces: Arc<[Trace]>,
    prepared: Option<PreparedAnalysis>,
    document_decorations: Option<BuildDocumentDecorations>,
}

pub type SymbolProvider = Rc<dyn Fn(Position) -> Option<SymbolInfo>>;
pub type DefinitionHandler = Rc<dyn Fn(SourceRange, &mut Window, &mut App)>;

/// Layout defaults inherit the enclosing StyleContext; explicit size is an escape hatch.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeOptions {
    pub size: Option<Size>,
    pub line_numbers: bool,
    pub minimap: bool,
    /// Width of analysis cards in font cells. Zero hides the margin and layered hover cards.
    pub margin_chars: usize,
}
impl Default for CodeOptions {
    fn default() -> Self {
        Self {
            size: None,
            line_numbers: true,
            minimap: true,
            margin_chars: 24,
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CodeStats {
    pub text_renders: u64,
    pub overlay_renders: u64,
    pub shaped_lines: u64,
    pub cache_hits: u64,
    pub last_visible_rows: usize,
    pub last_truncated_rows: usize,
    pub max_shaped_bytes: usize,
    pub visible_bubbles: usize,
    pub analyses_started: u64,
    pub stale_analyses: u64,
}
#[derive(Default)]
pub(crate) struct Geometry {
    pub bounds: Bounds<Pixels>,
    pub line_height: f32,
    pub font_size: f32,
    pub cell_width: f32,
    pub gutter: f32,
    pub margin: f32,
    pub mini: f32,
    pub shaped: std::collections::HashMap<usize, Rc<ShapedLine>>,
    pub font: Font,
    pub content_width: f64,
}
pub(crate) struct Shared {
    pub id: u64,
    pub highlighted: RefCell<Highlighted>,
    pub annotations: RefCell<Arc<[Annotation]>>,
    pub decorations: RefCell<Option<Arc<Decorations>>>,
    decoration_handler: RefCell<Option<DecorationHandler>>,
    hover_decoration: Cell<Option<(DecorationLayerId, DecorationId)>>,
    hover_target: Cell<Option<DecorationTarget>>,
    text_annotations: RefCell<Arc<[Annotation]>>,
    pub has_range_styling: Cell<bool>,
    has_bubbles: Cell<bool>,
    pub hints: RefCell<BTreeMap<usize, DisplayLine>>,
    pub index: RefCell<IntervalIndex>,
    pub annotation_ids: RefCell<BTreeMap<u64, usize>>,
    block_layout: RefCell<BlockLayout>,
    block_renderer: RefCell<Option<BlockWidgetRenderer>>,
    text_view: RefCell<Option<WeakEntity<TextLayer>>>,
    pub rows: RefCell<RowMap>,
    pub folds: RefCell<Vec<Fold>>,
    pub gutters: RefCell<Vec<GutterColumn>>,
    pub options: RefCell<CodeOptions>,
    pub viewport: Cell<Viewport>,
    pub geometry: RefCell<Geometry>,
    pub stats: Cell<CodeStats>,
    pub selection: Cell<Option<SourceRange>>,
    pub dragging: Cell<Option<Position>>,
    /// A minimap drag is in progress (pointer pressed on the minimap).
    pub minimap_drag: Cell<bool>,
    pub symbol: RefCell<Option<SymbolInfo>>,
    pub provider: RefCell<Option<SymbolProvider>>,
    pub definition: RefCell<Option<DefinitionHandler>>,
    pub hover: Cell<Option<Position>>,
    pub bubbles: RefCell<BTreeMap<u64, Entity<MarkdownState>>>,
    review_hover: RefCell<Option<Entity<MarkdownState>>>,
    pub hover_markdown: RefCell<Option<Entity<MarkdownState>>>,
    pub traces: RefCell<Arc<[Trace]>>,
    pub dependents: RefCell<Vec<WeakEntity<TraceView>>>,
    pub revision: Cell<u64>,
    pending: RefCell<Option<AnalysisRequest>>,
    analysis_generation: Arc<AtomicU64>,
    analyzing: Cell<bool>,
    presentation: RefCell<Option<PendingPresentation>>,
    pub text_epoch: Cell<u64>,
    pub overlay_view: RefCell<Option<WeakEntity<OverlayLayer>>>,
    pub focus: FocusHandle,
    pub menu: guiraffe_core::TextMenu,
}
/// Composite control handle. All updates publish immutable data or pure leaf notifications;
/// hosts must keep this handle outside their render state to retain unrelated ancestors.
#[derive(Clone)]
pub struct CodeHandle {
    pub(crate) shared: Rc<Shared>,
    pub(crate) text: Entity<TextLayer>,
    pub(crate) overlay: Entity<OverlayLayer>,
}
impl CodeHandle {
    pub fn new(id: u64, document: Arc<CodeDocument>, cx: &mut App) -> Self {
        crate::init(cx);
        let rows = RowMap::new(document.line_count(), &[], &[]);
        let shared = Rc::new(Shared {
            id,
            highlighted: RefCell::new(Highlighted::plain(document)),
            annotations: RefCell::new(Arc::from([])),
            decorations: RefCell::default(),
            decoration_handler: RefCell::default(),
            hover_decoration: Cell::default(),
            hover_target: Cell::default(),
            text_annotations: RefCell::new(Arc::from([])),
            has_range_styling: Cell::default(),
            has_bubbles: Cell::default(),
            hints: RefCell::default(),
            index: RefCell::new(IntervalIndex::default()),
            annotation_ids: RefCell::default(),
            block_layout: RefCell::default(),
            block_renderer: RefCell::default(),
            text_view: RefCell::default(),
            rows: RefCell::new(rows),
            folds: RefCell::default(),
            gutters: RefCell::default(),
            options: RefCell::new(CodeOptions::default()),
            viewport: Cell::default(),
            geometry: RefCell::default(),
            stats: Cell::default(),
            selection: Cell::default(),
            dragging: Cell::default(),
            minimap_drag: Cell::default(),
            symbol: RefCell::default(),
            provider: RefCell::default(),
            definition: RefCell::default(),
            hover: Cell::default(),
            bubbles: RefCell::default(),
            review_hover: RefCell::default(),
            hover_markdown: RefCell::default(),
            traces: RefCell::new(Arc::from([])),
            dependents: RefCell::default(),
            revision: Cell::default(),
            pending: RefCell::default(),
            analysis_generation: Arc::default(),
            analyzing: Cell::default(),
            presentation: RefCell::default(),
            text_epoch: Cell::default(),
            overlay_view: RefCell::new(None),
            focus: cx.focus_handle(),
            menu: guiraffe_core::TextMenu::new(),
        });
        let text = cx.new(|_| TextLayer {
            shared: shared.clone(),
            cache: Rc::new(RefCell::new(ShapeCache::default())),
        });
        let overlay = cx.new(|_| OverlayLayer {
            shared: shared.clone(),
        });
        *shared.overlay_view.borrow_mut() = Some(overlay.downgrade());
        *shared.text_view.borrow_mut() = Some(text.downgrade());
        Self {
            shared,
            text,
            overlay,
        }
    }
    pub fn id(&self) -> u64 {
        self.shared.id
    }
    /// Whether indexing or syntax highlighting is queued or running.
    pub fn is_analyzing(&self) -> bool {
        self.shared.analyzing.get()
    }
    pub fn highlight_error(&self) -> Option<Arc<str>> {
        self.shared.highlighted.borrow().error.clone()
    }
    pub fn document(&self) -> Arc<CodeDocument> {
        self.shared.highlighted.borrow().document.clone()
    }
    pub fn stats(&self) -> CodeStats {
        self.shared.stats.get()
    }
    pub fn viewport(&self) -> Viewport {
        self.shared.viewport.get()
    }
    pub fn selection(&self) -> Option<SourceRange> {
        self.shared.selection.get()
    }
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.shared.focus.focus(window, cx);
    }
    fn notify_overlay(&self, cx: &mut App) {
        cx.notify(self.overlay.entity_id());
        self.shared.dependents.borrow_mut().retain(|weak| {
            if weak.is_upgradable() {
                cx.notify(weak.entity_id());
                true
            } else {
                false
            }
        });
    }
    pub(crate) fn notify_all(&self, cx: &mut App) {
        cx.notify(self.text.entity_id());
        self.notify_overlay(cx);
    }
    pub fn set_options(&self, options: CodeOptions, cx: &mut App) {
        if *self.shared.options.borrow() != options {
            *self.shared.options.borrow_mut() = options;
            self.notify_all(cx);
        }
    }
    pub fn set_gutters(&self, gutters: Vec<GutterColumn>, cx: &mut App) {
        if *self.shared.gutters.borrow() != gutters {
            *self.shared.gutters.borrow_mut() = gutters;
            self.shared
                .text_epoch
                .set(self.shared.text_epoch.get().wrapping_add(1));
            self.notify_all(cx);
        }
    }
    /// Publish layered decorations prepared for the currently displayed document.
    /// Returns false if the source revision has changed. Pass `None` to clear the overlay.
    pub fn set_decorations(&self, next: Option<Arc<Decorations>>, cx: &mut App) -> bool {
        if next
            .as_ref()
            .is_some_and(|next| !Arc::ptr_eq(next.document(), &self.document()))
        {
            return false;
        }
        let current = self.shared.decorations.borrow();
        let unchanged = match (current.as_ref(), next.as_ref()) {
            (Some(current), Some(next)) => Arc::ptr_eq(current, next),
            (None, None) => true,
            _ => false,
        };
        drop(current);
        if !unchanged {
            *self.shared.decorations.borrow_mut() = next;
            self.shared.hover_decoration.set(None);
            self.shared.review_hover.borrow_mut().take();
            self.shared.hover_target.set(None);
            self.notify_all(cx);
        }
        true
    }

    pub(crate) fn set_document_decorations(
        &self,
        build: impl FnOnce(Arc<CodeDocument>) -> DocumentPresentation + 'static,
        cx: &mut App,
    ) {
        {
            let mut presentation = self.shared.presentation.borrow_mut();
            if let Some(pending) = presentation.as_mut() {
                pending.document_decorations = Some(Box::new(build));
                return;
            }
        }
        self.publish_document_decorations(build(self.document()), cx);
    }

    fn publish_document_decorations(
        &self,
        (prepared, decorations): DocumentPresentation,
        cx: &mut App,
    ) {
        self.set_analysis(prepared, cx);
        self.set_decorations(decorations, cx);
    }

    /// Set the renderer for inter-line block decorations. The default displays the payload
    /// on a themed surface. Changing renderers remeasures visible widgets on their next layout.
    pub fn on_block_widget(&self, renderer: Option<BlockWidgetRenderer>, cx: &mut App) {
        *self.shared.block_renderer.borrow_mut() = renderer;
        let mut layout = self.shared.block_layout.borrow_mut();
        layout.generation = layout.generation.wrapping_add(1);
        layout.measured.clear();
        layout.pending.clear();
        drop(layout);
        self.notify_overlay(cx);
    }

    /// Register a callback for layered decoration hover and click actions.
    pub fn on_decoration(&self, handler: Option<DecorationHandler>) {
        *self.shared.decoration_handler.borrow_mut() = handler;
    }

    fn decoration_target(&self, point: Point<Pixels>) -> DecorationTarget {
        let geometry = self.shared.geometry.borrow();
        let x = (point.x - geometry.bounds.origin.x).as_f32();
        if x >= geometry.gutter {
            if x < geometry.bounds.size.width.as_f32() - geometry.margin - geometry.mini {
                return DecorationTarget::Text;
            }
            return DecorationTarget::Outside;
        }
        let data = self.shared.decorations.borrow();
        let Some(data) = data.as_ref() else {
            return DecorationTarget::Outside;
        };
        let blame = if data.has_blame() {
            29. * geometry.cell_width
        } else {
            0.
        };
        let edge = geometry.gutter - blame;
        if x >= edge && data.has_blame() {
            return DecorationTarget::Blame;
        }
        let lane_width = 2. * geometry.cell_width;
        let first = edge - f32::from(data.gutter_lanes()) * lane_width;
        if x >= first && lane_width > 0. {
            return DecorationTarget::Gutter(((x - first) / lane_width) as u8);
        }
        DecorationTarget::Outside
    }

    fn lens_position(&self, point: Point<Pixels>) -> Option<Position> {
        let geometry = self.shared.geometry.borrow();
        if geometry.line_height <= 0.
            || !geometry.bounds.contains(&point)
            || (point.x - geometry.bounds.origin.x).as_f32() < geometry.gutter
        {
            return None;
        }
        let y = f64::from((point.y - geometry.bounds.origin.y).as_f32()) + self.viewport().y;
        let row = (y / f64::from(geometry.line_height)).floor() as usize;
        let Row::Lens(index) = self.shared.rows.borrow().row(row)? else {
            return None;
        };
        self.shared
            .annotations
            .borrow()
            .get(index)
            .map(|a| a.range.start)
    }

    fn decoration_at(
        &self,
        position: Option<Position>,
        target: DecorationTarget,
    ) -> Option<(DecorationLayerId, DecorationId)> {
        let state = self.shared.decorations.borrow();
        let decorations = state.as_ref()?;
        if target == DecorationTarget::Outside {
            return None;
        }
        let mut position = position?;
        if matches!(
            target,
            DecorationTarget::Blame | DecorationTarget::Gutter(_)
        ) {
            position.column = 0;
        }
        let bytes = decorations.document().offset(position)? as u64;
        let mut best = None;
        for (id, layer) in decorations.layers() {
            let mut hit = None;
            layer.query_limited(bytes..bytes + 1, 64, |mark| {
                let matches = match (target, decorations.style(mark.style)) {
                    (
                        DecorationTarget::Gutter(lane),
                        Some(DecorationStyle::Gutter { lane: style, .. }),
                    ) => lane == (*style).min(3),
                    (DecorationTarget::Blame, Some(DecorationStyle::Blame(_))) => true,
                    (
                        DecorationTarget::Lens,
                        Some(DecorationStyle::Inline {
                            widget: InlineWidget::CodeLens,
                            ..
                        }),
                    ) => true,

                    (
                        DecorationTarget::Text,
                        Some(DecorationStyle::Blame(_) | DecorationStyle::Gutter { .. }),
                    ) => false,
                    (DecorationTarget::Text, Some(_)) => true,
                    _ => false,
                };
                if matches && hit.is_none_or(|previous| mark.id < previous) {
                    hit = Some(mark.id);
                }
            });
            if let Some(hit) = hit {
                best = Some((id, hit));
            }
        }
        best
    }

    fn emit_decoration(
        &self,
        key: (DecorationLayerId, DecorationId),
        action: DecorationAction,
        cx: &mut App,
    ) {
        emit_decoration_event(&self.shared, key, action, cx);
    }

    /// Move the caret to the adjacent diagnostic. Returns false at either end.
    pub fn navigate_diagnostic(&self, previous: bool, cx: &mut App) -> bool {
        self.navigate_decoration(previous, false, cx)
    }

    /// Move the caret to the adjacent diff mark. Returns false at either end.
    pub fn navigate_change(&self, previous: bool, cx: &mut App) -> bool {
        self.navigate_decoration(previous, true, cx)
    }

    fn navigate_decoration(&self, reverse: bool, change: bool, cx: &mut App) -> bool {
        let next = {
            let state = self.shared.decorations.borrow();
            let Some(data) = state.as_ref() else {
                return false;
            };
            let document = data.document();
            let caret = self
                .selection()
                .and_then(|range| document.offset(range.start));
            let navigation = data.layers().filter_map(|(_, layer)| {
                layer
                    .adjacent(caret.map(|offset| offset as u32), reverse, change)
                    .map(|mark| mark.start)
            });
            let next = if reverse {
                navigation.max()
            } else {
                navigation.min()
            };
            next.and_then(|offset| document.position(offset as usize))
        };
        let Some(next) = next else {
            return false;
        };
        self.select(Some(SourceRange::new(next, next)), cx);
        self.scroll_to_line(next.line, cx);
        true
    }

    /// Convenience for small analysis batches. Large producers should build PreparedAnalysis
    /// on their background worker and publish it with set_analysis instead.
    pub fn set_annotations(&self, annotations: Arc<[Annotation]>, cx: &mut App) {
        if let Some(pending) = self.shared.presentation.borrow_mut().as_mut() {
            if let Some(prepared) = pending.prepared.take() {
                pending.folds = prepared.folds;
            }
            pending.annotations = annotations;
            return;
        }
        if *self.shared.annotations.borrow() == annotations {
            return;
        }
        let prepared = PreparedAnalysis::new(
            self.document(),
            annotations,
            self.shared.folds.borrow().clone(),
        );
        self.set_analysis(prepared, cx);
    }
    /// Move precomputed buffers onto the UI thread. False means source was replaced while
    /// the analysis was running; no state is changed in that case.
    pub fn set_analysis(&self, mut prepared: PreparedAnalysis, cx: &mut App) -> bool {
        if let Some(pending) = self.shared.presentation.borrow_mut().as_mut() {
            if !Arc::ptr_eq(prepared.document.source(), &pending.source) {
                return false;
            }
            pending.prepared = Some(prepared);
            return true;
        }
        if !Arc::ptr_eq(&self.document(), &prepared.document) {
            return false;
        }
        let mut blocks = self.shared.block_layout.borrow_mut();
        let previous_annotations = self.shared.annotations.borrow();
        let previous_ids = self.shared.annotation_ids.borrow();
        let same_payload = |old_id: &u64, new_id: &u64| {
            let old = previous_ids
                .get(old_id)
                .and_then(|&i| previous_annotations.get(i));
            let new = prepared
                .ids
                .get(new_id)
                .and_then(|&i| prepared.annotations.get(i));
            matches!((old, new), (Some(old), Some(new)) if old.payload == new.payload)
        };
        let previous_blocks: BTreeMap<_, _> =
            blocks.keys.iter().map(|(id, key)| (*key, *id)).collect();
        let mut reused_rows = false;
        // An unrelated decoration update must not jump back to estimated widget heights.
        // The next layout still measures the widgets at their current width.
        for (id, rows) in &mut prepared.block_rows {
            if let Some(key) = prepared.block_keys.get(id)
                && let Some(old_id) = previous_blocks.get(key)
                && same_payload(old_id, id)
                && let Some(previous) = blocks.rows.get(old_id)
                && rows != previous
            {
                *rows = *previous;
                reused_rows = true;
            }
        }
        if reused_rows {
            prepared.rows = RowMap::with_blocks(
                prepared.document.line_count(),
                &prepared.folds,
                &prepared.annotations,
                &prepared.block_rows,
            );
        }
        self.shared
            .bubbles
            .borrow_mut()
            .retain(|id, _| same_payload(id, id));
        drop(previous_ids);
        drop(previous_annotations);
        let text_changed = blocks.rows != prepared.block_rows
            || blocks.keys != prepared.block_keys
            || *self.shared.text_annotations.borrow() != prepared.text
            || *self.shared.folds.borrow() != prepared.folds;
        *blocks = BlockLayout {
            rows: prepared.block_rows,
            keys: prepared.block_keys,
            generation: blocks.generation.wrapping_add(1),
            ..Default::default()
        };
        drop(blocks);
        *self.shared.text_annotations.borrow_mut() = prepared.text;
        self.shared
            .has_range_styling
            .set(prepared.has_range_styling);
        self.shared.has_bubbles.set(prepared.has_bubbles);
        *self.shared.index.borrow_mut() = prepared.index;
        *self.shared.rows.borrow_mut() = prepared.rows;
        *self.shared.folds.borrow_mut() = prepared.folds;
        *self.shared.hints.borrow_mut() = prepared.hints;
        *self.shared.annotation_ids.borrow_mut() = prepared.ids;
        *self.shared.annotations.borrow_mut() = prepared.annotations;
        if text_changed {
            self.shared
                .text_epoch
                .set(self.shared.text_epoch.get().wrapping_add(1));
            cx.notify(self.text.entity_id());
        }
        self.notify_overlay(cx);
        true
    }
    pub fn set_folds(&self, mut folds: Vec<Fold>, cx: &mut App) {
        folds.sort_by_key(|f| f.start);
        if let Some(pending) = self.shared.presentation.borrow_mut().as_mut() {
            if let Some(prepared) = pending.prepared.as_mut() {
                prepared.replace_folds(folds.clone());
            }
            pending.folds = folds;
            return;
        }
        if *self.shared.folds.borrow() == folds {
            return;
        }
        *self.shared.rows.borrow_mut() = RowMap::with_blocks(
            self.document().line_count(),
            &folds,
            &self.shared.annotations.borrow(),
            &self.shared.block_layout.borrow().rows,
        );
        *self.shared.folds.borrow_mut() = folds;
        self.shared
            .text_epoch
            .set(self.shared.text_epoch.get().wrapping_add(1));
        self.scroll_to(self.viewport(), cx);
        self.notify_all(cx);
    }
    pub fn toggle_fold(&self, line: usize, cx: &mut App) {
        let mut folds = self.shared.folds.borrow().clone();
        if let Some(fold) = folds.iter_mut().find(|f| f.start == line) {
            fold.collapsed = !fold.collapsed;
            self.set_folds(folds, cx);
        }
    }
    pub fn scroll_to(&self, viewport: Viewport, cx: &mut App) {
        let g = self.shared.geometry.borrow();
        let mut next = viewport.clamped(
            self.shared.rows.borrow().len(),
            g.line_height,
            g.bounds.size.height.as_f32(),
        );
        let width = (g.bounds.size.width.as_f32() - g.gutter - g.margin - g.mini).max(0.);
        next.x = next.x.min((g.content_width - f64::from(width)).max(0.));
        drop(g);
        if self.viewport() != next {
            self.shared.viewport.set(next);
            self.notify_all(cx);
        }
    }
    /// If `position` (window coordinates) is over the minimap, the source line it maps to.
    /// With `clamp`, positions outside the minimap's vertical extent clamp to its ends
    /// (used while dragging, so the pointer may leave the minimap).
    pub fn minimap_line_at(
        &self,
        position: gpui_kit::Point<gpui_kit::Pixels>,
        clamp: bool,
    ) -> Option<usize> {
        let g = self.shared.geometry.borrow();
        if g.mini <= 0. {
            return None;
        }
        let local = position - g.bounds.origin;
        let height = g.bounds.size.height.as_f32();
        if !clamp && local.x.as_f32() < g.bounds.size.width.as_f32() - g.mini {
            return None;
        }
        let y = local.y.as_f32().clamp(0., height.max(0.));
        let lines = self.document().line_count();
        let line = match crate::paint::minimap_line_height(lines, height) {
            Some(step) => (y / step) as usize,
            None => ((y / height.max(1.)) * lines as f32) as usize,
        };
        Some(line.min(lines.saturating_sub(1)))
    }

    pub fn scroll_to_line(&self, line: usize, cx: &mut App) {
        if let Some(row) = self.shared.rows.borrow().source_row_or_before(line) {
            let y = row as f64 * f64::from(self.shared.geometry.borrow().line_height);
            self.scroll_to(
                Viewport {
                    x: self.viewport().x,
                    y,
                },
                cx,
            );
        }
    }
    pub fn select(&self, range: Option<SourceRange>, cx: &mut App) {
        let range = range.filter(|r| self.document().range(*r).is_some());
        if self.shared.selection.replace(range) != range {
            self.notify_overlay(cx);
        }
    }
    pub fn copy(&self, cx: &mut App) {
        let doc = self.document();
        if let Some(text) = self
            .selection()
            .and_then(|r| doc.range(r))
            .filter(|r| !r.is_empty())
            .and_then(|r| doc.source().get(r))
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
        }
    }
    /// Copy the selection prefixed with one-based line numbers.
    pub fn copy_with_line_numbers(&self, cx: &mut App) {
        if let Some(text) = self
            .selection()
            .and_then(|r| self.document().with_line_numbers(r))
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
    /// Select the whole source.
    pub fn select_all(&self, cx: &mut App) {
        let doc = self.document();
        if let Some(end) = doc.position(doc.source().len()) {
            self.select(Some(SourceRange::new(Position::default(), end)), cx);
        }
    }
    /// The selection context menu (open state, entries), for hosts and tests.
    pub fn context_menu(&self) -> &guiraffe_core::TextMenu {
        &self.shared.menu
    }
    /// Open the selection context menu at `position` (window coordinates). Copy is disabled
    /// without a selection; "Go to definition" appears when a definition handler is set and a
    /// symbol is under `at`.
    pub fn open_context_menu(
        &self,
        position: Point<Pixels>,
        at: Option<Position>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let selected = self
            .selection()
            .is_some_and(|r| self.document().range(r).is_some_and(|r| !r.is_empty()));
        let (copy, all, numbered) = (self.clone(), self.clone(), self.clone());
        let mut spec = guiraffe_core::TextMenuSpec::new(
            selected,
            move |_, cx| copy.copy(cx),
            move |_, cx| all.select_all(cx),
        )
        .item(
            guiraffe_core::TextMenuItem::new("Copy with line numbers", move |_, cx| {
                numbered.copy_with_line_numbers(cx)
            })
            .enabled(selected),
        );
        let handler = self.shared.definition.borrow().clone();
        let symbol = at.and_then(|p| self.symbol_at(p));
        if let (Some(handler), Some(range)) = (handler, symbol) {
            spec = spec.item(guiraffe_core::TextMenuItem::new(
                "Go to definition",
                move |window, cx| handler(range, window, cx),
            ));
        }
        self.shared.menu.open(
            position,
            &spec,
            self.overlay.entity_id(),
            self.shared.focus.clone(),
            window,
            cx,
        );
    }
    fn symbol_at(&self, position: Position) -> Option<SourceRange> {
        if let Some(provider) = self.shared.provider.borrow().as_ref() {
            return provider(position).map(|s| s.range);
        }
        let doc = self.document();
        let offset = doc.offset(position)?;
        let mut hits = Vec::new();
        self.shared
            .index
            .borrow()
            .query(offset..offset.saturating_add(1), &mut hits);
        let annotations = self.shared.annotations.borrow();
        hits.iter()
            .filter_map(|i| annotations.get(*i))
            .find(|a| a.kind == AnnotationKind::Symbol)
            .map(|a| a.range)
    }
    /// Where a keyboard-opened menu appears: the selection start, else the text origin.
    fn menu_anchor(&self) -> Point<Pixels> {
        let at = self
            .selection()
            .and_then(|r| crate::paint::anchor(&self.shared, r.start));
        at.unwrap_or_else(|| {
            let g = self.shared.geometry.borrow();
            g.bounds.origin + point(px(g.gutter), px(g.line_height))
        })
    }
    pub fn set_traces(&self, traces: Arc<[Trace]>, cx: &mut App) {
        if let Some(pending) = self.shared.presentation.borrow_mut().as_mut() {
            pending.traces = traces;
            return;
        }
        if *self.shared.traces.borrow() != traces {
            *self.shared.traces.borrow_mut() = traces;
            self.notify_overlay(cx);
        }
    }
    pub fn set_symbol_provider(&self, provider: Option<SymbolProvider>, cx: &mut App) {
        *self.shared.provider.borrow_mut() = provider;
        let hover = self.shared.hover.replace(None);
        self.set_hover(hover, cx);
    }
    pub fn on_definition(&self, handler: Option<DefinitionHandler>) {
        *self.shared.definition.borrow_mut() = handler;
    }
    pub fn set_hover(&self, position: Option<Position>, cx: &mut App) {
        self.set_hover_target(position, DecorationTarget::Text, cx);
    }

    fn set_hover_target(&self, position: Option<Position>, target: DecorationTarget, cx: &mut App) {
        let unchanged_target = self.shared.hover_target.replace(Some(target)) == Some(target);
        let prior_position = self.shared.hover.replace(position);
        if prior_position == position && unchanged_target {
            return;
        }
        let next = self.decoration_at(position, target);
        let previous = self.shared.hover_decoration.replace(next);
        if previous != next {
            *self.shared.review_hover.borrow_mut() = next.and_then(|(layer, id)| {
                let decorations = self.shared.decorations.borrow();
                let decorations = decorations.as_ref()?;
                let mark = decorations.layer(layer)?.get(id)?;
                let DecorationStyle::Review { markdown } = decorations.style(mark.style)? else {
                    return None;
                };
                Some(cx.new(|cx| MarkdownState::new(markdown.to_string(), cx)))
            });
            if let Some(previous) = previous {
                self.emit_decoration(previous, DecorationAction::Leave, cx);
            }
            if let Some(next) = next {
                self.emit_decoration(next, DecorationAction::Hover, cx);
            }
            self.notify_overlay(cx);
        }
        if next.is_some()
            && previous == next
            && prior_position.map(|p| p.line) != position.map(|p| p.line)
        {
            self.notify_overlay(cx);
        }
        let symbol = position
            .filter(|_| target == DecorationTarget::Text)
            .and_then(|p| {
                if let Some(provider) = self.shared.provider.borrow().as_ref() {
                    return provider(p);
                }
                let doc = self.document();
                let offset = doc.offset(p)?;
                let mut hits = Vec::new();
                self.shared
                    .index
                    .borrow()
                    .query(offset..offset.saturating_add(1), &mut hits);
                let annotations = self.shared.annotations.borrow();
                let a = hits
                    .iter()
                    .filter_map(|i| annotations.get(*i))
                    .find(|a| a.kind == AnnotationKind::Symbol)?;
                Some(SymbolInfo {
                    range: a.range,
                    occurrences: Arc::from([a.range]),
                    markdown: a.payload.clone(),
                })
            });
        if *self.shared.symbol.borrow() != symbol {
            *self.shared.hover_markdown.borrow_mut() = symbol
                .as_ref()
                .map(|s| cx.new(|cx| MarkdownState::new(s.markdown.to_string(), cx)));
            *self.shared.symbol.borrow_mut() = symbol;
            self.notify_overlay(cx);
        }
    }
    /// Parse and index source off-thread; stale completions cannot replace newer source.
    /// Discrete immutable result hand-off uses the GPUI background task channel.
    pub fn set_source(&self, source: Arc<str>, language: Arc<str>, cx: &mut App) {
        let dark = GuiraffeTheme::global(cx).scheme == Scheme::Dark;
        self.queue(AnalysisRequest::Source(source, language, dark), false, cx);
    }
    pub fn highlight(&self, language: Arc<str>, cx: &mut App) {
        let source = self
            .shared
            .presentation
            .borrow()
            .as_ref()
            .map(|p| p.source.clone());
        let dark = GuiraffeTheme::global(cx).scheme == Scheme::Dark;
        let request = match source {
            Some(source) => AnalysisRequest::Source(source, language, dark),
            None => AnalysisRequest::Document(self.document(), language, dark),
        };
        self.queue(request, true, cx);
    }
    fn queue(&self, request: AnalysisRequest, preserve: bool, cx: &mut App) {
        let revision = self.shared.revision.get().wrapping_add(1);
        self.shared.revision.set(revision);
        self.shared
            .analysis_generation
            .store(revision, Ordering::Relaxed);
        if let AnalysisRequest::Source(source, _, _) = &request {
            let mut presentation = self.shared.presentation.borrow_mut();
            if preserve && let Some(pending) = presentation.as_mut() {
                pending.revision = revision;
            } else {
                *presentation = Some(PendingPresentation {
                    revision,
                    source: source.clone(),
                    annotations: Arc::from([]),
                    folds: Vec::new(),
                    traces: Arc::from([]),
                    prepared: None,
                    document_decorations: None,
                });
            }
        }
        *self.shared.pending.borrow_mut() = Some(request);
        if self.shared.analyzing.replace(true) {
            return;
        }
        let weak = Rc::downgrade(&self.shared);
        let text = self.text.downgrade();
        let overlay = self.overlay.downgrade();
        cx.spawn(async move |cx| {
            loop {
                let Some(shared) = weak.upgrade() else {
                    break;
                };
                let Some(request) = shared.pending.borrow_mut().take() else {
                    shared.analyzing.set(false);
                    break;
                };
                let revision = shared.revision.get();
                let mut stats = shared.stats.get();
                stats.analyses_started += 1;
                shared.stats.set(stats);
                let generation = Arc::downgrade(&shared.analysis_generation);
                drop(shared);
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        request.compute(|| {
                            generation
                                .upgrade()
                                .is_none_or(|current| current.load(Ordering::Relaxed) != revision)
                        })
                    })
                    .await;
                let keep_going = cx.update(|cx| {
                    let Some(shared) = weak.upgrade() else {
                        return false;
                    };
                    if let Some(result) = result.filter(|_| shared.revision.get() == revision)
                        && let (Some(text), Some(overlay)) = (text.upgrade(), overlay.upgrade())
                    {
                        CodeHandle {
                            shared: shared.clone(),
                            text,
                            overlay,
                        }
                        .publish(result, cx);
                    } else {
                        let mut stats = shared.stats.get();
                        stats.stale_analyses += 1;
                        shared.stats.set(stats);
                    }
                    let more = shared.pending.borrow().is_some();
                    if !more {
                        shared.analyzing.set(false);
                    }
                    more
                });
                if !keep_going {
                    break;
                }
            }
        })
        .detach();
    }
    /// Install a snapshot prepared by a host's own worker. Invalidate older pending jobs.
    pub fn set_highlighted(&self, snapshot: Highlighted, cx: &mut App) {
        self.shared.presentation.borrow_mut().take();
        self.shared.pending.borrow_mut().take();
        self.shared
            .revision
            .set(self.shared.revision.get().wrapping_add(1));
        self.shared
            .analysis_generation
            .store(self.shared.revision.get(), Ordering::Relaxed);
        self.publish(snapshot, cx);
    }
    /// Visible source anchor in window coordinates; folded/offscreen positions return None.
    pub fn anchor_in_window(&self, position: Position) -> Option<Point<Pixels>> {
        crate::paint::anchor(&self.shared, position)
    }
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.shared.geometry.borrow().bounds
    }
    fn publish(&self, mut result: Highlighted, cx: &mut App) {
        let pending = self.shared.presentation.borrow_mut().take().filter(|p| {
            p.revision == self.shared.revision.get()
                && Arc::ptr_eq(&p.source, result.document.source())
        });
        if let Some(prepared) = pending.as_ref().and_then(|p| p.prepared.as_ref()) {
            result.document = prepared.document.clone();
        }
        self.shared
            .text_epoch
            .set(self.shared.text_epoch.get().wrapping_add(1));
        let source_changed = !Arc::ptr_eq(&self.document(), &result.document);
        *self.shared.highlighted.borrow_mut() = result;
        if source_changed {
            self.shared.geometry.borrow_mut().content_width = 0.;
            self.shared.selection.set(None);
            self.shared.dragging.set(None);
            self.shared.hover.set(None);
            self.shared.symbol.borrow_mut().take();
            self.shared.hover_markdown.borrow_mut().take();
            self.shared
                .annotations
                .borrow_mut()
                .clone_from(&Arc::from([]));
            self.shared.bubbles.borrow_mut().clear();
            self.shared.decorations.borrow_mut().take();
            self.shared.hover_decoration.set(None);
            self.shared.review_hover.borrow_mut().take();
            self.shared.hover_target.set(None);
            self.shared.annotation_ids.borrow_mut().clear();
            self.shared.folds.borrow_mut().clear();
            self.shared.hints.borrow_mut().clear();
            *self.shared.text_annotations.borrow_mut() = Arc::from([]);
            self.shared.has_range_styling.set(false);
            let generation = self.shared.block_layout.borrow().generation.wrapping_add(1);
            *self.shared.block_layout.borrow_mut() = BlockLayout {
                generation,
                ..Default::default()
            };
            self.shared.has_bubbles.set(false);
            self.shared.traces.borrow_mut().clone_from(&Arc::from([]));
            *self.shared.index.borrow_mut() = IntervalIndex::default();
            *self.shared.rows.borrow_mut() = RowMap::new(self.document().line_count(), &[], &[]);
        }
        if let Some(pending) = pending {
            if let Some(build) = pending.document_decorations {
                self.publish_document_decorations(build(self.document()), cx);
            } else {
                let prepared = pending.prepared.unwrap_or_else(|| {
                    PreparedAnalysis::new(self.document(), pending.annotations, pending.folds)
                });
                self.set_analysis(prepared, cx);
            }
            self.set_traces(pending.traces, cx);
        }
        self.notify_all(cx);
    }
    fn hit(&self, point: Point<Pixels>) -> Option<Position> {
        crate::paint::hit(&self.shared, point)
    }
}

pub(crate) struct TextLayer {
    pub shared: Rc<Shared>,
    pub cache: Rc<RefCell<ShapeCache>>,
}
impl Render for TextLayer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut stats = self.shared.stats.get();
        stats.text_renders += 1;
        self.shared.stats.set(stats);
        let theme = GuiraffeTheme::global(cx).clone();
        let context = StyleContext::current();
        let shared = self.shared.clone();
        let cache = self.cache.clone();

        canvas(
            move |bounds, window, cx| {
                let old = {
                    let g = shared.geometry.borrow();
                    (g.bounds, g.line_height, g.gutter, g.margin, g.mini)
                };
                let old_blame = inline_blame_anchor(&shared);
                let truncated = shared.stats.get().last_truncated_rows > 0;
                let frame = crate::paint::prepare(
                    &shared,
                    &mut cache.borrow_mut(),
                    bounds,
                    &theme,
                    context,
                    window,
                    cx,
                );
                let changed = {
                    let g = shared.geometry.borrow();
                    old != (g.bounds, g.line_height, g.gutter, g.margin, g.mini)
                };
                let overlay_changed = changed
                    || old_blame != inline_blame_anchor(&shared)
                    || truncated != (shared.stats.get().last_truncated_rows > 0);
                if overlay_changed {
                    let dependents = if changed {
                        shared.dependents.borrow().clone()
                    } else {
                        Vec::new()
                    };
                    let overlay = shared.overlay_view.borrow().clone();
                    cx.defer(move |cx| {
                        for weak in dependents {
                            if weak.is_upgradable() {
                                cx.notify(weak.entity_id());
                            }
                        }
                        if let Some(overlay) = overlay
                            && overlay.is_upgradable()
                        {
                            cx.notify(overlay.entity_id());
                        }
                    });
                }
                frame
            },
            move |_, frame, window, cx| crate::paint::paint_text(frame, window, cx),
        )
        .size_full()
    }
}
pub(crate) struct OverlayLayer {
    shared: Rc<Shared>,
}
fn accessible_lines(
    document: &CodeDocument,
    rows: &RowMap,
    viewport: Viewport,
    line_height: f32,
    height: f32,
    selection: Option<SourceRange>,
    decorations: Option<&Decorations>,
) -> impl IntoElement {
    let mut list = div()
        .id("code-source-lines")
        .test_support()
        .role(Role::List)
        .aria_label("Source lines")
        .absolute()
        .size_full();
    for row in viewport.visible(rows.len(), line_height, height) {
        let Some(Row::Source(line)) = rows.row(row) else {
            continue;
        };
        let text = document.line(line).unwrap_or_default();
        // Bound the accessible preview just as paint bounds line shaping.
        let end = text
            .char_indices()
            .nth(256)
            .map_or(text.len(), |(byte, _)| byte);
        let selected = selection.is_some_and(|range| {
            range.start != range.end
                && range.start.line <= line
                && (line < range.end.line || (line == range.end.line && range.end.column > 0))
        });
        let mut label = format!("Line {}: {}", line + 1, &text[..end]);
        if let Some(decorations) = decorations
            && let Some(bytes) = document.line_range(line)
        {
            use std::fmt::Write;
            for (_, layer) in decorations.layers() {
                layer.query_limited(bytes.start as u64..bytes.end as u64 + 1, 8, |mark| {
                    if let Some(DecorationStyle::Diagnostic {
                        severity, message, ..
                    }) = decorations.style(mark.style)
                    {
                        let end = message
                            .char_indices()
                            .nth(160)
                            .map_or(message.len(), |(index, _)| index);
                        let _ = write!(label, ". {severity:?}: {}", &message[..end]);
                    }
                });
            }
        }
        let y = (row as f64 * f64::from(line_height) - viewport.y) as f32;
        list = list.child(
            div()
                .id(("source-line", line))
                .test_support()
                .role(Role::ListItem)
                .aria_label(label)
                .aria_position_in_set(line + 1)
                .aria_size_of_set(document.line_count())
                .aria_selected(selected)
                .absolute()
                .top(px(y))
                .w_full()
                .h(px(line_height)),
        );
    }
    list
}

fn inline_blame_anchor(shared: &Shared) -> Option<(usize, Point<Pixels>)> {
    let line = shared.selection.get()?.start.line;
    let decorations = shared.decorations.borrow();
    let decorations = decorations.as_ref().filter(|data| data.has_blame())?;
    let column = decorations.document().line_range(line)?.len();
    crate::paint::anchor(shared, Position::new(line, column)).map(|anchor| (line, anchor))
}

fn emit_decoration_event(
    shared: &Shared,
    key: (DecorationLayerId, DecorationId),
    action: DecorationAction,
    cx: &mut App,
) {
    let Some(handler) = shared.decoration_handler.borrow().clone() else {
        return;
    };
    let style = shared.decorations.borrow().as_ref().and_then(|data| {
        let mark = data.layer(key.0)?.get(key.1)?;
        data.style(mark.style).cloned()
    });
    if let Some(style) = style {
        handler(
            DecorationEvent {
                action,
                layer: key.0,
                decoration: key.1,
                style,
            },
            cx,
        );
    }
}

fn measure_block(
    shared: &Rc<Shared>,
    id: u64,
    generation: u64,
    height: Pixels,
    line_height: f32,
    cx: &mut App,
) {
    let mut layout = shared.block_layout.borrow_mut();
    if layout.generation != generation || !layout.measured.insert(id) {
        return;
    }
    let rows = (height.as_f32() / line_height)
        .ceil()
        .clamp(1., f32::from(u16::MAX)) as usize;
    if layout.rows.get(&id) == Some(&rows) {
        return;
    }
    let schedule = layout.pending.is_empty();
    layout.pending.insert(id, rows);
    drop(layout);
    if schedule {
        let shared = shared.clone();
        cx.defer(move |cx| {
            let mut layout = shared.block_layout.borrow_mut();
            if layout.generation != generation {
                return;
            }
            let heights = std::mem::take(&mut layout.pending);
            layout.rows.extend(heights);
            *shared.rows.borrow_mut() = RowMap::with_blocks(
                shared.highlighted.borrow().document.line_count(),
                &shared.folds.borrow(),
                &shared.annotations.borrow(),
                &layout.rows,
            );
            if let Some(text) = shared.text_view.borrow().as_ref() {
                cx.notify(text.entity_id());
            }
            if let Some(overlay) = shared.overlay_view.borrow().as_ref() {
                cx.notify(overlay.entity_id());
            }
        });
    }
}

impl Render for OverlayLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut stats = self.shared.stats.get();
        stats.overlay_renders += 1;
        self.shared.stats.set(stats);
        let shared = self.shared.clone();
        // The preceding text layer publishes this frame's geometry in prepaint.
        gpui_kit::container_query(move |_, window, cx| Self::content(&shared, window, cx))
            .absolute()
            .top_0()
            .left_0()
    }
}

impl OverlayLayer {
    fn content(shared: &Rc<Shared>, window: &mut Window, cx: &mut App) -> gpui_kit::Div {
        let s = GuiraffeTheme::global(cx).surfaces();
        let paint_shared = shared.clone();
        let g = shared.geometry.borrow();
        let h = g.bounds.size.height.as_f32();
        let lh = g.line_height;
        let margin = g.margin;
        let width = g.bounds.size.width.as_f32();
        let mini = g.mini;
        drop(g);
        let annotations = shared.annotations.borrow();
        let rows = shared.rows.borrow();
        let viewport = shared.viewport.get();
        let mut requests = Vec::new();
        if margin > 0. && shared.has_bubbles.get() {
            let visible = viewport.visible(rows.len(), lh, h);
            let mut hits = Vec::new();
            let doc = shared.highlighted.borrow();
            let index = shared.index.borrow();
            for row in visible {
                if let Some(Row::Source(line)) = rows.row(row)
                    && let Some(range) = doc.document.line_range(line)
                {
                    index.query(range.start..range.end.saturating_add(1), &mut hits);
                    for &id in &hits {
                        if let Some(a) = annotations.get(id)
                            && a.kind == AnnotationKind::Bubble
                        {
                            requests.push((
                                a.id,
                                (row as f64 * f64::from(lh) - viewport.y) as f32,
                                lh * 3.,
                            ));
                        }
                    }
                }
            }
        }
        requests.sort_by_key(|(id, _, _)| *id);
        requests.dedup_by_key(|(id, _, _)| *id);
        let placements = place_bubbles(&requests, h, lh * 0.25);
        shared
            .bubbles
            .borrow_mut()
            .retain(|id, _| placements.iter().any(|placement| placement.id == *id));
        let mut stats = shared.stats.get();
        stats.visible_bubbles = placements.len();
        shared.stats.set(stats);
        let mut root = div().size_full().text_color(s.text.to_hsla()).child(
            canvas(
                |_, _, _| (),
                move |_, _, window, cx| crate::paint::paint_overlay(&paint_shared, window, cx),
            )
            .absolute()
            .size_full(),
        );
        if window.is_a11y_active() {
            root = root.child(accessible_lines(
                &shared.highlighted.borrow().document,
                &rows,
                viewport,
                lh,
                h,
                shared.selection.get(),
                shared.decorations.borrow().as_deref(),
            ));
        }
        if shared.stats.get().last_truncated_rows > 0 {
            root = root.child(
                div()
                    .absolute()
                    .bottom_0()
                    .left(px(shared.geometry.borrow().gutter))
                    .bg(s.body.to_hsla())
                    .text_color(s.dimmed.to_hsla())
                    .text_size(px(Size::Xs.font_size()))
                    .child("Long line preview limited to 16 KiB; source copy is complete"),
            );
        }
        if margin > 0. {
            for p in placements {
                if !shared.bubbles.borrow().contains_key(&p.id)
                    && let Some(a) = shared
                        .annotation_ids
                        .borrow()
                        .get(&p.id)
                        .and_then(|i| annotations.get(*i))
                {
                    let md = cx.new(|cx| MarkdownState::new(a.payload.to_string(), cx));
                    shared.bubbles.borrow_mut().insert(p.id, md);
                }
                if let Some(md) = shared.bubbles.borrow().get(&p.id) {
                    root = root.child(
                        div()
                            .absolute()
                            .left(px((width - mini - margin).max(0.)))
                            .top(px(p.y))
                            .w(px(margin))
                            .h(px(p.height))
                            .overflow_hidden()
                            .bg(s.surface.to_hsla())
                            .p(px(Size::Xs.spacing() * 0.5))
                            .rounded(px(Size::Sm.radius()))
                            .border_1()
                            .border_color(s.border.to_hsla())
                            .child(
                                MarkdownView::new(format!("code-detail-{}", shared.id)).state(md),
                            ),
                    );
                }
            }
        }
        if let Some((line, at)) = inline_blame_anchor(shared)
            && let Some(decorations) = shared.decorations.borrow().as_ref()
            && let Some(bytes) = decorations.document().line_range(line)
        {
            let mut blame = None;
            for (_, layer) in decorations.layers().filter(|(_, layer)| layer.has_blame()) {
                layer.query_limited(bytes.start as u64..bytes.start as u64 + 1, 8, |mark| {
                    if let Some(DecorationStyle::Blame(commit)) = decorations.style(mark.style) {
                        blame = Some(commit);
                    }
                });
            }
            if let Some(commit) = blame {
                let origin = shared.geometry.borrow().bounds.origin;
                let x = (at.x - origin.x + px(lh * 0.5)).as_f32();
                let available = width - margin - mini - x;
                if available > lh * 4. {
                    let message: String = commit.message.chars().take(120).collect();
                    root = root.child(
                        div()
                            .absolute()
                            .left(px(x))
                            .top(at.y - origin.y - px(lh * 0.5))
                            .w(px(available))
                            .h(px(lh))
                            .overflow_hidden()
                            .text_color(s.dimmed.to_hsla())
                            .text_size(px(Size::Xs.font_size()))
                            .child(format!(
                                "{} · {} days ago · {message}",
                                commit.author, commit.age_days
                            )),
                    );
                }
            }
        }
        let hover_row = shared
            .hover
            .get()
            .and_then(|position| rows.source_row(position.line))
            .filter(|row| viewport.visible(rows.len(), lh, h).contains(row));
        if let Some(row) = hover_row {
            let card_height = (lh * 5.).min(h);
            let y = (row as f64 * f64::from(lh) - viewport.y) as f32;
            let right_padding = mini.max(8.) + 4.;
            let available = (width - right_padding).max(0.);
            let card_width = if margin > 8. {
                (margin - 8.).min(available)
            } else {
                (lh * 14.).min(available)
            };
            let top = if margin > 8. || y + lh + card_height <= h {
                y + lh
            } else {
                y - card_height
            };
            let card = || {
                div()
                    .absolute()
                    .right(px(right_padding))
                    .top(px(top.clamp(0., (h - card_height).max(0.))))
                    .w(px(card_width))
                    .max_h(px(card_height))
                    .overflow_hidden()
                    .rounded(px(Size::Sm.radius()))
                    .bg(s.surface.to_hsla())
                    .border_1()
                    .border_color(s.border.to_hsla())
                    .p(px(Size::Sm.spacing()))
            };
            let decorations = shared.decorations.borrow();
            let detail = shared.hover_decoration.get().and_then(|(layer, id)| {
                let decorations = decorations.as_ref()?;
                let record = decorations.layer(layer)?.get(id)?;
                match decorations.style(record.style)? {
                    DecorationStyle::Blame(commit) => Some(format!(
                        "{} · {} · {}\n{}",
                        commit.author, commit.date, commit.short_hash, commit.message
                    )),
                    DecorationStyle::Diagnostic { message, .. } => Some(message.to_string()),
                    _ => None,
                }
            });
            if margin > 8.
                && let Some(md) = shared.review_hover.borrow().as_ref()
            {
                root = root.child(
                    card()
                        .h(px(card_height))
                        .child(MarkdownView::new(format!("code-review-{}", shared.id)).state(md)),
                );
            } else if margin > 8.
                && let Some(detail) = detail
            {
                root = root.child(card().child(detail));
            } else if let Some(md) = shared.hover_markdown.borrow().as_ref() {
                root = root.child(
                    card()
                        .h(px(card_height))
                        .child(MarkdownView::new(format!("code-detail-{}", shared.id)).state(md)),
                );
            }
        }
        let mut visible_widgets = Vec::new();
        let left = shared.geometry.borrow().gutter;
        let widget_width = (width - left - margin - mini).max(0.);
        if lh > 0. && !shared.block_layout.borrow().keys.is_empty() {
            let mut layout = shared.block_layout.borrow_mut();
            let size = Some((widget_width.to_bits(), lh.to_bits()));
            if layout.size != size {
                layout.size = size;
                layout.generation = layout.generation.wrapping_add(1);
                layout.measured.clear();
                layout.pending.clear();
            }
            let viewport_rows = viewport.visible(rows.len(), lh, h);
            let first = viewport_rows.start;
            for row in viewport_rows {
                let Some(Row::Block {
                    annotation, start, ..
                }) = rows.row(row)
                else {
                    continue;
                };
                if row != start.max(first) {
                    continue;
                }
                if let Some(item) = annotations.get(annotation)
                    && let Some(&(layer, decoration)) = layout.keys.get(&item.id)
                {
                    let y = (start as f64 * f64::from(lh) - viewport.y) as f32;
                    visible_widgets.push((
                        item.id,
                        y,
                        layout.generation,
                        !layout.measured.contains(&item.id),
                        BlockWidget {
                            layer,
                            decoration,
                            payload: item.payload.clone(),
                        },
                    ));
                }
            }
        }
        drop(rows);
        drop(annotations);
        if !visible_widgets.is_empty() {
            let renderer = shared.block_renderer.borrow().clone();
            for (id, y, generation, needs_measure, widget) in visible_widgets {
                let key = (widget.layer, widget.decoration);
                let content = if let Some(renderer) = &renderer {
                    renderer(widget, window, cx)
                } else {
                    div()
                        .p(px(Size::Sm.spacing()))
                        .bg(s.surface.to_hsla())
                        .border_1()
                        .border_color(s.border.to_hsla())
                        .rounded(px(Size::Sm.radius()))
                        .child(widget.payload.to_string())
                        .into_any_element()
                };
                let mut block = div()
                    .absolute()
                    .top(px(y))
                    .left(px(left))
                    .w(px(widget_width))
                    .child(content);
                if needs_measure {
                    let shared = shared.clone();
                    block = block.child(
                        canvas(
                            move |bounds, _, cx| {
                                measure_block(&shared, id, generation, bounds.size.height, lh, cx)
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    );
                }
                let hover = shared.clone();
                let click = shared.clone();
                root = root.child(
                    block
                        .id(ElementId::Name(
                            format!("code-block-{}-{}", key.0, key.1).into(),
                        ))
                        .on_hover(move |is_hovered, _, cx| {
                            let action = if *is_hovered {
                                DecorationAction::Hover
                            } else {
                                DecorationAction::Leave
                            };
                            emit_decoration_event(&hover, key, action, cx);
                        })
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            emit_decoration_event(&click, key, DecorationAction::Click, cx);
                        }),
                );
            }
        }
        root.children(shared.menu.element())
    }
}
/// Read-only viewer embeddable anywhere, including a host's Markdown custom-fence factory.
/// Construct the handle once, then rebuild this cheap element as needed.
#[derive(gpui_kit::IntoElement)]
pub struct CodeViewer {
    handle: CodeHandle,
    style: StyleRefinement,
}
impl CodeViewer {
    pub fn new(handle: &CodeHandle) -> Self {
        Self {
            handle: handle.clone(),
            style: StyleRefinement::default(),
        }
    }
}
impl Styled for CodeViewer {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
impl RenderOnce for CodeViewer {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let h = self.handle;
        let wheel = h.clone();
        let down = h.clone();
        let motion = h.clone();
        let up = h.clone();
        let up_out = h.clone();
        let key = h.clone();
        let leave = h.clone();
        let right = h.clone();
        let mut root = div()
            .id(ElementId::Name(format!("code-{}", h.id()).into()))
            .key_context("GrCode")
            .track_focus(&h.shared.focus)
            .relative()
            .size_full()
            .overflow_hidden()
            .child(h.text.clone())
            .child(h.overlay.clone())
            .on_scroll(move |event: &ScrollEvent, _, cx| {
                let d = event
                    .delta
                    .pixel_delta(px(wheel.shared.geometry.borrow().line_height));
                let v = wheel.viewport();
                wheel.scroll_to(
                    Viewport {
                        x: v.x - f64::from(d.x.as_f32()),
                        y: v.y - f64::from(d.y.as_f32()),
                    },
                    cx,
                );
                cx.stop_propagation();
            })
            .on_mouse_down(
                MouseButton::Left,
                move |event: &MouseDownEvent, window, cx| {
                    down.focus(window, cx);
                    if let Some(line) = down.minimap_line_at(event.position, false) {
                        down.shared.minimap_drag.set(true);
                        down.scroll_to_line(line, cx);
                        return;
                    }
                    let g = down.shared.geometry.borrow();
                    let local = event.position - g.bounds.origin;
                    let gutter = local.x.as_f32() < g.gutter;
                    drop(g);
                    if let Some(mark) = down
                        .decoration_at(down.lens_position(event.position), DecorationTarget::Lens)
                    {
                        down.emit_decoration(mark, DecorationAction::Click, cx);
                        return;
                    }
                    if let Some(p) = down.hit(event.position) {
                        if let Some(mark) =
                            down.decoration_at(Some(p), down.decoration_target(event.position))
                        {
                            down.emit_decoration(mark, DecorationAction::Click, cx);
                        }
                        if gutter {
                            down.toggle_fold(p.line, cx);
                        } else {
                            let definition = down.shared.symbol.borrow().as_ref().map(|s| s.range);
                            let handler = down.shared.definition.borrow().clone();
                            if (event.modifiers.control || event.modifiers.platform)
                                && let (Some(range), Some(handler)) = (definition, handler)
                            {
                                handler(range, window, cx);
                                return;
                            }
                            down.shared.dragging.set(Some(p));
                            down.select(Some(SourceRange::new(p, p)), cx);
                        }
                    }
                },
            )
            .on_mouse_down(
                MouseButton::Right,
                move |event: &MouseDownEvent, window, cx| {
                    let at = right.hit(event.position);
                    // Right-click outside the selection moves the caret there, as editors do.
                    let inside = right
                        .selection()
                        .zip(at)
                        .is_some_and(|(r, p)| r.start <= p && p <= r.end && r.start != r.end);
                    if !inside && let Some(p) = at {
                        right.select(Some(SourceRange::new(p, p)), cx);
                    }
                    right.open_context_menu(event.position, at, window, cx);
                    cx.stop_propagation();
                },
            )
            .on_mouse_move(move |event: &MouseMoveEvent, _, cx| {
                if event.pressed_button != Some(MouseButton::Left) {
                    motion.shared.dragging.set(None);
                    motion.shared.minimap_drag.set(false);
                }
                // Dragging on the minimap scrolls continuously, even if the pointer leaves it.
                if motion.shared.minimap_drag.get() {
                    if let Some(line) = motion.minimap_line_at(event.position, true) {
                        motion.scroll_to_line(line, cx);
                    }
                    return;
                }
                let position = motion.hit(event.position);
                if let Some(start) = motion.shared.dragging.get()
                    && let Some(end) = position
                {
                    motion.select(Some(SourceRange::new(start.min(end), start.max(end))), cx);
                } else {
                    if let Some(lens) = motion.lens_position(event.position) {
                        motion.set_hover_target(Some(lens), DecorationTarget::Lens, cx);
                    } else {
                        motion.set_hover_target(
                            position,
                            motion.decoration_target(event.position),
                            cx,
                        );
                    }
                }
            })
            .on_mouse_up(MouseButton::Left, move |_, _, _| {
                up.shared.dragging.set(None);
                up.shared.minimap_drag.set(false);
            })
            .on_mouse_up_out(MouseButton::Left, move |_, _, _| {
                up_out.shared.dragging.set(None);
                up_out.shared.minimap_drag.set(false);
            })
            .on_hover(move |hovered, _, cx| {
                if !hovered {
                    leave.set_hover(None, cx);
                }
            })
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if guiraffe_core::text_menu::is_menu_key(event) {
                    let at = key.selection().map(|r| r.start);
                    key.open_context_menu(key.menu_anchor(), at, window, cx);
                    cx.stop_propagation();
                    return;
                }
                let command =
                    event.keystroke.modifiers.platform || event.keystroke.modifiers.control;
                match event.keystroke.key.as_str() {
                    "c" if command => key.copy(cx),
                    "a" if command => key.select_all(cx),
                    "f8" => {
                        key.navigate_diagnostic(event.keystroke.modifiers.shift, cx);
                    }
                    "f7" => {
                        key.navigate_change(event.keystroke.modifiers.shift, cx);
                    }
                    "pagedown" | "pageup" => {
                        let g = key.shared.geometry.borrow();
                        let step = g.bounds.size.height.as_f32()
                            * if event.keystroke.key == "pageup" {
                                -1.
                            } else {
                                1.
                            };
                        drop(g);
                        let v = key.viewport();
                        key.scroll_to(
                            Viewport {
                                x: v.x,
                                y: v.y + f64::from(step),
                            },
                            cx,
                        );
                    }
                    _ => return,
                }
                cx.stop_propagation();
            });
        root.style().refine(&self.style);
        root
    }
}

/// A shared overlay above multiple sibling viewers. Attach it to their common `relative`
/// container, after the viewers. Endpoints use window coordinates measured by each viewer.
/// Scrolling either viewer purely notifies this leaf.
#[derive(Clone)]
pub struct TraceHandle {
    view: Entity<TraceView>,
    data: Rc<RefCell<Arc<[Trace]>>>,
    renders: Rc<Cell<u64>>,
}
pub(crate) struct TraceView {
    viewers: Vec<Weak<Shared>>,
    data: Rc<RefCell<Arc<[Trace]>>>,
    renders: Rc<Cell<u64>>,
}
impl TraceHandle {
    pub fn new(viewers: &[CodeHandle], traces: Arc<[Trace]>, cx: &mut App) -> Self {
        let data = Rc::new(RefCell::new(traces));
        let renders = Rc::new(Cell::new(0));
        let view = cx.new(|_| TraceView {
            viewers: viewers.iter().map(|h| Rc::downgrade(&h.shared)).collect(),
            data: data.clone(),
            renders: renders.clone(),
        });
        for h in viewers {
            h.shared.dependents.borrow_mut().push(view.downgrade());
        }
        Self {
            view,
            data,
            renders,
        }
    }
    pub fn render_count(&self) -> u64 {
        self.renders.get()
    }
    pub fn set_traces(&self, traces: Arc<[Trace]>, cx: &mut App) {
        if *self.data.borrow() != traces {
            *self.data.borrow_mut() = traces;
            cx.notify(self.view.entity_id());
        }
    }
    pub fn element(&self) -> impl IntoElement {
        self.view.clone()
    }
}
impl Render for TraceView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let viewers = self.viewers.clone();
        let traces = self.data.borrow().clone();
        let theme = GuiraffeTheme::global(cx).clone();
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let viewers: Vec<_> = viewers.iter().filter_map(Weak::upgrade).collect();
                crate::paint::paint_traces(&viewers, &traces, &theme, window);
            },
        )
        .absolute()
        .size_full()
    }
}

#[cfg(test)]
mod review_tests {
    use crate::{
        Anchor, Annotation, AnnotationKind, CodeDocument, CodeHandle, Fold, Position,
        PreparedAnalysis, SourceRange, Trace,
    };
    use gpui_kit::TestAppContext;
    use std::sync::Arc;
    fn annotation() -> Annotation {
        Annotation {
            id: 42,
            range: SourceRange::new(Position::new(0, 0), Position::new(0, 1)),
            kind: AnnotationKind::Highlight,
            payload: "note".into(),
            color: guiraffe_core::ColorName::Blue,
        }
    }
    #[gpui_kit::test]
    fn block_widgets_measure_their_content_once_and_mount_only_when_visible(
        cx: &mut TestAppContext,
    ) {
        use crate::{AnchorBias, DecorationInput, DecorationStyle, Decorations};
        use gpui_kit::{
            Context, IntoElement, Render, Styled, Window, div, px, test::TestWindowExt,
        };
        use std::{cell::RefCell, rc::Rc};

        struct Fixture(CodeHandle);
        impl Render for Fixture {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                crate::CodeViewer::new(&self.0)
            }
        }

        let called = Rc::new(RefCell::new(Vec::new()));
        let capture = called.clone();
        let (view, vcx) = cx.add_window_view(|_, cx| {
            let document = Arc::new(CodeDocument::new("x\n".repeat(10_000)));
            let code = CodeHandle::new(17, document.clone(), cx);
            let mut data = Decorations::new(document).expect("fixture source fits u32");
            let style = data
                .intern_style(DecorationStyle::Block {
                    estimated_rows: 1,
                    payload: "Review".into(),
                })
                .expect("block style can be interned");
            data.update_layer(
                1,
                Default::default(),
                &[],
                [0, 5_000].map(|line| {
                    let at = Position::new(line, 0);
                    DecorationInput {
                        id: line as u64,
                        range: SourceRange::new(at, at),
                        style,
                        start_bias: AnchorBias::Left,
                        end_bias: AnchorBias::Right,
                    }
                }),
            )
            .expect("block anchors are in range");
            code.set_analysis(data.prepare_analysis(Vec::new(), Vec::new()), cx);
            code.set_decorations(Some(Arc::new(data)), cx);
            code.on_block_widget(
                Some(Rc::new(move |block, _, _| {
                    capture.borrow_mut().push(block.decoration);
                    div().h(px(70.)).into_any_element()
                })),
                cx,
            );
            Fixture(code)
        });
        let code = vcx.update(|_, cx| view.read(cx).0.clone());
        for _ in 0..4 {
            vcx.update(|window, cx| window.render_frame(cx));
            vcx.run_until_parked();
        }
        assert_eq!(code.shared.block_layout.borrow().measured.len(), 1);
        assert!(called.borrow().iter().all(|&id| id == 0));
        let expected = (70. / code.shared.geometry.borrow().line_height).ceil() as usize;
        assert_eq!(code.shared.rows.borrow().source_row(1), Some(1 + expected));

        vcx.update(|_, cx| code.scroll_to_line(5_000, cx));
        for _ in 0..3 {
            vcx.update(|window, cx| window.render_frame(cx));
            vcx.run_until_parked();
        }
        assert!(called.borrow().contains(&5_000));
        assert_eq!(code.shared.block_layout.borrow().measured.len(), 2);
    }

    #[gpui_kit::test]
    fn layered_review_opens_only_for_its_hover_target(cx: &mut TestAppContext) {
        use crate::{AnchorBias, DecorationInput, DecorationStyle, Decorations};

        cx.update(|cx| {
            let document = Arc::new(CodeDocument::new("subtotal + fee"));
            let code = CodeHandle::new(24, document.clone(), cx);
            let mut decorations = Decorations::new(document).expect("fixture fits u32");
            let style = decorations
                .intern_style(DecorationStyle::Review {
                    markdown: "Check the fee.".into(),
                })
                .expect("intern review style");
            decorations
                .update_layer(
                    1,
                    Default::default(),
                    &[],
                    [DecorationInput {
                        id: 0,
                        range: SourceRange::new(Position::new(0, 0), Position::new(0, 8)),
                        style,
                        start_bias: AnchorBias::Left,
                        end_bias: AnchorBias::Right,
                    }],
                )
                .expect("valid review span");
            assert!(code.set_analysis(decorations.prepare_analysis(Vec::new(), Vec::new()), cx));
            assert!(!code.shared.has_bubbles.get());
            assert!(code.set_decorations(Some(Arc::new(decorations)), cx));
            assert!(code.shared.review_hover.borrow().is_none());

            code.set_hover(Some(Position::new(0, 3)), cx);
            assert!(code.shared.review_hover.borrow().is_some());
            code.set_hover(Some(Position::new(0, 12)), cx);
            assert!(code.shared.review_hover.borrow().is_none());
        });
    }

    #[gpui_kit::test]
    fn layered_diagnostics_navigate_emit_hover_and_reject_other_revisions(cx: &mut TestAppContext) {
        use crate::{
            AnchorBias, DecorationAction, DecorationInput, DecorationStyle, Decorations, Severity,
            UnderlinePattern,
        };
        use std::{cell::RefCell, rc::Rc};

        cx.update(|cx| {
            let document = Arc::new(CodeDocument::new("first\nsecond"));
            let code = CodeHandle::new(12, document.clone(), cx);
            let mut decorations = Decorations::new(document).expect("fixture source fits u32");
            let style = decorations
                .intern_style(DecorationStyle::Diagnostic {
                    severity: Severity::Warning,
                    pattern: UnderlinePattern::Dotted,
                    message: "Check these values".into(),
                })
                .expect("diagnostic style can be interned");
            decorations
                .update_layer(
                    1,
                    Default::default(),
                    &[],
                    (0_u64..2).map(|id| {
                        let start = Position::new(id as usize, 0);
                        DecorationInput {
                            id,
                            range: SourceRange::new(start, Position::new(id as usize, 3)),
                            style,
                            start_bias: AnchorBias::Left,
                            end_bias: AnchorBias::Right,
                        }
                    }),
                )
                .expect("diagnostic ranges are valid");
            assert!(code.set_decorations(Some(Arc::new(decorations)), cx));
            let elsewhere = Arc::new(
                Decorations::new(Arc::new(CodeDocument::new("first\nsecond")))
                    .expect("other revision fits u32"),
            );
            assert!(!code.set_decorations(Some(elsewhere), cx));
            let events = Rc::new(RefCell::new(Vec::new()));
            let captured = events.clone();
            code.on_decoration(Some(Rc::new(move |event, _| {
                captured.borrow_mut().push(event)
            })));
            assert!(code.navigate_diagnostic(false, cx));
            assert_eq!(
                code.selection().map(|range| range.start),
                Some(Position::new(0, 0))
            );
            code.set_hover(Some(Position::new(0, 1)), cx);
            code.set_hover(Some(Position::new(0, 2)), cx);
            code.set_hover(None, cx);
            assert_eq!(
                events
                    .borrow()
                    .iter()
                    .map(|event| event.action)
                    .collect::<Vec<_>>(),
                [DecorationAction::Hover, DecorationAction::Leave]
            );
            assert!(code.navigate_diagnostic(false, cx));
            assert_eq!(
                code.selection().map(|range| range.start),
                Some(Position::new(1, 0))
            );
            assert!(!code.navigate_diagnostic(false, cx));
        });
    }

    #[gpui_kit::test]
    fn accessible_line_nodes_expose_a_bounded_near_end_viewport(cx: &mut TestAppContext) {
        use gpui_kit::{
            Context, IntoElement, Render, Role, Window,
            test::{TestQueryExt, TestWindowExt},
        };

        struct AccessibleFixture {
            document: CodeDocument,
            rows: crate::RowMap,
        }
        impl Render for AccessibleFixture {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                super::accessible_lines(
                    &self.document,
                    &self.rows,
                    crate::Viewport {
                        x: 0.,
                        y: 9_999_996. * 20.,
                    },
                    20.,
                    80.,
                    None,
                    None,
                )
            }
        }
        let (_, vcx) = cx.add_window_view(|_, _| AccessibleFixture {
            document: CodeDocument::new("\n".repeat(9_999_999) + "end"),
            rows: crate::RowMap::new(10_000_000, &[], &[]),
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            window.render_frame(cx);
            let lines = window.find_all_by_role(Role::ListItem);
            assert_eq!(lines.len(), 4);
            assert_eq!(lines[0].label(), Some("Line 9999997: "));
            assert_eq!(lines[3].label(), Some("Line 10000000: end"));
        });
    }

    #[gpui_kit::test]
    fn bubble_cache_evicts_cards_when_they_leave_the_viewport(cx: &mut TestAppContext) {
        use gpui_kit::{Context, IntoElement, Render, Window, test::TestWindowExt};

        struct Fixture(CodeHandle);
        impl Render for Fixture {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                crate::CodeViewer::new(&self.0)
            }
        }

        let (view, vcx) = cx.add_window_view(|_, cx| {
            let code = CodeHandle::new(1, Arc::new(CodeDocument::new("x\n".repeat(10_000))), cx);
            code.set_annotations(
                Arc::from([
                    Annotation {
                        kind: AnnotationKind::Bubble,
                        ..annotation()
                    },
                    Annotation {
                        id: 43,
                        kind: AnnotationKind::Bubble,
                        range: SourceRange::new(Position::new(5_000, 0), Position::new(5_000, 1)),
                        ..annotation()
                    },
                ]),
                cx,
            );
            Fixture(code)
        });
        let code = vcx.update(|_, cx| view.read(cx).0.clone());
        vcx.update(|window, cx| window.render_frame(cx));
        vcx.run_until_parked();
        vcx.update(|window, cx| window.render_frame(cx));
        assert!(code.shared.bubbles.borrow().contains_key(&42));

        vcx.update(|window, cx| {
            code.scroll_to_line(5_000, cx);
            window.render_frame(cx);
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| window.render_frame(cx));
        assert_eq!(
            code.shared
                .bubbles
                .borrow()
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            [43]
        );
    }

    #[gpui_kit::test]
    fn changing_provider_refreshes_an_existing_hover(cx: &mut TestAppContext) {
        let code = cx.update(|cx| {
            CodeHandle::new(
                1,
                Arc::new(CodeDocument::new(Arc::<str>::from("value"))),
                cx,
            )
        });
        let range = SourceRange::new(Position::new(0, 0), Position::new(0, 5));
        cx.update(|cx| {
            code.set_symbol_provider(
                Some(std::rc::Rc::new(move |_| {
                    Some(crate::SymbolInfo {
                        range,
                        occurrences: Arc::from([range]),
                        markdown: "first".into(),
                    })
                })),
                cx,
            );
            code.set_hover(Some(Position::new(0, 1)), cx);
            code.set_symbol_provider(
                Some(std::rc::Rc::new(move |_| {
                    Some(crate::SymbolInfo {
                        range,
                        occurrences: Arc::from([range]),
                        markdown: "second".into(),
                    })
                })),
                cx,
            );
        });
        assert_eq!(
            code.shared
                .symbol
                .borrow()
                .as_ref()
                .expect("symbol")
                .markdown
                .as_ref(),
            "second"
        );
    }
    #[gpui_kit::test]
    fn pending_source_keeps_latest_annotations_folds_and_traces(cx: &mut TestAppContext) {
        let code = cx.update(|cx| {
            CodeHandle::new(1, Arc::new(CodeDocument::new(Arc::<str>::from("old"))), cx)
        });
        cx.update(|cx| code.set_source("stale".into(), "text".into(), cx));
        for _ in 0..100 {
            if code.stats().analyses_started == 1 {
                break;
            }
            cx.background_executor.tick();
        }
        cx.update(|cx| {
            code.set_source("a\nb\nc\n".into(), "text".into(), cx);
            code.set_document_decorations(
                |document| {
                    let mut decorations =
                        crate::Decorations::new(document).expect("source fits decorations");
                    decorations
                        .update_layer(
                            23,
                            crate::DecorationLayerOptions {
                                z_index: -2,
                                merge_identical: true,
                            },
                            &[],
                            [],
                        )
                        .expect("empty decoration layer");
                    let prepared = decorations.prepare_analysis(
                        vec![annotation()],
                        vec![Fold {
                            start: 0,
                            end: 3,
                            collapsed: true,
                        }],
                    );
                    (prepared, Some(Arc::new(decorations)))
                },
                cx,
            );
            code.set_traces(
                Arc::from([Trace {
                    from: Anchor {
                        viewer: 1,
                        position: Position::new(0, 0),
                    },
                    to: Anchor {
                        viewer: 1,
                        position: Position::new(1, 0),
                    },
                    color: guiraffe_core::ColorName::Blue,
                }]),
                cx,
            );
        });
        cx.run_until_parked();
        assert_eq!(code.document().source().as_ref(), "a\nb\nc\n");
        assert_eq!(code.shared.annotations.borrow().as_ref(), &[annotation()]);
        assert_eq!(code.shared.folds.borrow().len(), 1);
        assert_eq!(code.shared.traces.borrow().len(), 1);
        assert_eq!(code.shared.rows.borrow().len(), 2);
        let stored = code.shared.decorations.borrow();
        let decorations = stored.as_ref().expect("presentation kept its decorations");
        assert!(Arc::ptr_eq(decorations.document(), &code.document()));
        let layer = decorations.layer(23).expect("presented layer");
        assert_eq!(layer.options().z_index, -2);
        assert!(layer.options().merge_identical);
    }
    #[gpui_kit::test]
    fn pending_prepared_analysis_is_tied_to_its_source(cx: &mut TestAppContext) {
        let code = cx.update(|cx| {
            CodeHandle::new(1, Arc::new(CodeDocument::new(Arc::<str>::from("old"))), cx)
        });
        let source: Arc<str> = "replacement".into();
        let doc = Arc::new(CodeDocument::new(source.clone()));
        let prepared = PreparedAnalysis::new(doc.clone(), Arc::from([annotation()]), vec![]);
        cx.update(|cx| {
            code.set_source(source, "text".into(), cx);
            assert!(code.set_analysis(prepared, cx));
        });
        cx.run_until_parked();
        assert!(Arc::ptr_eq(&code.document(), &doc));
        assert_eq!(code.shared.annotations.borrow().as_ref(), &[annotation()]);
        cx.update(|cx| {
            code.set_source("superseded".into(), "text".into(), cx);
            code.set_annotations(Arc::from([annotation()]), cx);
            code.set_source("latest".into(), "text".into(), cx);
        });
        cx.run_until_parked();
        assert_eq!(code.document().source().as_ref(), "latest");
        assert!(code.shared.annotations.borrow().is_empty());
    }
}
