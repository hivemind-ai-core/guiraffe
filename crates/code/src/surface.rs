use crate::Position;
use crate::paint::ShapeCache;
use crate::*;
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
struct PendingPresentation {
    revision: u64,
    source: Arc<str>,
    annotations: Arc<[Annotation]>,
    folds: Vec<Fold>,
    traces: Arc<[Trace]>,
    prepared: Option<PreparedAnalysis>,
}

pub type SymbolProvider = Rc<dyn Fn(Position) -> Option<SymbolInfo>>;
pub type DefinitionHandler = Rc<dyn Fn(SourceRange, &mut Window, &mut App)>;

/// Layout defaults inherit the enclosing StyleContext; explicit size is an escape hatch.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeOptions {
    pub size: Option<Size>,
    pub line_numbers: bool,
    pub minimap: bool,
    /// Width of analysis cards in font cells. Zero hides the margin.
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
    pub gutter: f32,
    pub margin: f32,
    pub mini: f32,
    pub shaped: std::collections::HashMap<usize, Rc<ShapedLine>>,
    pub font: Font,
    pub content_width: f64,
    pub layout_epoch: u64,
}
pub(crate) struct Shared {
    pub id: u64,
    pub highlighted: RefCell<Highlighted>,
    pub annotations: RefCell<Arc<[Annotation]>>,
    text_annotations: RefCell<Arc<[Annotation]>>,
    pub hints: RefCell<BTreeMap<usize, DisplayLine>>,
    pub index: RefCell<IntervalIndex>,
    pub annotation_ids: RefCell<BTreeMap<u64, usize>>,
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
    pub hover_markdown: RefCell<Option<Entity<MarkdownState>>>,
    pub traces: RefCell<Arc<[Trace]>>,
    pub dependents: RefCell<Vec<WeakEntity<TraceView>>>,
    pub revision: Cell<u64>,
    pending: RefCell<Option<AnalysisRequest>>,
    analysis_generation: Arc<AtomicU64>,
    analyzing: Cell<bool>,
    presentation: RefCell<Option<PendingPresentation>>,
    pub text_epoch: Cell<u64>,
    pub(crate) layout_epoch: Cell<u64>,
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
            text_annotations: RefCell::new(Arc::from([])),
            hints: RefCell::default(),
            index: RefCell::new(IntervalIndex::default()),
            annotation_ids: RefCell::default(),
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
            hover_markdown: RefCell::default(),
            traces: RefCell::new(Arc::from([])),
            dependents: RefCell::default(),
            revision: Cell::default(),
            pending: RefCell::default(),
            analysis_generation: Arc::default(),
            analyzing: Cell::default(),
            presentation: RefCell::default(),
            text_epoch: Cell::default(),
            layout_epoch: Cell::default(),
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
        self.shared
            .layout_epoch
            .set(self.shared.layout_epoch.get().wrapping_add(1));
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
    pub fn set_analysis(&self, prepared: PreparedAnalysis, cx: &mut App) -> bool {
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
        let text_changed = *self.shared.text_annotations.borrow() != prepared.text
            || *self.shared.folds.borrow() != prepared.folds;
        *self.shared.text_annotations.borrow_mut() = prepared.text;
        *self.shared.index.borrow_mut() = prepared.index;
        *self.shared.rows.borrow_mut() = prepared.rows;
        *self.shared.folds.borrow_mut() = prepared.folds;
        *self.shared.hints.borrow_mut() = prepared.hints;
        *self.shared.annotation_ids.borrow_mut() = prepared.ids;
        *self.shared.annotations.borrow_mut() = prepared.annotations;
        self.shared.bubbles.borrow_mut().clear();
        if text_changed {
            self.shared
                .layout_epoch
                .set(self.shared.layout_epoch.get().wrapping_add(1));
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
            if let Some(prepared) = pending.prepared.take() {
                pending.annotations = prepared.annotations;
            }
            pending.folds = folds;
            return;
        }
        if *self.shared.folds.borrow() == folds {
            return;
        }
        *self.shared.rows.borrow_mut() = RowMap::new(
            self.document().line_count(),
            &folds,
            &self.shared.annotations.borrow(),
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
        if self.shared.hover.replace(position) == position {
            return;
        }
        let symbol = position.and_then(|p| {
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
            self.shared.annotation_ids.borrow_mut().clear();
            self.shared.folds.borrow_mut().clear();
            self.shared.hints.borrow_mut().clear();
            *self.shared.text_annotations.borrow_mut() = Arc::from([]);
            self.shared.traces.borrow_mut().clone_from(&Arc::from([]));
            *self.shared.index.borrow_mut() = IntervalIndex::default();
            *self.shared.rows.borrow_mut() = RowMap::new(self.document().line_count(), &[], &[]);
        }
        if let Some(pending) = pending {
            let prepared = pending.prepared.unwrap_or_else(|| {
                PreparedAnalysis::new(self.document(), pending.annotations, pending.folds)
            });
            self.set_analysis(prepared, cx);
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
                    (
                        g.bounds,
                        g.line_height,
                        g.gutter,
                        g.margin,
                        g.mini,
                        g.layout_epoch,
                    )
                };
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
                    old != (
                        g.bounds,
                        g.line_height,
                        g.gutter,
                        g.margin,
                        g.mini,
                        g.layout_epoch,
                    )
                };
                if changed {
                    let dependents = shared.dependents.borrow().clone();
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
        let y = (row as f64 * f64::from(line_height) - viewport.y) as f32;
        list = list.child(
            div()
                .id(("source-line", line))
                .test_support()
                .role(Role::ListItem)
                .aria_label(format!("Line {}: {}", line + 1, &text[..end]))
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

impl Render for OverlayLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut stats = self.shared.stats.get();
        stats.overlay_renders += 1;
        self.shared.stats.set(stats);
        let s = GuiraffeTheme::global(cx).surfaces();
        let shared = self.shared.clone();
        let g = self.shared.geometry.borrow();
        let h = g.bounds.size.height.as_f32();
        let lh = g.line_height;
        let margin = g.margin;
        let width = g.bounds.size.width.as_f32();
        let mini = g.mini;
        drop(g);
        let annotations = self.shared.annotations.borrow();
        let rows = self.shared.rows.borrow();
        let viewport = self.shared.viewport.get();
        let visible = viewport.visible(rows.len(), lh, h);
        let mut requests = Vec::new();
        for row in visible {
            if let Some(Row::Source(line)) = rows.row(row) {
                let doc = self.shared.highlighted.borrow();
                if let Some(range) = doc.document.line_range(line) {
                    let mut hits = Vec::new();
                    self.shared
                        .index
                        .borrow()
                        .query(range.start..range.end.saturating_add(1), &mut hits);
                    for id in hits {
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
        let mut stats = self.shared.stats.get();
        stats.visible_bubbles = placements.len();
        self.shared.stats.set(stats);
        let mut root = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .text_color(s.text.to_hsla())
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, cx| crate::paint::paint_overlay(&shared, window, cx),
                )
                .absolute()
                .size_full(),
            );
        if window.is_a11y_active() {
            root = root.child(accessible_lines(
                &self.shared.highlighted.borrow().document,
                &rows,
                viewport,
                lh,
                h,
                self.shared.selection.get(),
            ));
        }
        if self.shared.stats.get().last_truncated_rows > 0 {
            root = root.child(
                div()
                    .absolute()
                    .bottom_0()
                    .left(px(self.shared.geometry.borrow().gutter))
                    .bg(s.body.to_hsla())
                    .text_color(s.dimmed.to_hsla())
                    .text_size(px(Size::Xs.font_size()))
                    .child("Long line preview limited to 16 KiB; source copy is complete"),
            );
        }
        if margin > 0. {
            for p in placements {
                if !self.shared.bubbles.borrow().contains_key(&p.id)
                    && let Some(a) = self
                        .shared
                        .annotation_ids
                        .borrow()
                        .get(&p.id)
                        .and_then(|i| annotations.get(*i))
                {
                    let md = cx.new(|cx| MarkdownState::new(a.payload.to_string(), cx));
                    self.shared.bubbles.borrow_mut().insert(p.id, md);
                }
                if let Some(md) = self.shared.bubbles.borrow().get(&p.id) {
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
                                MarkdownView::new(format!("code-detail-{}", self.shared.id))
                                    .state(md),
                            ),
                    );
                }
            }
        }
        if let Some(md) = self.shared.hover_markdown.borrow().as_ref() {
            root = root.child(
                div()
                    .absolute()
                    .right(px(mini))
                    .bottom(px(lh))
                    .w(px(margin.max(lh * 10.)))
                    .h(px(lh * 4.))
                    .overflow_hidden()
                    .bg(s.surface.to_hsla())
                    .p(px(Size::Xs.spacing() * 0.5))
                    .rounded(px(Size::Sm.radius()))
                    .border_1()
                    .border_color(s.border.to_hsla())
                    .child(MarkdownView::new(format!("code-detail-{}", self.shared.id)).state(md)),
            );
        }
        root.children(self.shared.menu.element())
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
                    if let Some(p) = down.hit(event.position) {
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
                    motion.set_hover(position, cx);
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
            code.set_annotations(Arc::from([annotation()]), cx);
            code.set_folds(
                vec![Fold {
                    start: 0,
                    end: 3,
                    collapsed: true,
                }],
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
