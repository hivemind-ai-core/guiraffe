//! Live option knobs for demos: a row of segmented controls above a preview.
//!
//! [`playground`] builds two entities. The outer one holds the knob controls. The inner one
//! renders the preview from the current [`KnobValues`]. Turning a knob notifies only the inner
//! preview entity, so neither the shell nor the knob row re-renders.

use gpui_kit::{
    AnyElement, AnyView, App, AppContext, Context, Entity, IntoElement, ParentElement, Render,
    Styled, Subscription, Window, div, px,
};
use guiraffe_components::{SegmentedControl, SegmentedEvent, Text};
use guiraffe_core::{ColorName, Size, Variant};

use crate::demos::{column, example};

/// One knob: a label and its choices, with the initially selected index.
#[derive(Clone, Copy)]
pub struct Knob {
    pub label: &'static str,
    pub options: &'static [&'static str],
    pub default: usize,
}

impl Knob {
    pub const fn new(label: &'static str, options: &'static [&'static str]) -> Self {
        Self {
            label,
            options,
            default: 0,
        }
    }

    pub const fn default(mut self, index: usize) -> Self {
        self.default = index;
        self
    }

    /// The standard size scale, defaulting to `md`.
    pub const fn size() -> Self {
        Self::new("Size", SIZE_NAMES).default(2)
    }

    pub const fn radius() -> Self {
        Self::new("Radius", SIZE_NAMES).default(1)
    }

    pub const fn color() -> Self {
        Self::new("Color", COLOR_NAMES)
    }

    pub const fn variant() -> Self {
        Self::new("Variant", VARIANT_NAMES)
    }

    pub const fn toggle(label: &'static str) -> Self {
        Self::new(label, &["off", "on"])
    }

    pub const fn gap() -> Self {
        Self::new("Gap", SIZE_NAMES).default(2)
    }
}

pub const SIZE_NAMES: &[&str] = &["xs", "sm", "md", "lg", "xl"];
pub const SIZES: [Size; 5] = [Size::Xs, Size::Sm, Size::Md, Size::Lg, Size::Xl];
pub const COLOR_NAMES: &[&str] = &["primary", "teal", "grape", "orange", "red", "gray"];
pub const COLORS: [Option<ColorName>; 6] = [
    None,
    Some(ColorName::Teal),
    Some(ColorName::Grape),
    Some(ColorName::Orange),
    Some(ColorName::Red),
    Some(ColorName::Gray),
];
pub const VARIANT_NAMES: &[&str] = &["filled", "light", "outline", "subtle", "default"];
pub const VARIANTS: [Variant; 5] = [
    Variant::Filled,
    Variant::Light,
    Variant::Outline,
    Variant::Subtle,
    Variant::Default,
];

/// The selected index of every knob, in declaration order. Out-of-range lookups fall back to
/// the first choice, so a preview never panics on a mismatched knob list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnobValues(pub Vec<usize>);

impl KnobValues {
    pub fn index(&self, knob: usize) -> usize {
        self.0.get(knob).copied().unwrap_or(0)
    }

    pub fn pick<T: Copy>(&self, knob: usize, table: &[T]) -> Option<T> {
        table
            .get(self.index(knob))
            .or_else(|| table.first())
            .copied()
    }

    pub fn on(&self, knob: usize) -> bool {
        self.index(knob) == 1
    }

    pub fn size(&self, knob: usize) -> Size {
        self.pick(knob, &SIZES).unwrap_or_default()
    }

    pub fn color(&self, knob: usize) -> Option<ColorName> {
        self.pick(knob, &COLORS).flatten()
    }

    pub fn variant(&self, knob: usize) -> Variant {
        self.pick(knob, &VARIANTS).unwrap_or_default()
    }

    /// Set `knob` to `index`. Returns whether anything changed.
    pub fn set(&mut self, knob: usize, index: usize) -> bool {
        match self.0.get_mut(knob) {
            Some(v) if *v != index => {
                *v = index;
                true
            }
            _ => false,
        }
    }
}

pub type PreviewFn = fn(&KnobValues, &mut Window, &mut App) -> AnyElement;

/// Renders a stateful preview. `cx` is the preview's own context, so event handlers can
/// capture `cx.entity()` and mutate [`Preview::state`].
pub type StatefulPreviewFn<S> =
    fn(&S, &KnobValues, &mut Window, &mut Context<Preview<S>>) -> AnyElement;

/// The preview: re-renders from its values and state. Only this entity is notified on a
/// knob change.
pub struct Preview<S: 'static> {
    pub state: S,
    values: KnobValues,
    render: StatefulPreviewFn<S>,
}

impl<S: 'static> Preview<S> {
    pub fn values(&self) -> &KnobValues {
        &self.values
    }
}

impl<S: 'static> Render for Preview<S> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        (self.render)(&self.state, &self.values, window, cx)
    }
}

pub struct Playground {
    title: &'static str,
    knobs: Vec<(&'static str, Entity<SegmentedControl>)>,
    preview: AnyView,
    _subs: Vec<Subscription>,
}

impl Playground {
    pub fn knob(&self, i: usize) -> Option<&Entity<SegmentedControl>> {
        self.knobs.get(i).map(|k| &k.1)
    }
}

/// A playground with preview state `S`, built once by `init`.
pub fn playground_entity<S: 'static>(
    title: &'static str,
    knobs: &[Knob],
    init: impl FnOnce(&mut Window, &mut App) -> S,
    render: StatefulPreviewFn<S>,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<Playground>, Entity<Preview<S>>) {
    let values = KnobValues(knobs.iter().map(|k| k.default).collect());
    let state = init(window, cx);
    let preview = cx.new(|_| Preview {
        state,
        values,
        render,
    });
    let controls: Vec<_> = knobs
        .iter()
        .map(|k| {
            let c = SegmentedControl::new(k.options.iter().copied(), cx);
            c.update(cx, |c, cx| {
                c.set_size(Some(Size::Xs), cx);
                c.select(k.default, cx);
            });
            (k.label, c)
        })
        .collect();
    let p = preview.clone();
    let pg = cx.new(|cx| {
        let subs = controls
            .iter()
            .enumerate()
            .map(|(i, (_, c))| {
                let p = p.clone();
                cx.subscribe(c, move |_, _, e: &SegmentedEvent, cx| {
                    let SegmentedEvent::Changed(index) = e;
                    p.update(cx, |p, cx| {
                        if p.values.set(i, *index) {
                            cx.notify();
                        }
                    });
                })
            })
            .collect();
        Playground {
            title,
            knobs: controls,
            preview: p.into(),
            _subs: subs,
        }
    });
    (pg, preview)
}

/// [`playground_entity`] as an [`AnyView`], for catalog demos.
pub fn playground_with<S: 'static>(
    title: &'static str,
    knobs: &[Knob],
    init: impl FnOnce(&mut Window, &mut App) -> S,
    render: StatefulPreviewFn<S>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    playground_entity(title, knobs, init, render, window, cx)
        .0
        .into()
}

/// A stateless playground: `render` draws the preview from the knob values alone.
pub fn playground(
    title: &'static str,
    knobs: &[Knob],
    render: PreviewFn,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    playground_with(
        title,
        knobs,
        move |_, _| render,
        |f: &PreviewFn, v, window, cx| f(v, window, cx),
        window,
        cx,
    )
}

impl Render for Playground {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        column([
            example(
                "Options",
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x(px(20.))
                    .gap_y(px(10.))
                    .children(self.knobs.iter().map(|(label, c)| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(Text::new(*label).size(Size::Xs).dimmed(true))
                            .child(c.clone())
                    })),
            ),
            example(self.title, self.preview.clone()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_fall_back_safely() {
        let v = KnobValues(vec![3, 1, 9]);
        assert_eq!(v.size(0), Size::Lg);
        assert!(v.on(1));
        assert_eq!(v.size(2), Size::Xs, "out of range picks the first choice");
        assert_eq!(v.size(7), Size::Xs, "missing knob picks the first choice");
        assert_eq!(v.color(0), None);
        assert_eq!(v.color(1), Some(ColorName::Teal));
    }

    #[test]
    fn set_reports_change() {
        let mut v = KnobValues(vec![0]);
        assert!(v.set(0, 2));
        assert!(!v.set(0, 2));
        assert!(!v.set(5, 1));
    }
}
