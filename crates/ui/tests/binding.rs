//! `ui!` store binding: one-way and two-way bindings, render isolation, coalescing, undo,
//! stateful components on the action bus, and virtualized store-bound lists.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
};

use gpui_kit::{
    App, Context, IntoElement, ParentElement, Render, RenderOnce, SharedString, Styled,
    TestAppContext, VisualTestContext, Window, div, px,
};
use guiraffe_core::protocol::{Action, ActionBus, ControlId, Source, names};
use guiraffe_core::widget_value::Value;
use guiraffe_state::{
    EventRecord, Interceptor, KeyedList, Lens as _, ListLens, Store, UndoEvent, store,
};
use guiraffe_ui::{component, ui};

type Callback = Rc<dyn Fn(&u32, &mut Window, &mut App)>;

thread_local! {
    static CALLBACKS: RefCell<HashMap<&'static str, Callback>> = RefCell::default();
    static RENDERS: RefCell<HashMap<&'static str, usize>> = RefCell::default();
    static LABELS: RefCell<Vec<String>> = RefCell::default();
}

fn renders(id: &'static str) -> usize {
    RENDERS.with(|r| r.borrow().get(id).copied().unwrap_or_default())
}

/// The user's gesture: call the change callback the control last rendered with.
fn change(cx: &mut VisualTestContext, id: &'static str, value: u32) {
    let callback = CALLBACKS.with(|c| c.borrow().get(id).cloned());
    let callback = callback.expect("control rendered with a change callback");
    cx.update(|window, cx| callback(&value, window, cx));
    cx.run_until_parked();
}

/// A minimal controlled input: `value` plus `on_change`, counting its renders.
#[derive(IntoElement)]
struct Knob {
    id: &'static str,
    value: u32,
    label: SharedString,
}

impl Knob {
    fn new(id: &'static str) -> Self {
        Self {
            id,
            value: 0,
            label: SharedString::default(),
        }
    }
    fn value(mut self, value: u32) -> Self {
        self.value = value;
        self
    }
    fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }
    fn on_change(self, f: impl Fn(&u32, &mut Window, &mut App) + 'static) -> Self {
        let id = self.id;
        CALLBACKS.with(|c| c.borrow_mut().insert(id, Rc::new(f)));
        self
    }
}

impl RenderOnce for Knob {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        RENDERS.with(|r| *r.borrow_mut().entry(self.id).or_default() += 1);
        let text = format!("{} {}", self.label, self.value);
        LABELS.with(|l| l.borrow_mut().push(text.clone()));
        div().child(text)
    }
}

store! {
    pub Mixer {
        gain: u32 = 10 where 0..=100,
        pan: u32 = 50,
        title: SharedString = SharedString::new_static("Mix"),
        muted: bool = false,
    }
}

struct Page {
    store: Store<Mixer>,
    renders: Rc<Cell<usize>>,
    suffix: &'static str,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let store = self.store.clone();
        let suffix = self.suffix;
        ui! {
            div {
                store(store) {
                    Knob("gain") { value <=> gain  label: suffix }
                    Knob("pan") { value <= pan  label: "Pan" }
                    div { {<= title} }
                    if <= muted { "Muted" } else { "Live" }
                }
            }
        }
    }
}

fn page(cx: &mut TestAppContext) -> (Store<Mixer>, Rc<Cell<usize>>, &mut VisualTestContext) {
    let store = cx.update(Mixer::store);
    let renders = Rc::new(Cell::new(0));
    let (_, vcx) = cx.add_window_view({
        let store = store.clone();
        let renders = renders.clone();
        move |_, _| Page {
            store,
            renders,
            suffix: "",
        }
    });
    vcx.run_until_parked();
    (store, renders, vcx)
}

#[gpui_kit::test]
fn two_way_binding_round_trips_and_isolates_renders(cx: &mut TestAppContext) {
    let (store, page_renders, vcx) = page(cx);
    // gpui-fast rebuilds the parent once on the first edit after mounting (KNOWN_ISSUES.md).
    change(vcx, "gain", 11);
    let (page, gain, pan) = (page_renders.get(), renders("gain"), renders("pan"));
    assert!(gain > 0 && pan > 0);

    // A gesture writes the store through the generated Set event.
    change(vcx, "gain", 42);
    vcx.update(|_, cx| assert_eq!(store.state(cx).gain, 42));
    // Upstream GPUI re-renders the ancestors of a notified view (architecture.md), so render
    // isolation is a gpui-fast property; the Store's work is isolated on both.
    if gpui_kit::GPUI_FAST {
        assert_eq!(renders("gain"), gain + 1, "the bound control re-rendered");
        assert_eq!(
            renders("pan"),
            pan,
            "a control bound to another field did not"
        );
        assert_eq!(page_renders.get(), page, "the parent did not re-render");
    } else {
        assert!(renders("gain") > gain);
    }

    // Validation applies to bound edits.
    change(vcx, "gain", 500);
    vcx.update(|_, cx| assert_eq!(store.state(cx).gain, 100));

    // A host write updates the control and never comes back as an edit.
    vcx.update(|_, cx| store.set(Mixer::pan, 7, cx));
    vcx.run_until_parked();
    vcx.update(|_, cx| assert_eq!(store.state(cx).gain, 100, "no echo"));
    if gpui_kit::GPUI_FAST {
        assert_eq!(renders("pan"), pan + 1);
        assert_eq!(page_renders.get(), page);
    }

    // A parent re-render passes new unbound props to the bound control.
    LABELS.with(|l| l.borrow_mut().clear());
    vcx.update(|window, cx| {
        if let Some(Some(page)) = window.root::<Page>() {
            page.update(cx, |page, cx| {
                page.suffix = "dB";
                cx.notify();
            });
        }
    });
    vcx.run_until_parked();
    assert!(
        LABELS.with(|l| l.borrow().iter().any(|label| label == "dB 100")),
        "the bound control rendered the parent's new label"
    );
}

#[gpui_kit::test]
fn notifications_coalesce_to_one_render_per_frame(cx: &mut TestAppContext) {
    let (store, _, vcx) = page(cx);
    change(vcx, "gain", 11);
    let gain = renders("gain");
    vcx.update(|_, cx| {
        for value in [1, 2, 3, 4] {
            store.set(Mixer::gain, value, cx);
        }
    });
    vcx.run_until_parked();
    assert_eq!(
        renders("gain"),
        gain + 1,
        "four writes, one frame, one render"
    );
    vcx.update(|_, cx| assert_eq!(store.state(cx).gain, 4));
}

#[gpui_kit::test]
fn undo_restores_bound_controls(cx: &mut TestAppContext) {
    let (store, _, vcx) = page(cx);
    vcx.update(|_, cx| store.enable_undo(8, cx));
    change(vcx, "gain", 30);
    change(vcx, "gain", 60);
    let gain = renders("gain");
    vcx.update(|_, cx| store.dispatch_now(UndoEvent, cx));
    vcx.run_until_parked();
    vcx.update(|_, cx| assert_eq!(store.state(cx).gain, 30));
    assert_eq!(renders("gain"), gain + 1);
}

// ---- Stateful components ---------------------------------------------------------------

#[component(stateful)]
fn Counter(#[state] count: u32, step: u32) -> impl IntoElement {
    ui! {
        div {
            Knob("counter") {
                value: count
                on_change => count += step
            }
        }
    }
}

store! {
    pub Clicks {
        clicks: u32 = 0,
    }
}

struct Counters {
    store: Store<Clicks>,
}

impl Render for Counters {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.clone();
        ui! {
            div {
                Counter("local", 2) {}
                store(store) {
                    Counter("bound", 5) { count <=> clicks }
                }
            }
        }
    }
}

#[derive(Default)]
struct Sources(Rc<RefCell<Vec<Source>>>);

impl Interceptor<Clicks> for Sources {
    fn after(&mut self, record: &EventRecord, _: &Arc<Clicks>, _: &Arc<Clicks>) {
        self.0.borrow_mut().push(record.source);
    }
}

#[gpui_kit::test]
fn stateful_components_are_observed_and_driven_over_the_bus(cx: &mut TestAppContext) {
    let sources = Rc::new(RefCell::new(Vec::new()));
    let store = cx.update(|cx| {
        let store = Clicks::store(cx);
        store.add_interceptor(Sources(sources.clone()), cx);
        store
    });
    let (_, vcx) = cx.add_window_view({
        let store = store.clone();
        move |_, _| Counters { store }
    });
    vcx.run_until_parked();
    let set = |n: f64| Action {
        name: names::SET_VALUE.into(),
        payload: Value::Num(n),
    };
    vcx.update(|window, cx| {
        let local = ControlId::lookup("local", cx).expect("registered at its id");
        assert_eq!(
            ActionBus::query(local, window, cx).map(|s| s.value),
            Ok(Some(Value::Num(0.0)))
        );
        assert!(ActionBus::dispatch(local, &set(9.0), Source::Mcp, window, cx).is_ok());
        assert_eq!(
            ActionBus::query(local, window, cx).map(|s| s.value),
            Ok(Some(Value::Num(9.0)))
        );
        let invalid = Action {
            name: names::SET_VALUE.into(),
            payload: Value::Str("nine".into()),
        };
        assert!(ActionBus::dispatch(local, &invalid, Source::Mcp, window, cx).is_err());

        // Bound state goes to the store, tagged with the bus source.
        let bound = ControlId::lookup("bound", cx).expect("registered");
        assert!(ActionBus::dispatch(bound, &set(3.0), Source::Shortcut, window, cx).is_ok());
    });
    vcx.run_until_parked();
    vcx.update(|_, cx| assert_eq!(store.state(cx).clicks, 3));
    assert_eq!(*sources.borrow(), [Source::Shortcut]);
    vcx.update(|window, cx| {
        let bound = ControlId::lookup("bound", cx).expect("registered");
        assert_eq!(
            ActionBus::query(bound, window, cx).map(|s| s.value),
            Ok(Some(Value::Num(3.0))),
            "the snapshot mirrors the store"
        );
    });
}

// ---- Store-bound lists -----------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, guiraffe_state::Lens)]
pub struct Step {
    pub velocity: u32,
}

store! {
    pub Pattern {
        steps: KeyedList<u32, Step> = KeyedList::new(),
    }
}

thread_local! {
    static ROW_RENDERS: RefCell<HashMap<u32, usize>> = RefCell::default();
}

struct Sequencer {
    store: Store<Pattern>,
    renders: Rc<Cell<usize>>,
}

impl Render for Sequencer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        let store = self.store.clone();
        ui! {
            div {
                size_full
                store(store) {
                    for step in <= steps, row_height: px(20.) {
                        div {
                            h: px(20.)
                            {
                                ROW_RENDERS.with(|r| *r.borrow_mut().entry(step.velocity).or_default() += 1);
                                format!("{}", step.velocity)
                            }
                        }
                    }
                }
            }
        }
    }
}

fn row_renders() -> usize {
    ROW_RENDERS.with(|r| r.borrow().values().sum())
}

#[gpui_kit::test]
fn bound_lists_virtualize_and_edit_one_row(cx: &mut TestAppContext) {
    let n = 10_000u32;
    let store = cx.update(|cx| {
        Store::new(
            Pattern {
                steps: (0..n).map(|k| (k, Step { velocity: k })).collect(),
            },
            cx,
        )
    });
    let renders = Rc::new(Cell::new(0));
    let (_, vcx) = cx.add_window_view({
        let store = store.clone();
        let renders = renders.clone();
        move |_, _| Sequencer { store, renders }
    });
    vcx.simulate_resize(gpui_kit::size(px(200.), px(400.)));
    vcx.run_until_parked();
    let mounted = ROW_RENDERS.with(|r| r.borrow().len());
    assert!(
        (1..=100).contains(&mounted),
        "only the viewport mounts: {mounted} of {n} rows rendered"
    );
    // gpui-fast rebuilds the parent once on the first edit after mounting (KNOWN_ISSUES.md).
    vcx.update(|_, cx| store.set(Pattern::steps.by_key(4).then(Step::velocity), 4, cx));
    vcx.run_until_parked();
    let page = renders.get();

    // An offscreen row edit renders nothing.
    let before = row_renders();
    vcx.update(|_, cx| store.set(Pattern::steps.by_key(n - 1).then(Step::velocity), 1, cx));
    vcx.run_until_parked();
    assert_eq!(row_renders(), before);

    // A visible row edit renders that row alone on gpui-fast.
    vcx.update(|_, cx| store.set(Pattern::steps.by_key(3).then(Step::velocity), 77_777, cx));
    vcx.run_until_parked();
    vcx.update(|_, cx| {
        assert_eq!(
            store.state(cx).steps.get(&3),
            Some(&Step { velocity: 77_777 })
        );
    });
    if gpui_kit::GPUI_FAST {
        assert_eq!(row_renders(), before + 1);
        assert_eq!(renders.get(), page, "the page never re-renders");
    }
    assert!(
        ROW_RENDERS.with(|r| r.borrow().contains_key(&77_777)),
        "the edit drew"
    );

    // An insert at the top shifts the viewport; existing rows keep their views.
    vcx.update(|_, cx| {
        store.dispatch_now(InsertStep(n + 1), cx);
    });
    vcx.run_until_parked();
    vcx.update(|_, cx| assert_eq!(store.state(cx).steps.key_at(0), Some(&(n + 1))));
}

#[derive(Debug)]
struct InsertStep(u32);

impl guiraffe_state::Event<Pattern> for InsertStep {
    fn handle(self, fx: &mut guiraffe_state::Fx<'_, Pattern>) {
        fx.list_insert(Pattern::steps, 0, self.0, Step { velocity: 0 });
    }
}

// ---- Protocol control entities ---------------------------------------------------------

store! {
    pub Prefs {
        dark: bool = false,
        volume: f32 = 0.5,
    }
}

struct PrefsPage {
    store: Store<Prefs>,
}

impl Render for PrefsPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.clone();
        ui! {
            div {
                store(store) {
                    guiraffe_components::controls::SwitchControl {
                        checked <=> dark
                        .path("prefs/dark")
                        .label("Dark")
                    }
                    guiraffe_components::controls::SliderControl {
                        value <=> volume
                        .path("prefs/volume")
                        .range(0., 1.)
                    }
                }
            }
        }
    }
}

#[gpui_kit::test]
fn control_entities_bind_through_their_events_and_host_setters(cx: &mut TestAppContext) {
    let sources = Rc::new(RefCell::new(Vec::new()));
    let store = cx.update(|cx| {
        let store = Prefs::store(cx);
        store.add_interceptor(PrefSources(sources.clone()), cx);
        store
    });
    let (_, vcx) = cx.add_window_view({
        let store = store.clone();
        move |_, _| PrefsPage { store }
    });
    vcx.run_until_parked();
    let toggle = Action {
        name: names::TOGGLE.into(),
        payload: Value::Null,
    };
    vcx.update(|window, cx| {
        let dark = ControlId::lookup("prefs/dark", cx).expect("registered by .path");
        assert!(ActionBus::dispatch(dark, &toggle, Source::Mcp, window, cx).is_ok());
    });
    vcx.run_until_parked();
    vcx.update(|_, cx| assert!(store.state(cx).dark, "the bus edit reached the store"));
    assert_eq!(*sources.borrow(), [Source::Mcp]);

    // A host write reaches the control without echoing back as an edit.
    vcx.update(|_, cx| store.set(Prefs::volume, 0.25, cx));
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        let volume = ControlId::lookup("prefs/volume", cx).expect("registered");
        assert_eq!(
            ActionBus::query(volume, window, cx).map(|s| s.value),
            Ok(Some(Value::Num(0.25)))
        );
    });
    assert_eq!(sources.borrow().len(), 2, "one host write, no echo");
}

struct PrefSources(Rc<RefCell<Vec<Source>>>);

impl Interceptor<Prefs> for PrefSources {
    fn after(&mut self, record: &EventRecord, _: &Arc<Prefs>, _: &Arc<Prefs>) {
        self.0.borrow_mut().push(record.source);
    }
}
