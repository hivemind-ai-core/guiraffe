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

/// Remembers the knob values last pushed into stateful widgets, so a preview applies
/// setters (which notify) only when a knob actually changed, not on every render.
#[derive(Default)]
pub struct Applied(std::cell::RefCell<Option<KnobValues>>);

impl Applied {
    /// True (and records `v`) when `v` differs from the last applied values.
    pub fn changed(&self, v: &KnobValues) -> bool {
        let mut last = self.0.borrow_mut();
        if last.as_ref() == Some(v) {
            return false;
        }
        *last = Some(v.clone());
        true
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
    preview_first: bool,
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
    playground_entity_with_update(title, knobs, init, |_, _, _| {}, render, window, cx)
}

/// Build a playground whose stateful controls receive props before rendering starts.
fn playground_entity_with_update<S: 'static>(
    title: &'static str,
    knobs: &[Knob],
    init: impl FnOnce(&mut Window, &mut App) -> S,
    apply: fn(&S, &KnobValues, &mut App),
    render: StatefulPreviewFn<S>,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<Playground>, Entity<Preview<S>>) {
    let values = KnobValues(knobs.iter().map(|k| k.default).collect());
    let state = init(window, cx);
    apply(&state, &values, cx);
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
                            apply(&p.state, &p.values, cx);
                            cx.notify();
                        }
                    });
                })
            })
            .collect();
        Playground {
            title,
            preview_first: false,
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

pub fn playground_with_update_preview_first<S: 'static>(
    title: &'static str,
    knobs: &[Knob],
    init: impl FnOnce(&mut Window, &mut App) -> S,
    apply: fn(&S, &KnobValues, &mut App),
    render: StatefulPreviewFn<S>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    let (playground, _) =
        playground_entity_with_update(title, knobs, init, apply, render, window, cx);
    playground.update(cx, |playground, _| playground.preview_first = true);
    playground.into()
}

/// Stateful playground with a synchronous callback for initial and changed knob values.
pub fn playground_with_update<S: 'static>(
    title: &'static str,
    knobs: &[Knob],
    init: impl FnOnce(&mut Window, &mut App) -> S,
    apply: fn(&S, &KnobValues, &mut App),
    render: StatefulPreviewFn<S>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    playground_entity_with_update(title, knobs, init, apply, render, window, cx)
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
        if self.knobs.is_empty() {
            return column([example(self.title, self.preview.clone())]);
        }
        let options = example(
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
        );
        let preview = example(self.title, self.preview.clone());
        if self.preview_first {
            column([preview, options])
        } else {
            column([options, preview])
        }
    }
}

type StaticFn = fn(&mut Window, &mut App) -> AnyElement;

/// A playground above an existing static demo: `live` draws a knob-driven example, `rest`
/// the fixed examples below it.
pub fn with_knobs(
    title: &'static str,
    knobs: &[Knob],
    live: PreviewFn,
    rest: StaticFn,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    playground_with(
        title,
        knobs,
        move |_, _| (live, rest),
        |s: &(PreviewFn, StaticFn), v, window, cx| {
            column([(s.0)(v, window, cx), (s.1)(window, cx)])
        },
        window,
        cx,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Record automatic GPUI draws as well as explicitly stepped frames.
    fn assert_immediate_transition(demo: &str, changes: &[(usize, usize)]) {
        use gpui_kit::{HeadlessAppContext, platform, size};
        use std::{
            cell::{Cell, RefCell},
            rc::Rc,
            sync::Arc,
        };

        let entry = crate::catalog::entries()
            .into_iter()
            .find(|entry| entry.name == demo)
            .expect("catalog demo");
        let mut cx = HeadlessAppContext::with_platform(
            platform::current_platform(true).text_system(),
            Arc::new(guiraffe_icons::GuiraffeAssets),
            platform::current_headless_renderer,
        );
        cx.update(crate::init);
        let frames = Rc::new(RefCell::new(Vec::new()));
        let armed = Rc::new(Cell::new(false));
        let window = cx
            .open_window(size(px(1100.), px(850.)), |window, cx| {
                let playground = (entry.demo)(window, cx)
                    .downcast::<Playground>()
                    .unwrap_or_else(|_| panic!("{demo} is not a playground"));
                let frames = frames.clone();
                let armed = armed.clone();
                window.observe_test_frames(move |window| {
                    if armed.get() {
                        frames
                            .borrow_mut()
                            .push(window.render_to_image().expect("capture frame"));
                    }
                });
                cx.new(|_| FrameFixture(playground))
            })
            .expect("open window");
        let draw = |cx: &mut HeadlessAppContext| {
            cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
                .expect("draw frame");
        };
        for _ in 0..3 {
            draw(&mut cx);
            cx.run_until_parked();
        }
        for &(id, value) in changes {
            let before = cx
                .capture_screenshot(window.into())
                .expect("capture baseline");
            armed.set(true);
            window
                .update(&mut cx, |root, _, cx| {
                    let knob = root.0.read(cx).knob(id).expect("knob").clone();
                    knob.update(cx, |knob, cx| knob.select(value, cx));
                })
                .expect("change knob");

            for _ in 0..2 {
                cx.run_until_parked();
                draw(&mut cx);
            }
            cx.run_until_parked();
            armed.set(false);
            let settled = cx
                .capture_screenshot(window.into())
                .expect("capture settled frame");
            let recorded = frames.take();
            assert!(
                before != settled,
                "{demo} knob {id} must change preview paint"
            );
            assert!(!recorded.is_empty(), "{demo} knob {id} drew no frames");
            for (index, frame) in recorded.iter().enumerate() {
                assert!(
                    *frame == settled,
                    "{demo} knob {id} painted an intermediate frame at index {index}"
                );
            }
        }

        struct FrameFixture(Entity<Playground>);

        impl gpui_kit::Render for FrameFixture {
            fn render(
                &mut self,
                _: &mut Window,
                cx: &mut gpui_kit::Context<Self>,
            ) -> impl IntoElement {
                div().size_full().child(self.0.read(cx).preview.clone())
            }
        }
    }

    #[test]
    fn code_viewer_knobs_paint_complete_frames() {
        assert_immediate_transition("CodeViewer", &[(1, 0), (1, 1), (3, 0), (6, 1)]);
        assert_immediate_transition(
            "Code annotations",
            &[(6, 0), (6, 1), (1, 0), (1, 1), (4, 0), (4, 1), (3, 0)],
        );
    }

    #[test]
    fn tree_knobs_paint_complete_frames() {
        let row_style_changes = &[(2, 1), (2, 0), (3, 1)];
        assert_immediate_transition("Property grid", row_style_changes);
        assert_immediate_transition("Tree list", row_style_changes);
    }

    #[test]
    fn sample_component_knobs_paint_complete_frames() {
        assert_immediate_transition("SegmentedControl", &[(0, 3), (2, 1)]);
        assert_immediate_transition("Flame graph", &[(0, 1), (1, 1)]);
        assert_immediate_transition("Chart grammar", &[(0, 1), (2, 0)]);
    }

    #[test]
    fn values_fall_back_safely() {
        let v = KnobValues(vec![3, 1, 9]);
        assert_eq!(v.size(0), Size::Lg);
        assert!(v.on(1));
        assert_eq!(v.size(2), Size::Xs, "out of range picks the first choice");
        assert_eq!(v.size(7), Size::Xs, "missing knob picks the first choice");
        assert_eq!(v.color(0), Some(ColorName::Orange));
        assert_eq!(KnobValues(vec![0]).color(0), None, "primary");
        assert_eq!(v.color(1), Some(ColorName::Teal));
    }

    #[test]
    fn applied_tracks_changes() {
        let a = Applied::default();
        assert!(a.changed(&KnobValues(vec![1])));
        assert!(!a.changed(&KnobValues(vec![1])));
        assert!(a.changed(&KnobValues(vec![2])));
    }

    #[test]
    fn set_reports_change() {
        let mut v = KnobValues(vec![0]);
        assert!(v.set(0, 2));
        assert!(!v.set(0, 2));
        assert!(!v.set(5, 1));
    }
}
