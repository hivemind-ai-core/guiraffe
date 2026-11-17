//! GPUI Controls (gpui-kit's `component` layer), themed by GUIraffe's bridge
//! (`guiraffe_core::apply`). These are Kit types, so the coverage scanner skips `kit` entries.

use gpui_kit::{
    AnyElement, AnyView, App, AppContext, Context, Entity, IntoElement, Keystroke, ParentElement,
    Render, SharedString, Styled, Window,
    assets::IconName,
    component::{
        Disableable, Icon, IndexPath, Selectable, Sizable,
        accordion::Accordion,
        alert::Alert,
        avatar::{Avatar, AvatarGroup},
        badge::Badge,
        breadcrumb::{Breadcrumb, BreadcrumbItem},
        button::{Button, ButtonGroup, ButtonVariants},
        checkbox::Checkbox,
        date_picker::{DatePicker, DatePickerState},
        form::{field as form_field, v_form},
        input::{Input, InputGroup, InputState, NumberInput, Textarea, TextareaState},
        kbd::Kbd,
        menu::{DropdownMenu, PopupMenuItem},
        pagination::Pagination,
        progress::{Progress, ProgressCircle},
        radio::{Radio, RadioGroup},
        select::{Select, SelectState},
        skeleton::Skeleton,
        slider::{Slider, SliderState},
        spinner::Spinner,
        switch::Switch,
        tab::{Tab, TabBar},
        table::{Table, TableBody, TableCell, TableHead, TableHeader, TableRow},
        tag::Tag,
    },
    div, px,
};
use guiraffe_core::kit_size;

use super::{column, example};
use crate::{
    catalog::{Entry, stateless},
    knobs::{Knob, KnobValues, Preview, playground_with},
};

fn entry(
    name: &'static str,
    summary: &'static str,
    demo: fn(&mut Window, &mut App) -> AnyView,
) -> Entry {
    Entry {
        krate: "kit",
        name,
        summary,
        covers: &[],
        demo,
    }
}

pub fn entries() -> Vec<Entry> {
    vec![
        entry(
            "Kit Button",
            "Controls' Button: variants, sizes, icons, loading, disabled, groups and dropdowns. The primary color and radius come from the GUIraffe theme.",
            buttons,
        ),
        entry(
            "Kit Text input",
            "Input, password with reveal toggle, number stepper and textarea.",
            inputs,
        ),
        entry(
            "Kit Select",
            "Single choice from a list, optionally searchable and clearable.",
            select,
        ),
        entry(
            "Kit Checkbox, Radio & Switch",
            "Boolean and single-choice controls; click to toggle.",
            toggles,
        ),
        entry(
            "Kit Slider",
            "Single value, range and vertical sliders.",
            sliders,
        ),
        entry(
            "Kit Date picker",
            "Single date and range pickers with a calendar popup.",
            dates,
        ),
        entry(
            "Kit Tabs",
            "TabBar in underline, pill, outline and segmented styles.",
            tabs,
        ),
        entry(
            "Kit Accordion",
            "Collapsible sections; one or many open.",
            accordion,
        ),
        entry(
            "Kit Dropdown menu",
            "A button that opens a popup menu.",
            menu,
        ),
        entry(
            "Kit Table",
            "Static table with header and body rows.",
            table,
        ),
        entry(
            "Kit Badge & Tag",
            "Count, dot and icon badges; colored tags.",
            badges,
        ),
        entry(
            "Kit Progress & loading",
            "Bars, circles, spinners and skeletons.",
            progress,
        ),
        entry(
            "Kit Avatar & Kbd",
            "Initials avatars, groups and keyboard shortcuts.",
            avatars,
        ),
        entry(
            "Kit Alert",
            "Info, success, warning and error callouts.",
            alerts,
        ),
        entry(
            "Kit Breadcrumb & Pagination",
            "Navigation trails and page pickers.",
            navigation,
        ),
    ]
}

fn row() -> gpui_kit::Div {
    div().flex().flex_wrap().items_center().gap(px(10.))
}

// ---- Buttons ----------------------------------------------------------------------------

const BUTTON_VARIANTS: &[&str] = &[
    "primary",
    "secondary",
    "danger",
    "success",
    "warning",
    "info",
    "ghost",
    "link",
    "outline",
];

fn styled_button(b: Button, variant: usize) -> Button {
    match variant {
        0 => b.primary(),
        2 => b.danger(),
        3 => b.success(),
        4 => b.warning(),
        5 => b.info(),
        6 => b.ghost(),
        7 => b.link(),
        8 => b.outline(),
        _ => b,
    }
}

fn buttons(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Button",
        &[
            Knob::new("Variant", BUTTON_VARIANTS),
            Knob::new("Size", &["xs", "sm", "md", "lg"]).default(2),
            Knob::toggle("Icon"),
            Knob::toggle("Loading"),
            Knob::toggle("Disabled"),
        ],
        |_, _| (),
        |_: &(), v: &KnobValues, _, _| {
            let size = kit_size(v.size(1));
            let make = |id: &'static str, label: &'static str| {
                let b = Button::new(id)
                    .label(label)
                    .with_size(size)
                    .loading(v.on(3))
                    .disabled(v.on(4));
                let b = if v.on(2) {
                    b.icon(Icon::new(IconName::Plus))
                } else {
                    b
                };
                styled_button(b, v.index(0))
            };
            column([
                row()
                    .child(make("b-save", "Save changes"))
                    .child(make("b-new", "New project"))
                    .into_any_element(),
                example(
                    "All variants at this size",
                    row().children(BUTTON_VARIANTS.iter().enumerate().map(|(i, name)| {
                        styled_button(Button::new(("v", i)).label(*name).with_size(size), i)
                    })),
                ),
                example(
                    "Group, icon-only and dropdown",
                    row()
                        .child(
                            ButtonGroup::new("align")
                                .outline()
                                .with_size(size)
                                .child(Button::new("l").label("Left").selected(true))
                                .child(Button::new("c").label("Center"))
                                .child(Button::new("r").label("Right")),
                        )
                        .child(
                            Button::new("ico")
                                .icon(Icon::new(IconName::Settings))
                                .ghost()
                                .with_size(size)
                                .tooltip("Settings"),
                        )
                        .child(
                            Button::new("more")
                                .label("More")
                                .dropdown_caret(true)
                                .with_size(size)
                                .dropdown_menu(|m, _, _| {
                                    m.item(PopupMenuItem::new("Rename"))
                                        .item(PopupMenuItem::new("Duplicate"))
                                        .separator()
                                        .item(PopupMenuItem::new("Delete"))
                                }),
                        ),
                ),
            ])
        },
        window,
        cx,
    )
}

// ---- Inputs -----------------------------------------------------------------------------

struct InputDemo {
    text: Entity<InputState>,
    search: Entity<InputState>,
    password: Entity<InputState>,
    number: Entity<InputState>,
    area: Entity<TextareaState>,
    email: Entity<InputState>,
}

fn inputs(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Inputs",
        &[
            Knob::new("Size", &["xs", "sm", "md", "lg"]).default(2),
            Knob::toggle("Disabled"),
            Knob::toggle("Cleanable").default(1),
            Knob::new("Validation", &["none", "required", "error", "read-only"]).default(2),
        ],
        |window, cx| InputDemo {
            text: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Your name")
                    .default_value("Ada Lovelace")
            }),
            // Each field owns its state: sharing one InputState shares focus and text.
            search: cx.new(|cx| InputState::new(window, cx).placeholder("Search components")),
            password: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .default_value("hunter2")
            }),
            number: cx.new(|cx| InputState::new(window, cx).default_value("42")),
            area: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .rows(3)
                    .default_value("Multi-line text grows with its content.\nSecond line.")
            }),
            email: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("you@company.com")
                    .default_value("ada@")
            }),
        },
        |s: &InputDemo, v: &KnobValues, _, _| {
            let size = kit_size(v.size(0));
            let off = v.on(1);
            let field = |label: &'static str, el: AnyElement| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .w(px(300.))
                    .child(guiraffe_components::Text::new(label).size(guiraffe_core::Size::Sm))
                    .child(el)
            };
            div()
                .flex()
                .flex_wrap()
                .gap(px(16.))
                .child(field(
                    "Text",
                    Input::new(&s.text)
                        .with_size(size)
                        .cleanable(v.on(2))
                        .disabled(off)
                        .into_any_element(),
                ))
                .child(field(
                    "Password",
                    Input::new(&s.password)
                        .with_size(size)
                        .mask_toggle()
                        .disabled(off)
                        .into_any_element(),
                ))
                .child(field(
                    "Search with prefix",
                    Input::new(&s.search)
                        .with_size(size)
                        .prefix(Icon::new(IconName::Search))
                        .disabled(off)
                        .into_any_element(),
                ))
                .child(field(
                    "Number",
                    NumberInput::new(&s.number)
                        .with_size(size)
                        .disabled(off)
                        .into_any_element(),
                ))
                .child(field(
                    "Textarea",
                    Textarea::new(&s.area).disabled(off).into_any_element(),
                ))
                .child(div().w(px(300.)).child({
                    let mode = v.index(3);
                    let description = match mode {
                        1 => "We send the invoice here.",
                        2 => "Enter a complete address, like ada@example.com.",
                        3 => "Managed by your organization.",
                        _ => "Optional.",
                    };
                    v_form().child(
                        form_field()
                            .label("Work email")
                            .required(mode >= 1)
                            .description(description)
                            .child(
                                InputGroup::new("email-group")
                                    .input(Input::new(&s.email).with_size(size))
                                    .invalid(mode == 2)
                                    .readonly(mode == 3)
                                    .disabled(off),
                            ),
                    )
                }))
                .into_any_element()
        },
        window,
        cx,
    )
}

// ---- Select -----------------------------------------------------------------------------

struct SelectDemo {
    fruit: Entity<SelectState<Vec<&'static str>>>,
    city: Entity<SelectState<Vec<&'static str>>>,
}

fn select(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Select",
        &[
            Knob::new("Size", &["xs", "sm", "md", "lg"]).default(2),
            Knob::toggle("Cleanable"),
            Knob::toggle("Disabled"),
        ],
        |window, cx| SelectDemo {
            fruit: cx.new(|cx| {
                SelectState::new(
                    vec!["Apple", "Banana", "Cherry", "Mango"],
                    Some(IndexPath::new(1)),
                    window,
                    cx,
                )
            }),
            city: cx.new(|cx| {
                SelectState::new(
                    vec!["Berlin", "Lagos", "Lima", "Osaka", "Oslo", "Perth"],
                    None,
                    window,
                    cx,
                )
                .searchable(true)
            }),
        },
        |s: &SelectDemo, v: &KnobValues, _, _| {
            let size = kit_size(v.size(0));
            row()
                .child(
                    div().w(px(220.)).child(
                        Select::new(&s.fruit)
                            .with_size(size)
                            .cleanable(v.on(1))
                            .disabled(v.on(2)),
                    ),
                )
                .child(
                    div().w(px(220.)).child(
                        Select::new(&s.city)
                            .with_size(size)
                            .placeholder("Searchable city…")
                            .cleanable(v.on(1))
                            .disabled(v.on(2)),
                    ),
                )
                .into_any_element()
        },
        window,
        cx,
    )
}

// ---- Toggles ----------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct ToggleState {
    terms: bool,
    news: bool,
    plan: usize,
    wifi: bool,
    bluetooth: bool,
}

fn toggles(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Toggles",
        &[
            Knob::new("Size", &["xs", "sm", "md", "lg"]).default(2),
            Knob::toggle("Disabled"),
        ],
        |_, _| ToggleState {
            terms: true,
            news: false,
            plan: 1,
            wifi: true,
            bluetooth: false,
        },
        |st: &ToggleState, v: &KnobValues, _, cx: &mut Context<Preview<ToggleState>>| {
            let size = kit_size(v.size(0));
            let off = v.on(1);
            let st = *st;
            let s = &cx.entity();
            let set = |s: &Entity<Preview<ToggleState>>, f: fn(&mut ToggleState, bool)| {
                let s = s.clone();
                move |on: &bool, _: &mut Window, cx: &mut App| {
                    s.update(cx, |p, cx| {
                        f(&mut p.state, *on);
                        cx.notify();
                    })
                }
            };
            let s2 = s.clone();
            column([
                example(
                    "Checkbox",
                    row()
                        .gap(px(20.))
                        .child(
                            Checkbox::new("terms")
                                .label("Accept terms")
                                .checked(st.terms)
                                .with_size(size)
                                .disabled(off)
                                .on_click(set(s, |s, on| s.terms = on)),
                        )
                        .child(
                            Checkbox::new("news")
                                .label("Send me news")
                                .checked(st.news)
                                .with_size(size)
                                .disabled(off)
                                .on_click(set(s, |s, on| s.news = on)),
                        ),
                ),
                example(
                    "RadioGroup",
                    RadioGroup::horizontal("plan")
                        .children(
                            ["Free", "Pro", "Team"].map(|l| Radio::new(l).label(l).with_size(size)),
                        )
                        .selected_index(Some(st.plan))
                        .disabled(off)
                        .on_click(move |i, _, cx| {
                            s2.update(cx, |p, cx| {
                                p.state.plan = *i;
                                cx.notify();
                            })
                        }),
                ),
                example(
                    "Switch",
                    row()
                        .gap(px(20.))
                        .child(
                            Switch::new("wifi")
                                .label("Wi-Fi")
                                .checked(st.wifi)
                                .with_size(size)
                                .disabled(off)
                                .on_click(set(s, |s, on| s.wifi = on)),
                        )
                        .child(
                            Switch::new("bt")
                                .label("Bluetooth")
                                .checked(st.bluetooth)
                                .with_size(size)
                                .disabled(off)
                                .on_click(set(s, |s, on| s.bluetooth = on)),
                        ),
                ),
            ])
        },
        window,
        cx,
    )
}

// ---- Sliders ----------------------------------------------------------------------------

struct Sliders {
    volume: Entity<SliderState>,
    price: Entity<SliderState>,
    eq: Entity<SliderState>,
}

impl Render for Sliders {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([
            example(
                "Single (0–100, step 1)",
                div().w(px(360.)).child(Slider::new(&self.volume)),
            ),
            example(
                "Range (price 20–80)",
                div().w(px(360.)).child(Slider::new(&self.price)),
            ),
            example(
                "Vertical and disabled",
                row()
                    .gap(px(40.))
                    .child(div().h(px(120.)).child(Slider::new(&self.eq).vertical()))
                    .child(
                        div()
                            .w(px(200.))
                            .child(Slider::new(&self.volume).disabled(true)),
                    ),
            ),
        ])
    }
}

fn sliders(_: &mut Window, cx: &mut App) -> AnyView {
    let volume = cx.new(|_| SliderState::new().min(0.).max(100.).default_value(60.));
    let price = cx.new(|_| {
        SliderState::new()
            .min(0.)
            .max(100.)
            .default_value(20.0..80.0)
    });
    let eq = cx.new(|_| SliderState::new().min(0.).max(10.).default_value(7.));
    cx.new(|_| Sliders { volume, price, eq }).into()
}

// ---- Dates ------------------------------------------------------------------------------

struct Dates {
    single: Entity<DatePickerState>,
    range: Entity<DatePickerState>,
}

impl Render for Dates {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        row()
            .gap(px(20.))
            .child(example(
                "Single date",
                div()
                    .w(px(240.))
                    .child(DatePicker::new(&self.single).placeholder("Pick a date")),
            ))
            .child(example(
                "Range, two months",
                div().w(px(300.)).child(
                    DatePicker::new(&self.range)
                        .number_of_months(2)
                        .placeholder("Start – end")
                        .cleanable(true),
                ),
            ))
    }
}

fn dates(window: &mut Window, cx: &mut App) -> AnyView {
    let single = cx.new(|cx| DatePickerState::new(window, cx));
    let range = cx.new(|cx| DatePickerState::range(window, cx));
    cx.new(|_| Dates { single, range }).into()
}

// ---- Tabs -------------------------------------------------------------------------------

struct Tabs {
    selected: usize,
}

impl Render for Tabs {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let labels = ["Overview", "Activity", "Settings"];
        let bar = |id: &'static str, cx: &mut Context<Self>| {
            let e = cx.entity();
            TabBar::new(id)
                .selected_index(self.selected)
                .children(labels.map(|l| Tab::new().label(l)))
                .on_click(move |i, _, cx| {
                    e.update(cx, |t, cx| {
                        t.selected = *i;
                        cx.notify();
                    })
                })
        };
        column([
            example("Underline", bar("t-u", cx).underline()),
            example("Pill", bar("t-p", cx).pill()),
            example("Outline", bar("t-o", cx).outline()),
            example("Segmented, small", bar("t-s", cx).segmented().small()),
        ])
    }
}

fn tabs(_: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|_| Tabs { selected: 0 }).into()
}

// ---- Accordion --------------------------------------------------------------------------

struct AccordionDemo {
    open: Vec<usize>,
}

impl Render for AccordionDemo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let e = cx.entity();
        let items = [
            ("What is GUIraffe?", "Mantine-style widgets on GPUI."),
            (
                "Does it theme Kit controls?",
                "Yes: apply() syncs the Kit theme.",
            ),
            ("Can several be open?", "With multiple(true), yes."),
        ];
        let mut acc = Accordion::new("faq")
            .multiple(true)
            .on_toggle_click(move |open, _, cx| {
                e.update(cx, |a, cx| {
                    a.open = open.to_vec();
                    cx.notify();
                })
            });
        for (i, (q, a)) in items.into_iter().enumerate() {
            let open = self.open.contains(&i);
            acc = acc.item(move |it| it.title(q).open(open).child(a));
        }
        div().w(px(480.)).child(acc)
    }
}

fn accordion(_: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|_| AccordionDemo { open: vec![0] }).into()
}

// ---- Stateless families -----------------------------------------------------------------

fn menu(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, _| {
            example(
                "Click to open",
                row()
                    .child(
                        Button::new("file")
                            .label("File")
                            .dropdown_caret(true)
                            .dropdown_menu(|m, _, _| {
                                m.label("Document")
                                    .item(PopupMenuItem::new("New"))
                                    .item(PopupMenuItem::new("Open…"))
                                    .separator()
                                    .item(PopupMenuItem::new("Export as PDF"))
                            }),
                    )
                    .child(
                        Button::new("acct")
                            .icon(Icon::new(IconName::Settings))
                            .ghost()
                            .dropdown_menu(|m, _, _| {
                                m.item(PopupMenuItem::new("Profile"))
                                    .item(PopupMenuItem::new("Sign out"))
                            }),
                    ),
            )
        },
        cx,
    )
}

fn table(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, _| {
            let rows = [
                ("INV-001", "Paid", "Card", "$250.00"),
                ("INV-002", "Pending", "PayPal", "$150.00"),
                ("INV-003", "Unpaid", "Transfer", "$350.00"),
                ("INV-004", "Paid", "Card", "$450.00"),
            ];
            let head = ["Invoice", "Status", "Method", "Amount"];
            example(
                "Invoices",
                div().w(px(560.)).child(
                    Table::new()
                        .child(
                            TableHeader::new().child(
                                head.into_iter().fold(TableRow::new(), |r, h| {
                                    r.child(TableHead::new().child(h))
                                }),
                            ),
                        )
                        .child(
                            rows.into_iter()
                                .fold(TableBody::new(), |b, (a, s, m, amt)| {
                                    b.child(
                                        TableRow::new()
                                            .child(TableCell::new().child(a))
                                            .child(TableCell::new().child(s))
                                            .child(TableCell::new().child(m))
                                            .child(TableCell::new().child(amt)),
                                    )
                                }),
                        ),
                ),
            )
        },
        cx,
    )
}

fn badges(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, _| {
            column([
                example(
                    "Badge on a button",
                    row()
                        .gap(px(24.))
                        .child(
                            Badge::new()
                                .count(3)
                                .child(Button::new("inbox").label("Inbox")),
                        )
                        .child(
                            Badge::new()
                                .count(120)
                                .max(99)
                                .child(Button::new("n").label("Notifications")),
                        )
                        .child(
                            Badge::new()
                                .dot()
                                .child(Button::new("d").icon(Icon::new(IconName::Bell)).ghost()),
                        ),
                ),
                example(
                    "Tags",
                    row()
                        .child(Tag::primary().child("Primary"))
                        .child(Tag::secondary().child("Secondary"))
                        .child(Tag::success().child("Shipped"))
                        .child(Tag::warning().child("Pending"))
                        .child(Tag::danger().child("Failed"))
                        .child(Tag::info().child("Info"))
                        .child(Tag::primary().outline().child("Outline"))
                        .child(Tag::success().rounded_full().child("Rounded")),
                ),
            ])
        },
        cx,
    )
}

fn progress(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, _| {
            column([
                example(
                    "Progress bars",
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .w(px(360.))
                        .child(Progress::new("p1").value(25.))
                        .child(Progress::new("p2").value(60.))
                        .child(Progress::new("p3").value(90.).small()),
                ),
                example(
                    "Circles and spinners",
                    row()
                        .gap(px(20.))
                        .child(ProgressCircle::new("c1").value(35.))
                        .child(ProgressCircle::new("c2").value(75.).large())
                        .child(Spinner::new())
                        .child(Spinner::new().large()),
                ),
                example(
                    "Skeleton",
                    div()
                        .flex()
                        .gap(px(12.))
                        .child(Skeleton::new().size(px(48.)).rounded_full())
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .child(Skeleton::new().w(px(240.)).h(px(14.)))
                                .child(Skeleton::new().secondary().w(px(180.)).h(px(14.))),
                        ),
                ),
            ])
        },
        cx,
    )
}

fn avatars(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, _| {
            let people = [
                "Ada Lovelace",
                "Alan Turing",
                "Grace Hopper",
                "Edsger Dijkstra",
                "Barbara Liskov",
            ];
            column([
                example(
                    "Initials at each size",
                    row()
                        .child(Avatar::new().name("Ada Lovelace").xsmall())
                        .child(Avatar::new().name("Alan Turing").small())
                        .child(Avatar::new().name("Grace Hopper"))
                        .child(Avatar::new().name("Linus Torvalds").large())
                        .child(Avatar::new().placeholder(Icon::new(IconName::User))),
                ),
                example(
                    "Group, limited to 3",
                    div().flex().child(
                        AvatarGroup::new()
                            .limit(3)
                            .ellipsis()
                            .children(people.map(|p| Avatar::new().name(p))),
                    ),
                ),
                example(
                    "Keyboard shortcuts",
                    row().children(
                        ["ctrl-k", "ctrl-shift-p", "escape", "alt-enter"]
                            .into_iter()
                            .filter_map(|k| Keystroke::parse(k).ok())
                            .map(Kbd::new),
                    ),
                ),
            ])
        },
        cx,
    )
}

fn alerts(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, _| {
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .w(px(560.))
                .child(Alert::new("a0", "Your trial ends in 3 days.").title("Heads up"))
                .child(Alert::info("a1", "A new version is available."))
                .child(Alert::success("a2", "Changes saved.").title("Saved"))
                .child(Alert::warning("a3", "Disk is 90% full."))
                .child(Alert::error("a4", "Payment failed. Check your card.").title("Error"))
                .into_any_element()
        },
        cx,
    )
}

struct Nav {
    page: usize,
}

impl Render for Nav {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let e = cx.entity();
        let crumbs: SharedString = format!("Page {}", self.page).into();
        column([
            example(
                "Breadcrumb",
                Breadcrumb::new()
                    .child("Home")
                    .child(BreadcrumbItem::new("Projects"))
                    .child(BreadcrumbItem::new("Archived").disabled(true))
                    .child(crumbs),
            ),
            example(
                "Pagination (click a page)",
                Pagination::new("pages")
                    .current_page(self.page)
                    .total_pages(12)
                    .visible_pages(5)
                    .on_click(move |p, _, cx| {
                        e.update(cx, |n, cx| {
                            n.page = *p;
                            cx.notify();
                        })
                    }),
            ),
        ])
    }
}

fn navigation(_: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|_| Nav { page: 4 }).into()
}
