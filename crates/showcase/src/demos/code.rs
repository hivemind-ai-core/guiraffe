//! The shared read-only code surface, with analysis supplied as data. One entry per feature
//! family: a knob-driven viewer across languages, analysis (diagnostics, highlights, hints,
//! lenses, bubbles, symbol cards), data-flow traces, folds/gutters/long lines, a generated
//! million-line document, and the viewer inside Markdown.
use super::{column, example};
use crate::catalog::Entry;
use crate::knobs::{Knob, KnobValues, Preview, playground_with};
use gpui_kit::component::Disableable;
use gpui_kit::component::button::Button;
use gpui_kit::component::text::{MarkdownExtensions, TextView, TextViewState};
use gpui_kit::{
    AnyElement, AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render,
    Styled, Window, div, px,
};
use guiraffe_code::{
    Anchor, Annotation, AnnotationKind, CodeDocument, CodeHandle, CodeOptions, CodeViewer, Fold,
    GutterColumn, Position, SourceRange, SymbolInfo, Trace, TraceHandle,
};
use guiraffe_components::Text;
use guiraffe_core::{ColorName, Size};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};

pub fn entries() -> Vec<Entry> {
    vec![
        Entry {
            krate: "code",
            name: "CodeViewer",
            summary: "Read-only highlighted code (Rust, JavaScript, JSON) with knobs for minimap, line numbers, inline hints, lenses, bubbles, gutter columns and traces. Right-click a selection for Copy / Select all / Copy with line numbers.",
            covers: &["CodeViewer"],
            demo: viewer,
        },
        Entry {
            krate: "code",
            name: "Code analysis",
            summary: "Diagnostics (wavy underlines), highlights in every colour, inline type hints, code lenses, Markdown bubbles packed in the margin, and hover symbol cards with Go to definition.",
            covers: &[],
            demo: analysis,
        },
        Entry {
            krate: "code",
            name: "Code traces",
            summary: "Data-flow trace lines between variables, routed through the gutter (several, crossing), plus a TraceHandle linking two viewers.",
            covers: &[],
            demo: traces,
        },
        Entry {
            krate: "code",
            name: "Code folds & gutters",
            summary: "Click the gutter to fold; blame and coverage gutter columns; a 20 KB line previewed at 16 KiB with full-source copy.",
            covers: &[],
            demo: folds,
        },
        Entry {
            krate: "code",
            name: "Code 1M lines",
            summary: "A million-line document generated on demand and indexed off the UI thread; only visible rows are shaped.",
            covers: &[],
            demo: million,
        },
        Entry {
            krate: "code",
            name: "Code in Markdown",
            summary: "code_fences embeds the same viewer for fenced code inside Markdown, with bounded height.",
            covers: &[],
            demo: in_markdown,
        },
    ]
}

// ---------------------------------------------------------------------------------------
// Sources and annotation helpers.

const RUST: &str = r#"/// Applies a coupon and sales tax to a cart, returning the total in cents.
pub fn checkout(cart: &Cart, coupon: Option<&Coupon>) -> u64 {
    let subtotal = cart.items.iter().map(|i| i.price * i.qty).sum();
    let discount = coupon.map(|c| c.percent).unwrap_or(0);
    let reduced = subtotal - subtotal * discount / 100;
    let tax = reduced * TAX_RATE / 100;
    reduced + tax
}

pub struct Cart {
    pub items: Vec<Item>,
}

#[test]
fn checkout_applies_coupon() {
    let cart = Cart { items: vec![Item { price: 1_000, qty: 2 }] };
    assert_eq!(checkout(&cart, Some(&Coupon { percent: 10 })), 1_944);
}
"#;

const JS: &str = r#"// Fetches a user's open orders and totals them.
export async function openTotal(api, userId) {
  const res = await api.get(`/users/${userId}/orders`);
  const orders = res.json();
  let total = 0;
  for (const order of orders) {
    if (order.status = "open") {
      total += order.amount;
    }
  }
  return totl;
}
"#;

const JSON: &str = r#"{
  "name": "guiraffe-showcase",
  "version": "0.1.0",
  "features": ["minimap", "inlay-hints", "code-lens", "traces"],
  "limits": { "maxShapedLineBytes": 16384, "minimapBins": 256 },
  "experimental": false
}
"#;

/// Column of the `nth` occurrence of `needle` on `line` (zero-based UTF-8 bytes).
fn col(src: &str, line: usize, needle: &str, nth: usize) -> usize {
    let text = src.lines().nth(line).unwrap_or_default();
    text.match_indices(needle)
        .nth(nth)
        .map(|(i, _)| i)
        .unwrap_or(0)
}
fn span(src: &str, line: usize, needle: &str, nth: usize) -> SourceRange {
    let c = col(src, line, needle, nth);
    SourceRange::new(
        Position::new(line, c),
        Position::new(line, c + needle.len()),
    )
}
fn at(src: &str, line: usize, needle: &str, nth: usize) -> Position {
    Position::new(line, col(src, line, needle, nth))
}
fn after(src: &str, line: usize, needle: &str) -> SourceRange {
    let p = Position::new(line, col(src, line, needle, 0) + needle.len());
    SourceRange::new(p, p)
}
fn ann(
    id: u64,
    range: SourceRange,
    kind: AnnotationKind,
    payload: &str,
    color: ColorName,
) -> Annotation {
    Annotation {
        id,
        range,
        kind,
        payload: payload.into(),
        color,
    }
}
fn handle(id: u64, src: &str, language: &str, cx: &mut App) -> CodeHandle {
    let code = CodeHandle::new(id, Arc::new(CodeDocument::new(Arc::<str>::from(src))), cx);
    code.highlight(language.into(), cx);
    code
}
fn trace(viewer: u64, from: Position, to: Position, color: ColorName) -> Trace {
    Trace {
        from: Anchor {
            viewer,
            position: from,
        },
        to: Anchor {
            viewer,
            position: to,
        },
        color,
    }
}

/// Rust checkout analysis: hints, lenses, bubbles, symbols and highlights.
fn rust_annotations() -> Vec<Annotation> {
    use AnnotationKind::*;
    let s = RUST;
    vec![
        ann(
            1,
            after(s, 2, "let subtotal"),
            InlineHint,
            ": u64",
            ColorName::Gray,
        ),
        ann(
            2,
            after(s, 3, "let discount"),
            InlineHint,
            ": u64",
            ColorName::Gray,
        ),
        ann(
            3,
            after(s, 4, "let reduced"),
            InlineHint,
            ": u64",
            ColorName::Gray,
        ),
        ann(
            4,
            after(s, 5, "let tax"),
            InlineHint,
            ": u64",
            ColorName::Gray,
        ),
        ann(
            10,
            span(s, 1, "pub", 0),
            Lens,
            "2 references · 1 test · returns u64",
            ColorName::Blue,
        ),
        ann(
            11,
            span(s, 14, "fn", 0),
            Lens,
            "▶ Run test · ⏱ 0.4 ms",
            ColorName::Green,
        ),
        ann(
            20,
            span(s, 2, "subtotal", 0),
            Bubble,
            "**Hot path** · 38% of `checkout` samples",
            ColorName::Orange,
        ),
        ann(
            21,
            span(s, 4, "subtotal * discount", 0),
            Bubble,
            "**Overflow** possible when `subtotal > u64::MAX / 100`",
            ColorName::Red,
        ),
        ann(
            22,
            span(s, 6, "reduced + tax", 0),
            Bubble,
            "**Returns** `reduced + tax` (cents)",
            ColorName::Blue,
        ),
        ann(
            30,
            span(s, 2, "subtotal", 0),
            Highlight,
            "",
            ColorName::Green,
        ),
        ann(31, span(s, 1, "coupon", 0), Highlight, "", ColorName::Grape),
        ann(
            32,
            span(s, 4, "subtotal * discount", 0),
            Underline,
            "",
            ColorName::Red,
        ),
        ann(
            40,
            span(s, 1, "checkout", 0),
            Symbol,
            "```rust\npub fn checkout(cart: &Cart, coupon: Option<&Coupon>) -> u64\n```\nTotal in cents.",
            ColorName::Blue,
        ),
        ann(
            41,
            span(s, 2, "subtotal", 0),
            Symbol,
            "Local **subtotal**: `u64`",
            ColorName::Blue,
        ),
    ]
}

fn rust_traces(viewer: u64) -> Vec<Trace> {
    let s = RUST;
    vec![
        trace(
            viewer,
            at(s, 1, "coupon", 0),
            at(s, 3, "coupon", 0),
            ColorName::Grape,
        ),
        trace(
            viewer,
            at(s, 2, "subtotal", 0),
            at(s, 4, "subtotal", 0),
            ColorName::Green,
        ),
        trace(
            viewer,
            at(s, 3, "discount", 0),
            at(s, 4, "discount", 0),
            ColorName::Orange,
        ),
        trace(
            viewer,
            at(s, 4, "reduced", 0),
            at(s, 6, "reduced", 0),
            ColorName::Blue,
        ),
        trace(
            viewer,
            at(s, 5, "tax", 0),
            at(s, 6, "tax", 0),
            ColorName::Teal,
        ),
    ]
}

fn blame(lines: usize) -> GutterColumn {
    let authors = ["ada", "lin", "ada", "kai"];
    GutterColumn {
        title: "Blame".into(),
        cells: (0..lines)
            .filter(|l| l % 3 != 2)
            .map(|l| (l, Arc::<str>::from(authors[l % authors.len()])))
            .collect(),
        width_chars: 4,
    }
}

// ---------------------------------------------------------------------------------------
// 1. Knob-driven viewer.

const LANGS: &[&str] = &["rust", "javascript", "json"];

struct ViewerState {
    handles: Vec<CodeHandle>,
    applied: RefCell<Option<KnobValues>>,
}

fn viewer(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "CodeViewer",
        &[
            Knob::new("Language", LANGS),
            Knob::toggle("Minimap").default(1),
            Knob::toggle("Line numbers").default(1),
            Knob::toggle("Hints").default(1),
            Knob::toggle("Lenses").default(1),
            Knob::toggle("Bubbles").default(1),
            Knob::toggle("Blame").default(0),
            Knob::toggle("Traces").default(1),
        ],
        |_, cx| ViewerState {
            handles: vec![
                handle(101, RUST, "rust", cx),
                handle(102, JS, "javascript", cx),
                handle(103, JSON, "json", cx),
            ],
            applied: RefCell::new(None),
        },
        render_viewer,
        window,
        cx,
    )
}

fn render_viewer(
    state: &ViewerState,
    v: &KnobValues,
    _: &mut Window,
    cx: &mut Context<Preview<ViewerState>>,
) -> AnyElement {
    if state.applied.borrow().as_ref() != Some(v) {
        *state.applied.borrow_mut() = Some(v.clone());
        for (i, code) in state.handles.iter().enumerate() {
            code.set_options(
                CodeOptions {
                    minimap: v.on(1),
                    line_numbers: v.on(2),
                    margin_chars: if v.on(5) { 26 } else { 0 },
                    ..Default::default()
                },
                cx,
            );
            let all = if i == 0 {
                rust_annotations()
            } else {
                js_annotations()
            };
            let kept: Vec<_> = if i == 2 {
                Vec::new()
            } else {
                all.into_iter()
                    .filter(|a| match a.kind {
                        AnnotationKind::InlineHint => v.on(3),
                        AnnotationKind::Lens => v.on(4),
                        AnnotationKind::Bubble => v.on(5),
                        _ => true,
                    })
                    .collect()
            };
            code.set_annotations(Arc::from(kept), cx);
            let lines = code.document().line_count();
            code.set_gutters(if v.on(6) { vec![blame(lines)] } else { vec![] }, cx);
            let traces = if i == 0 && v.on(7) {
                rust_traces(101)
            } else {
                vec![]
            };
            code.set_traces(Arc::from(traces), cx);
        }
    }
    let code = state
        .handles
        .get(v.index(0))
        .or_else(|| state.handles.first());
    div()
        .h(px(360.))
        .children(code.map(CodeViewer::new))
        .into_any_element()
}

// ---------------------------------------------------------------------------------------
// 2. Analysis.

fn js_annotations() -> Vec<Annotation> {
    use AnnotationKind::*;
    let s = JS;
    vec![
        ann(
            1,
            span(s, 3, "res.json()", 0),
            Underline,
            "",
            ColorName::Yellow,
        ),
        ann(
            2,
            span(s, 6, "order.status = \"open\"", 0),
            Underline,
            "",
            ColorName::Red,
        ),
        ann(3, span(s, 10, "totl", 0), Underline, "", ColorName::Red),
        ann(
            4,
            after(s, 3, "const orders"),
            InlineHint,
            ": Promise<Order[]>",
            ColorName::Gray,
        ),
        ann(
            5,
            after(s, 2, "const res"),
            InlineHint,
            ": Response",
            ColorName::Gray,
        ),
        ann(
            6,
            span(s, 1, "export", 0),
            Lens,
            "1 reference · exported from orders.js",
            ColorName::Blue,
        ),
        ann(
            10,
            span(s, 3, "res.json()", 0),
            Bubble,
            "**Warning** · `res.json()` returns a Promise; missing `await`.",
            ColorName::Yellow,
        ),
        ann(
            11,
            span(s, 6, "order.status = \"open\"", 0),
            Bubble,
            "**Error** · assignment in condition; did you mean `===`?",
            ColorName::Red,
        ),
        ann(
            12,
            span(s, 10, "totl", 0),
            Bubble,
            "**Error** · `totl` is not defined. Quick fix: `total`.",
            ColorName::Red,
        ),
        ann(
            13,
            span(s, 4, "total", 0),
            Bubble,
            "**Info** · `total` is mutated in the loop.",
            ColorName::Blue,
        ),
        ann(20, span(s, 4, "total", 0), Highlight, "", ColorName::Blue),
        ann(21, span(s, 7, "total", 0), Highlight, "", ColorName::Blue),
        ann(22, span(s, 5, "order", 0), Highlight, "", ColorName::Teal),
        ann(23, span(s, 1, "userId", 0), Highlight, "", ColorName::Grape),
        ann(24, span(s, 2, "userId", 0), Highlight, "", ColorName::Grape),
        ann(25, span(s, 2, "await", 0), Highlight, "", ColorName::Orange),
        ann(26, span(s, 1, "async", 0), Highlight, "", ColorName::Orange),
    ]
}

struct AnalysisDemo {
    code: CodeHandle,
    colors: CodeHandle,
}
impl Render for AnalysisDemo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([
            example(
                "Diagnostics, hints, lens and bubbles · hover `total` or `openTotal` for a symbol card · Ctrl+click or right-click › Go to definition",
                div().h(px(440.)).child(CodeViewer::new(&self.code)),
            ),
            example(
                "Highlight colours",
                div().h(px(110.)).child(CodeViewer::new(&self.colors)),
            ),
        ])
    }
}

fn analysis(_: &mut Window, cx: &mut App) -> AnyView {
    let code = handle(201, JS, "javascript", cx);
    code.set_options(
        CodeOptions {
            margin_chars: 30,
            minimap: false,
            ..Default::default()
        },
        cx,
    );
    code.set_annotations(Arc::from(js_annotations()), cx);
    // Symbol provider: every occurrence of `total`, and the function name.
    let s = JS;
    let total: Arc<[SourceRange]> = Arc::from(vec![span(s, 4, "total", 0), span(s, 7, "total", 0)]);
    let def = span(s, 4, "total", 0);
    let name = span(s, 1, "openTotal", 0);
    let provider_total = total.clone();
    code.set_symbol_provider(
        Some(Rc::new(move |p: Position| {
            if provider_total.iter().any(|r| {
                r.start.line == p.line && (r.start.column..r.end.column).contains(&p.column)
            }) {
                Some(SymbolInfo {
                    range: def,
                    occurrences: provider_total.clone(),
                    markdown: "**total**: `number` · accumulates open order amounts".into(),
                })
            } else if p.line == name.start.line
                && (name.start.column..name.end.column).contains(&p.column)
            {
                Some(SymbolInfo {
                    range: name,
                    occurrences: Arc::from([name]),
                    markdown: "**openTotal**`(api, userId): Promise<number>`".into(),
                })
            } else {
                None
            }
        })),
        cx,
    );
    let target = code.clone();
    code.on_definition(Some(Rc::new(move |range, _, cx| {
        target.select(Some(range), cx);
        target.scroll_to_line(range.start.line, cx);
    })));

    let palette = [
        ColorName::Blue,
        ColorName::Teal,
        ColorName::Green,
        ColorName::Lime,
        ColorName::Yellow,
        ColorName::Orange,
        ColorName::Red,
        ColorName::Pink,
        ColorName::Grape,
        ColorName::Violet,
        ColorName::Indigo,
        ColorName::Cyan,
        ColorName::Gray,
    ];
    let words: Vec<String> = palette
        .iter()
        .map(|c| format!("{c:?}").to_lowercase())
        .collect();
    let quoted: Vec<_> = words.iter().map(|w| format!("\"{w}\"")).collect();
    let src = format!(
        "const palette = [\n  {},\n  {},\n];\n",
        quoted[..7].join(", "),
        quoted[7..].join(", ")
    );
    let colors = handle(202, &src, "javascript", cx);
    colors.set_options(
        CodeOptions {
            margin_chars: 0,
            minimap: false,
            ..Default::default()
        },
        cx,
    );
    let marks: Vec<_> = palette
        .iter()
        .zip(&words)
        .enumerate()
        .map(|(i, (c, w))| {
            ann(
                i as u64,
                span(&src, if i < 7 { 1 } else { 2 }, &format!("\"{w}\""), 0),
                AnnotationKind::Highlight,
                "",
                *c,
            )
        })
        .collect();
    colors.set_annotations(Arc::from(marks), cx);
    cx.new(|_| AnalysisDemo { code, colors }).into()
}

// ---------------------------------------------------------------------------------------
// 3. Traces.

const CALLER: &str = r#"fn main() {
    let cart = load_cart("cart.json");
    let coupon = Coupon::parse(&args().nth(1));
    let total = checkout(&cart, coupon.as_ref());
    println!("{total}");
}
"#;

struct TracesDemo {
    local: CodeHandle,
    left: CodeHandle,
    right: CodeHandle,
    linked: TraceHandle,
}
impl Render for TracesDemo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([
            example(
                "Data flow inside one function: parameter → locals → return (gutter-routed, crossing)",
                div().h(px(200.)).child(CodeViewer::new(&self.local)),
            ),
            example(
                "TraceHandle across two viewers: call-site arguments → parameters",
                div()
                    .relative()
                    .h(px(170.))
                    .flex()
                    .gap(px(Size::Xl.spacing() * 2.))
                    .child(div().flex_1().child(CodeViewer::new(&self.left)))
                    .child(div().flex_1().child(CodeViewer::new(&self.right)))
                    .child(self.linked.element()),
            ),
        ])
    }
}

fn traces(_: &mut Window, cx: &mut App) -> AnyView {
    let local = handle(301, RUST, "rust", cx);
    local.set_options(
        CodeOptions {
            margin_chars: 0,
            ..Default::default()
        },
        cx,
    );
    local.set_annotations(
        Arc::from(
            rust_annotations()
                .into_iter()
                .filter(|a| a.kind == AnnotationKind::Highlight)
                .collect::<Vec<_>>(),
        ),
        cx,
    );
    local.set_traces(Arc::from(rust_traces(301)), cx);
    let narrow = CodeOptions {
        margin_chars: 0,
        minimap: false,
        ..Default::default()
    };
    let left = handle(302, CALLER, "rust", cx);
    left.set_options(narrow.clone(), cx);
    let right = handle(303, RUST, "rust", cx);
    right.set_options(narrow, cx);
    let cross = |from: Position, to: Position, color| Trace {
        from: Anchor {
            viewer: 302,
            position: from,
        },
        to: Anchor {
            viewer: 303,
            position: to,
        },
        color,
    };
    let linked = TraceHandle::new(
        &[left.clone(), right.clone()],
        Arc::from([
            cross(
                at(CALLER, 3, "&cart", 0),
                at(RUST, 1, "cart", 0),
                ColorName::Green,
            ),
            cross(
                at(CALLER, 3, "coupon.as_ref()", 0),
                at(RUST, 1, "coupon", 0),
                ColorName::Grape,
            ),
        ]),
        cx,
    );
    cx.new(|_| TracesDemo {
        local,
        left,
        right,
        linked,
    })
    .into()
}

// ---------------------------------------------------------------------------------------
// 4. Folds, gutters, long lines.

struct FoldsDemo {
    code: CodeHandle,
    long: CodeHandle,
}
impl Render for FoldsDemo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([
            example(
                "Click a gutter row to fold/unfold (the struct starts folded) · blame and coverage columns",
                div().h(px(260.)).child(CodeViewer::new(&self.code)),
            ),
            example(
                "A 20 KB minified line: previewed at 16 KiB; Ctrl+A, Ctrl+C copies all of it",
                div().h(px(90.)).child(CodeViewer::new(&self.long)),
            ),
        ])
    }
}

fn folds(_: &mut Window, cx: &mut App) -> AnyView {
    let code = handle(401, RUST, "rust", cx);
    code.set_options(
        CodeOptions {
            margin_chars: 0,
            ..Default::default()
        },
        cx,
    );
    let lines = code.document().line_count();
    let coverage: BTreeMap<usize, Arc<str>> = (1..7)
        .map(|l| (l, Arc::<str>::from(if l == 4 { "0×" } else { "12×" })))
        .chain((14..17).map(|l| (l, Arc::<str>::from("1×"))))
        .collect();
    code.set_gutters(
        vec![
            blame(lines),
            GutterColumn {
                title: "Coverage".into(),
                cells: coverage,
                width_chars: 4,
            },
        ],
        cx,
    );
    code.set_folds(
        vec![
            Fold {
                start: 1,
                end: 7,
                collapsed: false,
            },
            Fold {
                start: 9,
                end: 11,
                collapsed: true,
            },
            Fold {
                start: 14,
                end: 17,
                collapsed: false,
            },
        ],
        cx,
    );
    // Preselected so the selection and the right-click menu are visible in context.
    code.select(Some(span(RUST, 6, "reduced + tax", 0)), cx);
    let mut minified = String::from("const data=[");
    let mut i = 0;
    while minified.len() < 20_000 {
        minified.push_str(&format!("{{\"id\":{i},\"v\":{}}},", i * 7 % 101));
        i += 1;
    }
    minified.push_str("];\n");
    let long = handle(402, &minified, "javascript", cx);
    long.set_options(
        CodeOptions {
            margin_chars: 0,
            ..Default::default()
        },
        cx,
    );
    cx.new(|_| FoldsDemo { code, long }).into()
}

// ---------------------------------------------------------------------------------------
// 5. One million lines.

struct MillionDemo {
    code: CodeHandle,
    generated: bool,
}
impl Render for MillionDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity();
        column([example(
            "Virtualized document: generate, then scroll, drag the minimap or PageDown",
            div()
                .flex()
                .flex_col()
                .gap(px(Size::Sm.spacing()))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(Size::Sm.spacing()))
                        .child(
                            Button::new("code-generate-1m")
                                .label(if self.generated {
                                    "Generated"
                                } else {
                                    "Generate 1,000,000 lines"
                                })
                                .disabled(self.generated)
                                .tab_stop(false)
                                .on_click(move |_, _, cx| {
                                    this.update(cx, |demo, cx| demo.generate(cx));
                                }),
                        )
                        .child(
                            Text::new("~48 MB of source; built and indexed off the UI thread")
                                .size(Size::Sm)
                                .dimmed(true),
                        ),
                )
                .child(div().h(px(380.)).child(CodeViewer::new(&self.code))),
        )])
    }
}
impl MillionDemo {
    fn generate(&mut self, cx: &mut Context<Self>) {
        self.generated = true;
        let code = self.code.clone();
        let task = cx
            .background_executor()
            .spawn(async move { million_source() });
        cx.spawn(async move |_, cx| {
            let source = task.await;
            let _ = cx.update(|cx| code.set_source(source, "rust".into(), cx));
        })
        .detach();
        cx.notify();
    }
}
fn million_source() -> Arc<str> {
    let mut s = String::with_capacity(48 * 1_000_000);
    for i in 0..1_000_000u32 {
        use std::fmt::Write;
        let _ = writeln!(
            s,
            "let row_{i} = checksum({i}, 0x{:08x});",
            i.wrapping_mul(2_654_435_761)
        );
    }
    s.into()
}

fn million(_: &mut Window, cx: &mut App) -> AnyView {
    let code = handle(
        501,
        "// Press “Generate 1,000,000 lines”.\n// The source is built and indexed on a background thread,\n// then published; only the visible rows are ever shaped.\n",
        "rust",
        cx,
    );
    code.set_options(
        CodeOptions {
            margin_chars: 0,
            ..Default::default()
        },
        cx,
    );
    cx.new(|_| MillionDemo {
        code,
        generated: false,
    })
    .into()
}

// ---------------------------------------------------------------------------------------
// 6. Inside Markdown.

struct MarkdownDemo {
    text: Entity<TextViewState>,
    extensions: MarkdownExtensions,
}
impl Render for MarkdownDemo {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([example(
            "Fenced code rendered by CodeViewer (max 10 lines, then scrolls)",
            TextView::new(&self.text).markdown_extensions(self.extensions.clone()),
        )])
    }
}

fn in_markdown(_: &mut Window, cx: &mut App) -> AnyView {
    guiraffe_code::init(cx);
    let source = format!(
        "### Checkout\n\nThe total is computed by `checkout`:\n\n```rust\n{RUST}```\n\nAnd its configuration:\n\n```json\n{JSON}```\n"
    );
    let text = cx.new(|cx| TextViewState::markdown(&source, cx));
    let extensions = guiraffe_code::code_fences(MarkdownExtensions::default(), 10);
    cx.new(|_| MarkdownDemo { text, extensions }).into()
}
