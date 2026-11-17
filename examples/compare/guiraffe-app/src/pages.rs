//! The five pages. Each is its own view, created on first visit and kept alive (the
//! idiomatic GPUI pattern: page state survives navigation and re-renders independently).

use std::{sync::Arc, time::Duration};

use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Task, Window,
    component::{
        ActiveTheme, Sizable,
        button::{Button, ButtonVariants},
        chart::{BarChart, LineChart},
        checkbox::Checkbox,
        form::{field as form_field, v_form},
        input::{Input, InputEvent, InputState, Textarea, TextareaState},
        select::{Select, SelectEvent, SelectState},
    },
    div,
    prelude::*,
    px,
};
use guiraffe_chat::{
    Chat, ChatEvent, ChatProps, ChatState, Message, MessageId, Role, stream::PushError,
};
use guiraffe_components::{Card, Text, Title};
use guiraffe_core::{ColorName, Size};
use guiraffe_datagrid::{
    Align, ArcSource, CellRef, ColumnDef, DataGrid, Filter, GridEvent, GridHandle, GridOptions,
    SelectionMode,
};
use guiraffe_markdown::{MarkdownState, MarkdownView};
use guiraffe_overlays::{
    OverlayHost,
    toast::{Toast, ToastId},
};

use crate::data::{self, Feed, Row};
use crate::validate::{self, FIELDS, LABELS, Values, f};

pub const DOCUMENT: &str = include_str!("../../shared/document.md");
pub const CHAT_JSON: &str = include_str!("../../shared/chat.json");

// ------------------------------------------------------------------ dashboard

pub struct Dashboard {
    feed: Feed,
    /// Ticks only while the dashboard is the current route (the shell flips this).
    pub active: bool,
    _ticker: Task<()>,
}

impl Dashboard {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let alive = this.update(cx, |d, cx| {
                    if d.active {
                        d.feed.tick();
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        Self {
            feed: Feed::new(),
            active: true,
            _ticker: ticker,
        }
    }
}

fn stat(label: &'static str, value: String) -> impl IntoElement {
    div().flex_1().child(
        Card::new()
            .child(Text::new(label).size(Size::Sm).dimmed(true))
            .child(Title::new(value).order(3)),
    )
}

fn panel(title: &'static str, chart: impl IntoElement) -> impl IntoElement {
    div().flex_1().min_w_0().child(
        Card::new()
            .child(Text::new(title).size(Size::Sm).dimmed(true))
            .child(div().h(px(280.)).w_full().child(chart)),
    )
}

impl Render for Dashboard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let feed = &self.feed;
        let line: Vec<(usize, f64)> = feed
            .throughput
            .iter()
            .enumerate()
            .map(|(i, v)| (i, f64::from(*v)))
            .collect();
        let bars: Vec<(SharedString, f64)> = feed
            .regions
            .iter()
            .enumerate()
            .map(|(i, v)| (SharedString::from(format!("R{}", i + 1)), f64::from(*v)))
            .collect();
        let stroke = cx.theme().chart_1;
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .gap(px(16.))
                    .child(stat("Active users", feed.users.to_string()))
                    .child(stat("Revenue", format!("${:.1}", feed.revenue)))
                    .child(stat("p50 latency", format!("{} ms", feed.latency)))
                    .child(stat("Error rate", format!("{:.2}%", feed.errors))),
            )
            .child(
                div()
                    .flex()
                    .gap(px(16.))
                    .child(panel(
                        "Throughput (req/s), last 60 s",
                        LineChart::new(line)
                            .appear(false)
                            .x(|d| SharedString::from(d.0.to_string()))
                            .y(|d| d.1)
                            .stroke(stroke)
                            .linear()
                            .grid(true),
                    ))
                    .child(panel(
                        "Load by region",
                        BarChart::new(bars)
                            .appear(false)
                            .band(|d| d.0.clone())
                            .value(|d| d.1),
                    )),
            )
    }
}

// ------------------------------------------------------------------ grid

pub struct GridPage {
    pub grid: GridHandle,
    filter: Entity<InputState>,
    selected: usize,
    _subs: Vec<Subscription>,
}

fn columns() -> Vec<ColumnDef> {
    let money = |v: CellRef<'_>, out: &mut String| {
        if let CellRef::Number(n) = v {
            let _ = std::fmt::Write::write_fmt(out, format_args!("${n:.2}"));
        }
    };
    let two = |v: CellRef<'_>, out: &mut String| {
        if let CellRef::Number(n) = v {
            let _ = std::fmt::Write::write_fmt(out, format_args!("{n:.2}"));
        }
    };
    vec![
        ColumnDef::new("id", "ID", 0).width(80.).align(Align::End),
        ColumnDef::new("name", "Name", 1).width(160.),
        ColumnDef::new("email", "Email", 2).width(240.),
        ColumnDef::new("city", "City", 3).width(110.),
        ColumnDef::new("country", "Country", 4).width(120.),
        ColumnDef::new("department", "Department", 5).width(120.),
        ColumnDef::new("status", "Status", 6).width(90.),
        ColumnDef::new("amount", "Amount", 7)
            .width(110.)
            .align(Align::End)
            .formatter(money),
        ColumnDef::new("quantity", "Quantity", 8)
            .width(90.)
            .align(Align::End),
        ColumnDef::new("score", "Score", 9)
            .width(80.)
            .align(Align::End)
            .formatter(two),
        ColumnDef::new("active", "Active", 10)
            .width(70.)
            .formatter(|v, out| {
                out.push_str(if matches!(v, CellRef::Bool(true)) {
                    "yes"
                } else {
                    "no"
                })
            }),
        ColumnDef::new("created", "Created", 11).width(110.),
    ]
}

pub fn source(rows: Arc<[Row]>) -> ArcSource<Row> {
    ArcSource::new(rows)
        .key(|r: &Row| r.id as u64)
        .column(|r: &Row| CellRef::Int(r.id))
        .column(|r: &Row| CellRef::Text(&r.name))
        .column(|r: &Row| CellRef::Text(&r.email))
        .column(|r: &Row| CellRef::Text(r.city))
        .column(|r: &Row| CellRef::Text(r.country))
        .column(|r: &Row| CellRef::Text(r.department))
        .column(|r: &Row| CellRef::Text(r.status))
        .column(|r: &Row| CellRef::Number(r.amount))
        .column(|r: &Row| CellRef::Int(r.quantity))
        .column(|r: &Row| CellRef::Number(r.score))
        .column(|r: &Row| CellRef::Bool(r.active))
        .column(|r: &Row| CellRef::Text(&r.created))
}

/// The Name-column filter for `query` (`None` clears it).
pub fn name_filter(query: &str) -> Option<Filter> {
    let q = query.trim().to_ascii_lowercase();
    (!q.is_empty()).then(|| -> Filter {
        Arc::new(move |v| matches!(v, CellRef::Text(s) if data::contains_ci(s, &q)))
    })
}

impl GridPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let rows: Arc<[Row]> = data::generate(data::ROWS).into();
        let grid = GridHandle::new(source(rows), columns(), cx);
        grid.set_options(
            GridOptions {
                selection: SelectionMode::MultiRow,
                focusable: Some(true),
                zebra: true,
                label: "People".into(),
                ..Default::default()
            },
            cx,
        );
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter by name…"));
        let g = grid.clone();
        let on_filter = cx.subscribe(&filter, move |_, input, e: &InputEvent, cx| {
            if let InputEvent::Change = e {
                let q = input.read(cx).value();
                g.set_filter(1, name_filter(&q), cx);
            }
        });
        let on_grid = cx.subscribe(grid.events(), |this: &mut Self, _, e: &GridEvent, cx| {
            if let GridEvent::SelectionChanged { selected_rows, .. } = e {
                this.selected = *selected_rows;
            }
            cx.notify();
        });
        Self {
            grid,
            filter,
            selected: 0,
            _subs: vec![on_filter, on_grid],
        }
    }
}

impl Render for GridPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let rows = self.grid.view_row_count();
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .w(px(280.))
                            .child(Input::new(&self.filter).cleanable(true)),
                    )
                    .child(
                        Text::new(format!(
                            "{} rows · {} selected",
                            group_digits(rows),
                            group_digits(self.selected)
                        ))
                        .size(Size::Sm)
                        .dimmed(true),
                    ),
            )
            .child(div().flex_1().min_h_0().child(DataGrid::new(&self.grid)))
    }
}

pub fn group_digits(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

// ------------------------------------------------------------------ document

pub struct DocsPage {
    doc: Entity<MarkdownState>,
}

impl DocsPage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            doc: cx.new(|cx| MarkdownState::new(DOCUMENT, cx)),
        }
    }
}

impl Render for DocsPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().min_h_0().child(
            MarkdownView::new("document")
                .state(&self.doc)
                .selectable(true)
                .code_copy(true)
                .scrollable(true),
        )
    }
}

// ------------------------------------------------------------------ chat

#[derive(serde::Deserialize)]
struct Fixture {
    history: Vec<FixtureMessage>,
    reply: String,
}
#[derive(serde::Deserialize)]
struct FixtureMessage {
    role: String,
    text: String,
}

pub struct ChatPage {
    pub chat: Entity<ChatState>,
    reply: Arc<str>,
    /// Tokens of the current reply pushed so far (bench reporting).
    pub tokens: Arc<std::sync::atomic::AtomicUsize>,
    _events: Subscription,
}

/// "Split after each space": the contract's tokenizer.
pub fn tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split_inclusive(' ')
}

impl ChatPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let fixture: Fixture = serde_json::from_str(CHAT_JSON).unwrap_or(Fixture {
            history: Vec::new(),
            reply: String::new(),
        });
        let chat = cx.new(|cx| {
            let mut chat = ChatState::with_props(
                ChatProps {
                    title: "Agent".into(),
                    subtitle: "Simulated streaming at 50 tokens/s".into(),
                },
                window,
                cx,
            );
            for (i, m) in fixture.history.iter().enumerate() {
                let role = if m.role == "user" {
                    Role::User
                } else {
                    Role::Assistant
                };
                chat.push_message(
                    Message::new(MessageId(i as u64 + 1), role, m.text.clone()),
                    cx,
                );
            }
            chat
        });
        let events = cx.subscribe(&chat, |this: &mut Self, _, e: &ChatEvent, cx| {
            if let ChatEvent::Submitted { .. } | ChatEvent::Retry(_) = e {
                this.stream_reply(cx);
            }
        });
        Self {
            chat,
            reply: fixture.reply.into(),
            tokens: Arc::default(),
            _events: events,
        }
    }

    /// Append a user message and stream the reply (what the composer's Submit does).
    pub fn send(&mut self, text: &str, cx: &mut Context<Self>) {
        self.chat.update(cx, |chat, cx| {
            let id = MessageId(chat.messages(cx).count() as u64 + 1_000);
            chat.push_message(Message::new(id, Role::User, text), cx);
        });
        self.stream_reply(cx);
    }

    fn stream_reply(&mut self, cx: &mut Context<Self>) {
        let (_, producer) = self.chat.update(cx, |chat, cx| chat.begin_reply(cx));
        let reply = self.reply.clone();
        let count = self.tokens.clone();
        count.store(0, std::sync::atomic::Ordering::Relaxed);
        let timers = cx.background_executor().clone();
        cx.background_executor()
            .spawn(async move {
                for token in tokens(&reply) {
                    let mut pending = token.to_owned();
                    loop {
                        match producer.push(pending) {
                            Ok(()) => break,
                            Err(PushError::Full(value)) => {
                                pending = value;
                                timers.timer(Duration::from_millis(5)).await;
                            }
                            Err(_) => return,
                        }
                    }
                    count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    timers.timer(Duration::from_millis(20)).await;
                }
            })
            .detach();
    }
}

impl Render for ChatPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .child(Chat::new(&self.chat))
    }
}

// ------------------------------------------------------------------ form

enum Field {
    Text(Entity<InputState>),
    Area(Entity<TextareaState>),
    Country(Entity<SelectState<Vec<&'static str>>>),
    Terms,
}

pub struct FormPage {
    fields: Vec<Field>,
    terms: bool,
    submitted: bool,
    errors: [Option<&'static str>; FIELDS],
    _subs: Vec<Subscription>,
}

impl FormPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut subs = Vec::new();
        let fields = (0..FIELDS)
            .map(|i| match i {
                f::COUNTRY => {
                    let s = cx.new(|cx| {
                        SelectState::new(data::COUNTRIES.to_vec(), None, window, cx)
                            .searchable(true)
                    });
                    subs.push(
                        cx.subscribe(&s, |this: &mut Self, _, _: &SelectEvent<_>, cx| {
                            this.changed(cx)
                        }),
                    );
                    Field::Country(s)
                }
                f::BIO => {
                    let s = cx.new(|cx| TextareaState::new(window, cx).rows(3));
                    subs.push(cx.subscribe(&s, |this: &mut Self, _, e: &InputEvent, cx| {
                        if let InputEvent::Change = e {
                            this.changed(cx)
                        }
                    }));
                    Field::Area(s)
                }
                f::TERMS => Field::Terms,
                _ => {
                    let masked = i == f::PASSWORD || i == f::CONFIRM;
                    let s = cx.new(|cx| InputState::new(window, cx).masked(masked));
                    subs.push(cx.subscribe(&s, |this: &mut Self, _, e: &InputEvent, cx| {
                        if let InputEvent::Change = e {
                            this.changed(cx)
                        }
                    }));
                    Field::Text(s)
                }
            })
            .collect();
        Self {
            fields,
            terms: false,
            submitted: false,
            errors: [None; FIELDS],
            _subs: subs,
        }
    }

    fn values(&self, cx: &App) -> Values {
        std::array::from_fn(|i| match self.fields.get(i) {
            Some(Field::Text(s)) => s.read(cx).value().to_string(),
            Some(Field::Area(s)) => s.read(cx).value().to_string(),
            Some(Field::Country(s)) => s
                .read(cx)
                .selected_value()
                .map(|v| v.to_string())
                .unwrap_or_default(),
            Some(Field::Terms) => if self.terms { "true" } else { "" }.to_string(),
            None => String::new(),
        })
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        if self.submitted {
            let errors = validate::validate(&self.values(cx));
            if errors != self.errors {
                self.errors = errors;
                cx.notify();
            }
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.submitted = true;
        self.errors = validate::validate(&self.values(cx));
        if self.errors.iter().all(Option::is_none) {
            let _ = OverlayHost::notify(window, cx, Toast::new(ToastId(1), "Profile saved"));
        }
        cx.notify();
    }

    fn field(&self, i: usize, cx: &mut Context<Self>) -> AnyElement {
        let required = matches!(
            i,
            f::FIRST | f::LAST | f::EMAIL | f::AGE | f::COUNTRY | f::CITY | f::POSTAL | f::PASSWORD
        );
        let control: AnyElement = match &self.fields[i] {
            Field::Text(s) => Input::new(s).into_any_element(),
            Field::Area(s) => Textarea::new(s).into_any_element(),
            Field::Country(s) => Select::new(s).placeholder("Choose…").into_any_element(),
            Field::Terms => Checkbox::new("terms")
                .label("I accept the terms")
                .checked(self.terms)
                .on_click(cx.listener(|this, on: &bool, _, cx| {
                    this.terms = *on;
                    this.changed(cx);
                    cx.notify();
                }))
                .into_any_element(),
        };
        let field = form_field()
            .label(LABELS[i])
            .required(required)
            .child(control);
        div()
            .w(px(if i == f::BIO { 632. } else { 300. }))
            .child(v_form().child(field))
            .when_some(self.errors[i], |el, e| {
                el.child(Text::new(e).size(Size::Xs).color(ColorName::Red))
            })
            .into_any_element()
    }
}

impl Render for FormPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fields: Vec<_> = (0..FIELDS).map(|i| self.field(i, cx)).collect();
        div()
            .id("form")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(Title::new("Profile").order(3))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(16.))
                    .max_w(px(640.))
                    .children(fields),
            )
            .child(
                div().child(
                    Button::new("submit")
                        .primary()
                        .label("Save profile")
                        .small()
                        .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                ),
            )
    }
}
