//! guiraffe-components, second batch: progress (Timeline, Stepper, Loader), disclosure
//! (Spoiler, Burger), actions (CopyButton), display (KbdGroup, ColorSwatch, AvatarGroup).

use std::{cell::Cell, rc::Rc};

use gpui_kit::{AnyElement, AnyView, App, IntoElement, ParentElement, Styled, Window, div, px};
use guiraffe_components::*;
use guiraffe_core::{ColorName, Size};

use super::{column, example};
use crate::catalog::Entry;
use crate::events::EventLog;
use crate::knobs::{Knob, KnobValues, playground_with, playground_with_update, with_knobs};

pub fn entries() -> Vec<Entry> {
    vec![
        Entry {
            krate: "components",
            name: "Timeline",
            summary: "Vertical history with reached and upcoming items, titles and descriptions.",
            covers: &["Timeline", "TimelineItem"],
            demo: timeline,
        },
        Entry {
            krate: "components",
            name: "Stepper",
            summary: "Completed, active and upcoming steps on a rail; steps are clickable.",
            covers: &["Stepper", "Step"],
            demo: stepper,
        },
        Entry {
            krate: "components",
            name: "Loader",
            summary: "Oval, bars and dots; animates in its own view, still under reduced motion.",
            covers: &["Loader"],
            demo: loader,
        },
        Entry {
            krate: "components",
            name: "Spoiler",
            summary: "Long content collapsed to a few lines, with an animated reveal.",
            covers: &["Spoiler"],
            demo: spoiler,
        },
        Entry {
            krate: "components",
            name: "Burger",
            summary: "Menu toggle whose bars animate into a close mark.",
            covers: &["Burger"],
            demo: burger,
        },
        Entry {
            krate: "components",
            name: "CopyButton",
            summary: "Copies a value to the clipboard and confirms for two seconds.",
            covers: &["CopyButton"],
            demo: copy_button,
        },
        Entry {
            krate: "components",
            name: "KbdGroup",
            summary: "Keyboard shortcuts: chords (Ctrl + K) and sequences (g then d).",
            covers: &["KbdGroup"],
            demo: kbd,
        },
        Entry {
            krate: "components",
            name: "ColorSwatch",
            summary: "Theme palette chips with a check or ring selection.",
            covers: &["ColorSwatch"],
            demo: swatches,
        },
        Entry {
            krate: "components",
            name: "AvatarGroup",
            summary: "Overlapping avatars with a +N overflow chip.",
            covers: &["AvatarGroup"],
            demo: avatars,
        },
    ]
}

fn timeline(window: &mut Window, cx: &mut App) -> AnyView {
    with_knobs(
        "Live Timeline",
        &[
            Knob::new("Active", &["1", "2", "3", "4"]).default(1),
            Knob::size(),
            Knob::color(),
        ],
        timeline_live,
        |_, _| {
            column([example(
                "Per-item colour",
                div().w(px(360.)).child(
                    Timeline::new([
                        TimelineItem::new("Build passed").description("All 412 tests green"),
                        TimelineItem::new("Review requested")
                            .description("Waiting on two approvals")
                            .color(ColorName::Orange),
                        TimelineItem::new("Release"),
                    ])
                    .active(1)
                    .color(ColorName::Teal)
                    .size(Size::Sm),
                ),
            )])
        },
        window,
        cx,
    )
}

fn timeline_live(v: &KnobValues, _: &mut Window, _: &mut App) -> AnyElement {
    let mut t = Timeline::new([
        TimelineItem::new("New branch").description("feat/fjord-tours created from main"),
        TimelineItem::new("Commits").description("12 commits pushed by Ada"),
        TimelineItem::new("Pull request").description("#482 opened for review"),
        TimelineItem::new("Merged").description("Squashed into main"),
    ])
    .active(v.index(0))
    .size(v.size(1));
    if let Some(c) = v.color(2) {
        t = t.color(c);
    }
    div().w(px(380.)).child(t).into_any_element()
}

fn stepper(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live Stepper",
        &[Knob::size(), Knob::color()],
        |_, cx| (Rc::new(Cell::new(1usize)), EventLog::new(cx)),
        |(active, log), v, _, cx| {
            let preview = cx.entity();
            let (a, l) = (active.clone(), log.clone());
            let mut s = Stepper::new([
                Step::new("Account").description("Name and email"),
                Step::new("Verify").description("Confirm your email"),
                Step::new("Workspace").description("Invite your team"),
                Step::new("Done"),
            ])
            .active(active.get())
            .size(v.size(0))
            .on_step_click(move |i, _, _, cx| {
                a.set(i);
                EventLog::log(&l, format!("on_step_click({i})"), cx);
                preview.update(cx, |_, cx| cx.notify());
            });
            if let Some(c) = v.color(1) {
                s = s.color(c);
            }
            column([
                div().w(px(640.)).child(s).into_any_element(),
                example("Events", log.clone()),
            ])
        },
        window,
        cx,
    )
}

fn loader(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with_update(
        "Live Loader",
        &[
            Knob::new("Type", &["oval", "bars", "dots"]),
            Knob::size(),
            Knob::color(),
        ],
        |_, cx| {
            let all = [LoaderType::Oval, LoaderType::Bars, LoaderType::Dots].map(|k| {
                let l = Loader::new(cx);
                l.update(cx, |l, cx| l.set_kind(k, cx));
                l
            });
            (Loader::new(cx), all)
        },
        |(live, _), v, cx| {
            let kind = v
                .pick(0, &[LoaderType::Oval, LoaderType::Bars, LoaderType::Dots])
                .unwrap_or_default();
            let (size, color) = (v.size(1), v.color(2));
            live.update(cx, |l, cx| {
                l.set_kind(kind, cx);
                l.set_size(Some(size), cx);
                l.set_color(color, cx);
            });
        },
        |(live, all), _, _, _| {
            column([
                div().child(live.clone()).into_any_element(),
                example(
                    "All types",
                    div()
                        .flex()
                        .items_center()
                        .gap(px(24.))
                        .children(all.iter().cloned()),
                ),
            ])
        },
        window,
        cx,
    )
}

const ARTICLE: &str = "Fjords are long, narrow inlets with steep sides, created by glacial \
erosion. Norway's western coast holds more than a thousand of them. The Sognefjord, the \
longest, reaches 205 km inland and 1,308 m deep. Ferries link the villages along its arms, \
and in summer the light lasts until almost midnight. Plan two days for Nærøyfjord alone: the \
cliffs rise straight out of the water and waterfalls appear after every rain.";

fn spoiler(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with_update(
        "Live Spoiler",
        &[
            Knob::new("Collapsed lines", &["1", "2", "3"]).default(1),
            Knob::color(),
        ],
        |_, cx| {
            let s = Spoiler::new(cx);
            s.update(cx, |s, cx| {
                s.set_content(
                    |_, _| Text::new(ARTICLE).size(Size::Sm).into_any_element(),
                    cx,
                )
            });
            s
        },
        |s, v, cx| {
            let (lines, color) = (v.index(0) + 1, v.color(1));
            s.update(cx, |s, cx| {
                s.set_collapsed_lines(lines, cx);
                s.set_color(color, cx);
            });
        },
        |s, _, _, _| div().w(px(420.)).child(s.clone()).into_any_element(),
        window,
        cx,
    )
}

fn burger(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with_update(
        "Live Burger",
        &[Knob::size(), Knob::color()],
        |_, cx| {
            let b = Burger::new(cx);
            let log = EventLog::new(cx);
            let l = log.clone();
            b.update(cx, |b, _| {
                b.on_change(move |open, _, cx| EventLog::log(&l, format!("on_change({open})"), cx))
            });
            (b, log)
        },
        |(b, _), v, cx| {
            let (size, color) = (v.size(0), v.color(1));
            b.update(cx, |b, cx| {
                b.set_size(Some(size), cx);
                b.set_color(color, cx);
            });
        },
        |(b, log), _, _, _| {
            column([
                div().child(b.clone()).into_any_element(),
                example("Events", log.clone()),
            ])
        },
        window,
        cx,
    )
}

fn copy_button(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with_update(
        "Live CopyButton",
        &[
            Knob::new("Label", &["icon", "text"]).default(1),
            Knob::size(),
            Knob::color(),
            Knob::toggle("Disabled"),
        ],
        |_, cx| {
            let b = CopyButton::new(cx);
            let log = EventLog::new(cx);
            let l = log.clone();
            b.update(cx, |b, cx| {
                b.set_value("cargo add guiraffe", cx);
                b.on_copy(move |v, _, cx| EventLog::log(&l, format!("on_copy({v:?})"), cx));
            });
            (b, log)
        },
        |(b, _), v, cx| {
            let label = v.on(0).then(|| "Copy command".into());
            let (size, color, disabled) = (v.size(1), v.color(2), v.on(3));
            b.update(cx, |b, cx| {
                b.set_label(label, cx);
                b.set_size(Some(size), cx);
                b.set_color(color, cx);
                b.set_disabled(disabled, cx);
            });
        },
        |(b, log), _, _, _| {
            column([
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(Text::new("$ cargo add guiraffe").size(Size::Sm))
                    .child(b.clone())
                    .into_any_element(),
                example("Events", log.clone()),
            ])
        },
        window,
        cx,
    )
}

fn kbd(window: &mut Window, cx: &mut App) -> AnyView {
    with_knobs(
        "Live KbdGroup",
        &[
            Knob::size(),
            Knob::new("Variant", &["default", "light", "outline"]),
            Knob::new("Separator", &["plus", "then"]),
        ],
        kbd_live,
        |_, _| {
            column([example(
                "Shortcuts",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(shortcut(
                        "Command palette",
                        KbdGroup::new(["Ctrl", "Shift", "P"]),
                    ))
                    .child(shortcut("Search", KbdGroup::new(["Ctrl", "K"])))
                    .child(shortcut(
                        "Go to dashboard",
                        KbdGroup::new(["g", "d"]).separator(KbdSeparator::Then),
                    )),
            )])
        },
        window,
        cx,
    )
}

fn shortcut(label: &'static str, keys: KbdGroup) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .w(px(320.))
        .child(Text::new(label).size(Size::Sm))
        .child(keys.size(Size::Sm))
        .into_any_element()
}

fn kbd_live(v: &KnobValues, _: &mut Window, _: &mut App) -> AnyElement {
    use guiraffe_core::Variant;
    let variant = v
        .pick(1, &[Variant::Default, Variant::Light, Variant::Outline])
        .unwrap_or(Variant::Default);
    let then = v.on(2);
    let keys = if then {
        KbdGroup::new(["g", "i"]).separator(KbdSeparator::Then)
    } else {
        KbdGroup::new(["Ctrl", "Alt", "Delete"])
    };
    keys.size(v.size(0)).variant(variant).into_any_element()
}

const SWATCH_COLORS: [(&str, ColorName); 8] = [
    ("Blue", ColorName::Blue),
    ("Teal", ColorName::Teal),
    ("Green", ColorName::Green),
    ("Yellow", ColorName::Yellow),
    ("Orange", ColorName::Orange),
    ("Red", ColorName::Red),
    ("Grape", ColorName::Grape),
    ("Gray", ColorName::Gray),
];

fn swatches(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live ColorSwatch",
        &[
            Knob::size(),
            Knob::new("Selection", &["check", "ring"]),
            Knob::toggle("Labels"),
        ],
        |_, cx| (Rc::new(Cell::new(1usize)), EventLog::new(cx)),
        |(chosen, log), v, _, cx| {
            let preview = cx.entity();
            let style = if v.on(1) {
                SwatchSelected::Ring
            } else {
                SwatchSelected::Check
            };
            let labels = v.on(2);
            let row = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(10.))
                .children(SWATCH_COLORS.iter().enumerate().map(|(i, (name, color))| {
                    let (c, l, p) = (chosen.clone(), log.clone(), preview.clone());
                    let name = *name;
                    let mut s = ColorSwatch::from_color(("swatch", i), *color, cx)
                        .size(v.size(0))
                        .selected(if chosen.get() == i {
                            style
                        } else {
                            SwatchSelected::None
                        })
                        .on_click(move |_, _, cx| {
                            c.set(i);
                            EventLog::log(&l, format!("picked {name}"), cx);
                            p.update(cx, |_, cx| cx.notify());
                        });
                    if labels {
                        s = s.label(name);
                    }
                    s
                }));
            column([row.into_any_element(), example("Events", log.clone())])
        },
        window,
        cx,
    )
}

const TEAM: [&str; 7] = [
    "Ada Lovelace",
    "Grace Hopper",
    "Alan Turing",
    "Katherine Johnson",
    "Linus Torvalds",
    "Margaret Hamilton",
    "Dennis Ritchie",
];

fn avatars(window: &mut Window, cx: &mut App) -> AnyView {
    with_knobs(
        "Live AvatarGroup",
        &[
            Knob::new("Limit", &["2", "4", "all"]).default(1),
            Knob::size(),
            Knob::color(),
        ],
        avatars_live,
        |_, _| {
            column([example(
                "Small team, no overflow",
                AvatarGroup::new(TEAM.iter().take(3).map(|n| Person::new(*n))).id("small"),
            )])
        },
        window,
        cx,
    )
}

fn avatars_live(v: &KnobValues, _: &mut Window, _: &mut App) -> AnyElement {
    let limit = v.pick(0, &[2, 4, usize::MAX]).unwrap_or(4);
    let mut g = AvatarGroup::new(TEAM.map(Person::new))
        .id("live")
        .limit(limit)
        .size(v.size(1));
    if let Some(c) = v.color(2) {
        g = g.color(c);
    }
    g.into_any_element()
}
