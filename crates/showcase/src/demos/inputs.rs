//! guiraffe-inputs: chips, tags, range slider, password strength, dropzone.

use gpui_kit::{
    AnyElement, AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window, div, px,
};
use guiraffe_components::Text;
use guiraffe_core::{ColorName, Size};
use guiraffe_inputs::*;

use super::{column, example};
use crate::catalog::{Entry, stateless};

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

/// A live caption under a stateful widget, updated from its events.
struct Live<V: 'static> {
    widget: Entity<V>,
    label: &'static str,
    caption: SharedString,
    _sub: Option<Subscription>,
}

impl<V: Render> Render for Live<V> {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([example(
            self.label,
            div()
                .flex()
                .flex_col()
                .gap(px(Size::Sm.spacing()))
                .child(self.widget.clone())
                .child(Text::new(self.caption.clone()).size(Size::Sm).dimmed(true)),
        )])
    }
}

fn chips(_: &mut Window, cx: &mut App) -> AnyView {
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
    cx.new(|_| Chips(multi, single)).into()
}

struct Chips(Entity<ChipGroup>, Entity<ChipGroup>);

impl Render for Chips {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([
            example("Multiple: languages you use", self.0.clone()),
            example("Single (teal): digest frequency", self.1.clone()),
        ])
    }
}

fn tags(window: &mut Window, cx: &mut App) -> AnyView {
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
    cx.new(|cx| {
        let sub = cx.subscribe(&tags, |this: &mut Live<TagsInput>, _, e, cx| {
            this.caption = match e {
                TagsEvent::Changed(t) => format!("{} of 6 tags", t.len()).into(),
                TagsEvent::Rejected(tag, why) => format!("rejected “{tag}”: {why:?}").into(),
            };
            cx.notify();
        });
        Live {
            widget: tags,
            label: "Topics (max 6, comma or space commits)",
            caption: "3 of 6 tags".into(),
            _sub: Some(sub),
        }
    })
    .into()
}

fn range(_: &mut Window, cx: &mut App) -> AnyView {
    let slider = RangeSlider::new(Range2::new(0.0, 500.0, 10.0, 120.0, 340.0), cx);
    cx.new(|cx| {
        let sub = cx.subscribe(&slider, |this: &mut Live<RangeSlider>, s, _, cx| {
            let (lo, hi) = s.read(cx).values();
            this.caption = format!("${lo:.0} – ${hi:.0}").into();
            cx.notify();
        });
        Live {
            widget: slider,
            label: "Price range (step $10)",
            caption: "$120 – $340".into(),
            _sub: Some(sub),
        }
    })
    .into()
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

fn dropzone(_: &mut Window, cx: &mut App) -> AnyView {
    let zone = Dropzone::new(vec!["image/*", ".pdf"], "Drop screenshots or PDFs here", cx);
    cx.new(|cx| {
        let sub = cx.subscribe(&zone, |this: &mut Live<Dropzone>, _, e, cx| {
            let names = |p: &[std::path::PathBuf]| {
                p.iter()
                    .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_owned))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            this.caption = match e {
                DropEvent::Accepted(p) => format!("accepted: {}", names(p)).into(),
                DropEvent::Rejected(p) => format!("rejected: {}", names(p)).into(),
                DropEvent::BrowseRequested => "browse requested (open a file picker)".into(),
            };
            cx.notify();
        });
        Live {
            widget: zone,
            label: "Accepts image/* and .pdf",
            caption: "Nothing dropped yet".into(),
            _sub: Some(sub),
        }
    })
    .into()
}
