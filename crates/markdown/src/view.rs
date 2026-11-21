//! The GPUI Markdown viewer.
//!
//! [`MarkdownState`] owns a [`TextViewState`] and the document analysis that
//! the view needs beyond rendering: headings for a table of contents, and
//! (with the `mermaid` feature) the fences to draw as diagrams.
//!
//! Streaming is incremental by construction: [`MarkdownState::append`] hands
//! the text view only the new suffix, so a reply that grows a token at a time
//! re-parses nothing that was already settled. Analysis runs on a background
//! task and is dropped when a newer revision supersedes it.

use std::sync::Arc;

use gpui_kit::component::text::{TextView, TextViewState};
use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Pixels, Render,
    RenderOnce, SharedString, Styled as _, Window, div,
};

use crate::{
    document::{Document, Heading},
    streaming::{Change, classify},
};

/// Work counters, for tests and devtools (proving streaming stays incremental).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarkdownStats {
    /// Background analyses started (coalesced: at most one in flight).
    pub analyses: u64,
    /// Bytes parsed by those analyses (incremental: only the unsettled tail).
    pub analyzed_bytes: u64,
    /// Full text replacements handed to the text view.
    pub replacements: u64,
    /// Appends handed to the text view.
    pub appends: u64,
}

/// How much of `source` may be shown: everything except an unfinished mermaid fence,
/// which would otherwise be laid out (and re-rendered) on every token.
fn showable(source: &str) -> usize {
    if cfg!(feature = "mermaid") {
        crate::streaming::unfinished_mermaid_fence(source).unwrap_or(source.len())
    } else {
        source.len()
    }
}

/// State of one Markdown viewer.
pub struct MarkdownState {
    text: Entity<TextViewState>,
    source: String,
    /// How much of `source` the text view has (an unfinished mermaid fence is held back).
    pushed: usize,
    revision: u64,
    analyzed: u64,
    document: Arc<Document>,
    /// Settled prefix of the source the current `document` was computed from.
    settled: usize,
    analyzing: bool,
    dirty: bool,
    /// Bumped on every full replacement: an analysis started before it must not seed later
    /// incremental runs (its settled point belongs to different text).
    replaced: u64,
    stats: MarkdownStats,
    menu: guiraffe_core::TextMenu,
    /// Markdown extensions, built once so a re-parse never happens because a
    /// host rebuilt the element.
    #[cfg(feature = "mermaid")]
    extensions: gpui_kit::component::text::MarkdownExtensions,
}

/// What a clicked link does. It runs on the UI thread with the app, and is
/// deliberately not given the viewer's state: the host owns navigation.
pub type LinkHandler = Arc<dyn Fn(&str, &mut Window, &mut App) + Send + Sync>;

/// What a clicked task checkbox does: called with the state the item should
/// now have and the source range of its marker.
pub type TaskToggleHandler =
    Arc<dyn Fn(bool, std::ops::Range<usize>, &mut Window, &mut App) + Send + Sync>;

/// Height of a diagram embedded in a document, in logical pixels.
#[cfg(feature = "mermaid")]
pub const EMBEDDED_DIAGRAM_HEIGHT: f32 = 260.0;
#[cfg(feature = "mermaid")]
use gpui_kit::px;

/// Name of the custom block node a Mermaid fence becomes.
#[cfg(feature = "mermaid")]
pub const MERMAID_NODE: &str = "guiraffe-mermaid";

impl MarkdownState {
    /// Create the state for `source`, parsed as GFM Markdown.
    pub fn new(source: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        let source = source.into().to_string();
        let pushed = showable(&source);
        let initial = source.get(..pushed).unwrap_or_default().to_owned();
        let text = cx.new(|cx| TextViewState::markdown(&initial, cx));
        let mut state = Self {
            text,
            source,
            pushed,
            revision: 0,
            analyzed: 0,
            document: Arc::default(),
            settled: 0,
            analyzing: false,
            dirty: false,
            replaced: 0,
            stats: MarkdownStats::default(),
            menu: guiraffe_core::TextMenu::new(),
            #[cfg(feature = "mermaid")]
            extensions: Self::extensions(),
        };
        state.reanalyze(cx);
        state
    }

    /// The text view's state, for hosts that want selection, highlights or
    /// reveals.
    pub fn text(&self) -> &Entity<TextViewState> {
        &self.text
    }

    /// The selection context menu (Copy, Select all) shown on right-click / Shift+F10.
    pub fn context_menu(&self) -> &guiraffe_core::TextMenu {
        &self.menu
    }

    /// The source currently shown.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Headings, fences and task items of the last analysis that landed.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Table of contents, ready to draw as an outline.
    pub fn headings(&self) -> &[Heading] {
        &self.document.headings
    }

    /// Replace the source, keeping the reader's selection where the text
    /// around it is unchanged.
    ///
    /// A source that extends the current one is handled as an append, which
    /// is what makes a streamed reply cheap; anything else replaces the text.
    pub fn set_source(&mut self, source: impl Into<SharedString>, cx: &mut Context<Self>) {
        let source = source.into().to_string();
        match classify(&self.source, &source) {
            Change::None => return,
            Change::Append { .. } => {
                self.source = source;
                self.sync_text(false, cx);
            }
            Change::Replace { .. } => {
                self.source = source;
                // Earlier blocks changed: the analysis can't reuse anything.
                self.replaced += 1;
                self.settled = 0;
                self.document = Arc::default();
                self.sync_text(true, cx);
            }
        }
        self.revision += 1;
        self.reanalyze(cx);
        cx.notify();
    }

    /// Append one chunk of streamed Markdown and schedule analysis of the
    /// unsettled tail.
    pub fn append(&mut self, chunk: &str, cx: &mut Context<Self>) {
        if chunk.is_empty() {
            return;
        }
        self.source.push_str(chunk);
        self.sync_text(false, cx);
        self.revision += 1;
        self.reanalyze(cx);
        cx.notify();
    }

    /// Hand the text view what it doesn't have yet (or everything, on `replace`).
    fn sync_text(&mut self, replace: bool, cx: &mut Context<Self>) {
        let target = showable(&self.source);
        if replace {
            let shown = self.source.get(..target).unwrap_or_default().to_owned();
            self.text.update(cx, |text, cx| text.set_text(&shown, cx));
            self.stats.replacements += 1;
        } else if target > self.pushed {
            let tail = self
                .source
                .get(self.pushed..target)
                .unwrap_or_default()
                .to_owned();
            self.text.update(cx, |text, cx| text.push_str(&tail, cx));
            self.stats.appends += 1;
        }
        self.pushed = target;
    }

    pub fn stats(&self) -> MarkdownStats {
        self.stats
    }

    /// Bytes of the source currently handed to the text view.
    pub fn shown_len(&self) -> usize {
        self.pushed
    }

    /// Scroll the heading into view.
    ///
    /// The heading's source range is mapped through the rendered document, so
    /// a heading inside a block that Markdown rendered differently (a shortcode,
    /// a table) still resolves.
    pub fn reveal_heading(&mut self, heading: &Heading, cx: &mut Context<Self>) -> bool {
        let range = self.text.update(cx, |text, cx| {
            let rendered = text.rendered_text();
            let mapped = rendered.range_for_source(heading.range.clone())?;
            text.reveal_range(mapped, cx).ok()
        });
        range.is_some()
    }

    /// Re-analyze off the UI thread, **coalesced**: at most one analysis is in flight; updates
    /// arriving meanwhile set `dirty` and trigger exactly one follow-up with the latest text.
    /// Each analysis parses only the tail after the last settled block (incremental).
    fn reanalyze(&mut self, cx: &mut Context<Self>) {
        if self.analyzing {
            self.dirty = true;
            return;
        }
        self.analyzing = true;
        self.dirty = false;
        let revision = self.revision;
        let replaced = self.replaced;
        let prev = self.document.clone();
        let base = self.settled;
        let tail = self.source[base..].to_owned();
        self.stats.analyses += 1;
        self.stats.analyzed_bytes += tail.len() as u64;
        cx.spawn(async move |this, cx| {
            let (document, settled) = cx
                .background_executor()
                .spawn(async move {
                    let doc = Document::analyze_tail(&prev, base, &tail);
                    (doc, base + crate::streaming::settled_prefix(&tail))
                })
                .await;
            this.update(cx, |this, cx| {
                this.analyzing = false;
                if this.replaced == replaced {
                    // Even a superseded *append* result analyses a prefix of the current text,
                    // so its settled point can seed the next incremental run.
                    this.document = Arc::new(document);
                    this.settled = settled;
                    this.analyzed = revision;
                }
                if this.dirty || this.revision != revision {
                    this.reanalyze(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Markdown extensions, exposed so a test can drive a bare `TextView` with
    /// the same registry the view uses.
    #[cfg(feature = "mermaid")]
    pub fn extensions() -> gpui_kit::component::text::MarkdownExtensions {
        use gpui_kit::component::text::{MarkdownExtensions, MarkdownNode};

        MarkdownExtensions::default()
            .block_parser(|node, _| {
                let markdown::mdast::Node::Code(code) = node else {
                    return None;
                };
                if code.lang.as_deref() != Some(crate::document::MERMAID_LANG) {
                    return None;
                }
                Some(
                    MarkdownNode::new(MERMAID_NODE, code.value.clone())
                        .text(code.value.clone())
                        .markdown(code.value.clone()),
                )
            })
            .block_renderer(MERMAID_NODE, |node, _, cx| {
                use guiraffe_mermaid::MermaidView;

                let _ = cx;
                let source = node.as_text().to_owned();
                let id: SharedString =
                    format!("mermaid-{}", crate::document::stable_hash(&source)).into();
                div()
                    .w_full()
                    .h(px(EMBEDDED_DIAGRAM_HEIGHT))
                    .child(MermaidView::new(id).source(source))
            })
    }
}

impl Render for MarkdownState {
    fn render(&mut self, _: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().w_full().child(self.text.clone())
    }
}

/// The Markdown viewer element.
///
/// ```no_run
/// # use gpui_kit::*;
/// # use guiraffe_markdown::{MarkdownView, MarkdownState};
/// fn reply(cx: &mut App) -> impl IntoElement {
///     let state = cx.new(|cx| MarkdownState::new("# Hello", cx));
///     div().child(MarkdownView::new("reply").state(&state))
/// }
/// ```
/// The least height of a scrollable [`MarkdownView`] unless the host names one.
pub const DEFAULT_SCROLL_MIN_HEIGHT: Pixels = gpui_kit::px(200.);

#[derive(IntoElement)]
pub struct MarkdownView {
    id: SharedString,
    state: Option<Entity<MarkdownState>>,
    source: Option<SharedString>,
    selectable: bool,
    scrollable: bool,
    min_height: Pixels,
    max_height: Option<Pixels>,
    on_link: Option<LinkHandler>,
    on_task_toggle: Option<TaskToggleHandler>,
    code_copy: bool,
}

impl MarkdownView {
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            state: None,
            source: None,
            selectable: true,
            scrollable: false,
            min_height: DEFAULT_SCROLL_MIN_HEIGHT,
            max_height: None,
            on_link: None,
            on_task_toggle: None,
            code_copy: true,
        }
    }

    /// Render an existing state entity. Hosts that stream call this so the
    /// element does not replace what they set.
    pub fn state(mut self, state: &Entity<MarkdownState>) -> Self {
        self.state = Some(state.clone());
        self
    }

    /// Render a fixed source, creating a keyed state for it.
    pub fn source(mut self, source: impl Into<SharedString>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    /// Whether the view scrolls its own content (true) or grows with it.
    ///
    /// A scrollable view fills the height its parent gives it. In a parent
    /// of no definite height it keeps at least [`Self::min_height`]
    /// (200px by default), so the document never collapses to nothing.
    pub fn scrollable(mut self, scrollable: bool) -> Self {
        self.scrollable = scrollable;
        self
    }

    /// The least height of a scrollable view (200px by default).
    pub fn min_height(mut self, height: Pixels) -> Self {
        self.min_height = height;
        self
    }

    /// The most height of a scrollable view; it scrolls past it.
    pub fn max_height(mut self, height: Pixels) -> Self {
        self.max_height = Some(height);
        self
    }

    /// Show a copy button on every code block.
    pub fn code_copy(mut self, copy: bool) -> Self {
        self.code_copy = copy;
        self
    }

    /// Called with the address of a clicked link.
    pub fn on_link_click(
        mut self,
        handler: impl Fn(&str, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.on_link = Some(Arc::new(handler));
        self
    }

    /// Called when a task checkbox is clicked, with the state it should now
    /// have and the range of its `[ ]` marker.
    pub fn on_task_toggle(
        mut self,
        handler: impl Fn(bool, std::ops::Range<usize>, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.on_task_toggle = Some(Arc::new(handler));
        self
    }
}

impl RenderOnce for MarkdownView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let MarkdownView {
            id,
            state,
            source,
            selectable,
            scrollable,
            min_height,
            max_height,
            on_link,
            on_task_toggle,
            code_copy,
        } = self;
        let state = match state {
            Some(state) => state,
            None => {
                let initial = source.clone().unwrap_or_default();
                window.use_keyed_state(
                    gpui_kit::ElementId::Name(SharedString::from(format!(
                        "guiraffe-markdown/{id}"
                    ))),
                    cx,
                    |_, cx| MarkdownState::new(initial, cx),
                )
            }
        };
        if let Some(source) = source
            && state.read(cx).source() != source.as_ref()
        {
            state.update(cx, |state, cx| state.set_source(source, cx));
        }

        let text = state.read(cx).text().clone();
        let mut view = TextView::new(&text)
            .selectable(selectable)
            .scrollable(scrollable);
        #[cfg(feature = "mermaid")]
        {
            view = view.markdown_extensions(state.read(cx).extensions.clone());
        }
        // Safe default: Markdown here is often untrusted (agent output, documents), and the
        // Controls TextView would otherwise open any clicked URL itself. Links do nothing
        // unless the host decides what they do.
        view = match on_link {
            Some(handler) => view
                .on_link_click(move |address, _, window, cx| handler(address.as_ref(), window, cx)),
            None => view.on_link_click(|_, _, _, _| {}),
        };
        if let Some(handler) = on_task_toggle {
            view = view.on_task_toggle(move |event, window, cx| {
                handler(event.checked(), event.marker_range(), window, cx)
            });
        }
        if code_copy {
            view = view.code_block_actions(|code, _, _| copy_button(code));
        }
        let mut root = div().w_full();
        if scrollable {
            // The Controls text view fills its box when it scrolls, so the
            // box needs a height: the parent's, but never less than the
            // least height (a parent of no definite height gives none).
            // The view sits in an absolutely placed layer that covers the
            // box, so it gets the box's resolved height even when that
            // height comes only from the least height.
            root = root.relative().h_full().min_h(min_height);
            if let Some(max) = max_height {
                root = root.max_h(max);
            }
            root = root.child(div().absolute().inset_0().child(view));
        } else {
            root = root.child(view);
        }
        if !selectable {
            return root;
        }
        let menu = state.read(cx).menu.clone();
        guiraffe_core::text_menu::text_view_menu(
            root,
            format!("markdown/{id}"),
            &menu,
            &text,
            std::rc::Rc::new(|_, _| Vec::new()),
            window,
            cx,
        )
    }
}

/// The copy affordance shown under a code block.
fn copy_button(code: &gpui_kit::base::text::CodeBlock) -> gpui_kit::AnyElement {
    use gpui_kit::component::Sizable as _;
    use gpui_kit::component::clipboard::Clipboard;

    // Identical blocks share an id, which only shares the “copied” checkmark.
    let id = format!(
        "copy-{}",
        crate::document::stable_hash(code.code().as_ref())
    );
    div()
        .flex()
        .justify_end()
        .child(
            Clipboard::new(SharedString::from(id))
                .value(code.code())
                .tooltip("Copy code")
                .xsmall(),
        )
        .into_any_element()
}
