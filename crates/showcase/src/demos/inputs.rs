//! guiraffe-inputs: chips, tags, range slider, password strength, dropzone.

use gpui_kit::{
    AnyElement, AnyView, App, Entity, IntoElement, ParentElement, Styled, Window, div, px,
};
use guiraffe_components::Text;
use guiraffe_core::{ColorName, Size};
use guiraffe_inputs::*;

use super::{column, example};
use crate::catalog::{Entry, stateless};
use crate::events::EventLog;
use crate::knobs::{Applied, Knob, playground_with};

pub fn entries() -> Vec<Entry> {
    vec![
        Entry {
            krate: "inputs",
            name: "ChipGroup",
            summary: "Toggleable chips with single or multiple selection.",
            covers: &["ChipGroup"],
            demo: chips,
        },
        Entry {
            krate: "inputs",
            name: "TagsInput",
            summary: "Free-form tags: Enter or comma commits, Backspace removes, pasted lists split; duplicates and limits are rejected.",
            covers: &["TagsInput"],
            demo: tags,
        },
        Entry {
            krate: "inputs",
            name: "RangeSlider",
            summary: "Two thumbs on one track, stepped; drag or use the keyboard.",
            covers: &["RangeSlider"],
            demo: range,
        },
        Entry {
            krate: "inputs",
            name: "PasswordStrength",
            summary: "Meter and checklist scoring length, case, digits and symbols.",
            covers: &["PasswordStrength"],
            demo: strength,
        },
        Entry {
            krate: "inputs",
            name: "Dropzone",
            summary: "File drop target with an accept filter; emits accepted and rejected paths.",
            covers: &["Dropzone"],
            demo: dropzone,
        },
    ]
}

/// A stateful widget under knobs, with its event log beside it.
fn widget_and_log(widget: impl IntoElement, log: &Entity<EventLog>, width: f32) -> AnyElement {
    div()
        .flex()
        .gap(px(Size::Xl.spacing()))
        .child(div().w(px(width)).child(widget))
        .child(div().flex_1().child(log.clone()))
        .into_any_element()
}

fn chips(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live ChipGroup",
        &[
            Knob::color(),
            Knob::toggle("Disabled"),
            Knob::toggle("Focusable"),
        ],
        |_, cx| {
            let multi = ChipGroup::new(["Rust", "TypeScript", "Go", "Zig", "Swift"], true, cx);
            multi.update(cx, |c, cx| {
                c.toggle(0, cx);
                c.toggle(3, cx);
            });
            let single = ChipGroup::new(["Daily", "Weekly", "Monthly"], false, cx);
            single.update(cx, |c, cx| {
                c.toggle(1, cx);
                c.set_color(ColorName::Teal, cx);
            });
            let log = EventLog::new(cx);
            EventLog::watch_with(
                &log,
                &multi,
                |e: &ChipEvent| {
                    let ChipEvent::Changed(v) = e;
                    format!("languages Changed({v:?})").into()
                },
                cx,
            );
            EventLog::watch_with(
                &log,
                &single,
                |e: &ChipEvent| {
                    let ChipEvent::Changed(v) = e;
                    format!("frequency Changed({v:?})").into()
                },
                cx,
            );
            (multi, single, log, Applied::default())
        },
        |(multi, single, log, applied), v, _, cx| {
            if applied.changed(v) {
                let (color, disabled, focusable) =
                    (v.color(0).unwrap_or(ColorName::Blue), v.on(1), v.on(2));
                multi.update(cx, |c, cx| {
                    c.set_color(color, cx);
                    c.set_disabled(disabled, cx);
                    c.set_focusable(Some(focusable), cx);
                });
            }
            column([
                widget_and_log(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(Size::Sm.spacing()))
                        .child(Text::new("Multiple: languages you use").size(Size::Sm))
                        .child(multi.clone()),
                    log,
                    420.,
                ),
                example("Single (teal, fixed): digest frequency", single.clone()),
            ])
        },
        window,
        cx,
    )
}

fn tags(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live TagsInput (max 6, comma or space commits, no duplicates)",
        &[],
        |window, cx| {
            let rules = TagRules {
                max_tags: Some(6),
                split_chars: vec![',', ' '],
                ..TagRules::default()
            };
            let tags = TagsInput::new(rules, "Add a topic…", window, cx);
            tags.update(cx, |t, cx| {
                for tag in ["gpui", "rust", "design-systems"] {
                    t.add(tag, cx);
                }
            });
            let empty = TagsInput::new(
                TagRules::default(),
                "Placeholder: type and press Enter",
                window,
                cx,
            );
            let log = EventLog::new(cx);
            EventLog::watch_with(
                &log,
                &tags,
                |e: &TagsEvent| match e {
                    TagsEvent::Changed(t) => format!("Changed: {} of 6", t.len()).into(),
                    TagsEvent::Rejected(tag, why) => format!("Rejected “{tag}”: {why:?}").into(),
                },
                cx,
            );
            (tags, empty, log)
        },
        |(tags, empty, log), _, _, _| {
            column([
                widget_and_log(tags.clone(), log, 420.),
                example(
                    "Empty, showing its placeholder",
                    div().w(px(420.)).child(empty.clone()),
                ),
            ])
        },
        window,
        cx,
    )
}

fn range(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live RangeSlider (step $10; drag or arrow keys)",
        &[
            Knob::toggle("Disabled"),
            Knob::toggle("Focusable").default(1),
        ],
        |_, cx| {
            let slider = RangeSlider::new(Range2::new(0.0, 500.0, 10.0, 120.0, 340.0), cx);
            slider.update(cx, |s, cx| s.set_label("Price range", cx));
            let log = EventLog::new(cx);
            EventLog::watch_with(
                &log,
                &slider,
                |e: &RangeEvent| {
                    let RangeEvent::Changed(lo, hi) = e;
                    format!("Changed(${lo:.0}, ${hi:.0})").into()
                },
                cx,
            );
            (slider, log, Applied::default())
        },
        |(slider, log, applied), v, _, cx| {
            if applied.changed(v) {
                let (disabled, focusable) = (v.on(0), v.on(1));
                slider.update(cx, |s, cx| {
                    s.set_disabled(disabled, cx);
                    s.set_focusable(Some(focusable), cx);
                });
            }
            widget_and_log(slider.clone(), log, 420.)
        },
        window,
        cx,
    )
}

fn strength(_: &mut Window, cx: &mut App) -> AnyView {
    stateless(
        |_, _| {
            let row = |pw: &'static str| -> AnyElement {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(Size::Xs.spacing()))
                    .child(Text::new(format!("“{pw}”")).size(Size::Sm))
                    .child(PasswordStrength::new(pw))
                    .into_any_element()
            };
            column([
                example("Weak", row("hunter2")),
                example("Fair", row("Sunflower22")),
                example("Strong", row("Correct-Horse-Battery-9")),
            ])
        },
        cx,
    )
}

fn dropzone(window: &mut Window, cx: &mut App) -> AnyView {
    playground_with(
        "Live Dropzone (accepts image/* and .pdf)",
        &[
            Knob::toggle("Disabled"),
            Knob::toggle("Focusable").default(1),
        ],
        |_, cx| {
            let zone = Dropzone::new(vec!["image/*", ".pdf"], "Drop screenshots or PDFs here", cx);
            let log = EventLog::new(cx);
            EventLog::watch_with(
                &log,
                &zone,
                |e: &DropEvent| {
                    let names = |p: &[std::path::PathBuf]| {
                        p.iter()
                            .filter_map(|p| {
                                p.file_name().and_then(|n| n.to_str()).map(str::to_owned)
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    match e {
                        DropEvent::Accepted(p) => format!("Accepted: {}", names(p)).into(),
                        DropEvent::Rejected(p) => format!("Rejected: {}", names(p)).into(),
                        DropEvent::BrowseRequested => "BrowseRequested (open a file picker)".into(),
                    }
                },
                cx,
            );
            (zone, log, Applied::default())
        },
        |(zone, log, applied), v, _, cx| {
            if applied.changed(v) {
                let (disabled, focusable) = (v.on(0), v.on(1));
                zone.update(cx, |z, cx| {
                    z.set_disabled(disabled, cx);
                    z.set_focusable(Some(focusable), cx);
                });
            }
            widget_and_log(zone.clone(), log, 420.)
        },
        window,
        cx,
    )
}
