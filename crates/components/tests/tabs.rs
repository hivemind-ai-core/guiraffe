//! Tabs: click, keyboard and bus give the same events; closing moves the selection
//! predictably; host writes don't echo; goldens in both themes.
use std::{cell::RefCell, rc::Rc};

use gpui_kit::{
    AppContext, Context, Entity, IntoElement, Modifiers, ParentElement, Render, Styled,
    TestAppContext, VisualTestContext, Window, assets::IconName, div, px, size,
    test::TestWindowExt,
};
use guiraffe_components::tabs::{
    SetSelected, SetTabs, TabId, TabItem, Tabs, TabsAction, TabsDoc, TabsEvent, TabsOptions,
    TabsVariant, tab_close_id, tab_element_id, tabs_store,
};
use guiraffe_core::{
    GuiraffeTheme, Scheme, Size, apply,
    protocol::{ActionBus, ControlId, Source},
};
use guiraffe_state::Store;
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

struct Root(Entity<Tabs>);
impl Render for Root {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .p(px(12.))
            .child(self.0.clone())
    }
}

fn doc() -> TabsDoc {
    TabsDoc::new(vec![
        TabItem::new(1, "Mixer").icon(IconName::Settings),
        TabItem::new(2, "Piano roll").closable(),
        TabItem::new(3, "Browser")
            .icon(IconName::Folder)
            .badge("12")
            .closable(),
        TabItem::new(4, "Locked").disabled(true),
        TabItem::new(5, "Notes").closable(),
    ])
}

struct Mounted<'a> {
    tabs: Entity<Tabs>,
    store: Store<TabsDoc>,
    id: ControlId,
    events: Rc<RefCell<Vec<TabsEvent>>>,
    vcx: &'a mut VisualTestContext,
}

fn mount(cx: &mut TestAppContext) -> Mounted<'_> {
    mount_with(cx, TabsOptions::default())
}

fn mount_with(cx: &mut TestAppContext, options: TabsOptions) -> Mounted<'_> {
    cx.update(guiraffe_components::init);
    let events = Rc::new(RefCell::new(Vec::new()));
    let ev = events.clone();
    let mut parts = None;
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let store = tabs_store(doc(), cx);
        let tabs = Tabs::new(store.clone(), options, cx);
        let id = Tabs::register(&tabs, "editor/tabs", window, cx);
        cx.subscribe(&tabs, move |_, _, e: &TabsEvent, _| {
            ev.borrow_mut().push(*e)
        })
        .detach();
        parts = Some((tabs.clone(), store, id));
        Root(tabs)
    });
    vcx.simulate_resize(size(px(800.), px(200.)));
    vcx.run_until_parked();
    let (tabs, store, id) = parts.expect("mounted");
    Mounted {
        tabs,
        store,
        id,
        events,
        vcx,
    }
}

#[gpui_kit::test]
fn click_key_and_bus_select_alike(cx: &mut TestAppContext) {
    // Every size and variant behaves the same: the look never changes the keyboard or the bus.
    for options in [
        TabsOptions::default(),
        TabsOptions::default()
            .size(Size::Xs)
            .variant(TabsVariant::Pill),
        TabsOptions::default().size(Size::Md),
    ] {
        let click = {
            let m = mount_with(cx, options.clone());
            m.vcx.update(|w, cx| w.click(tab_element_id(TabId(3)), cx));
            m.vcx.run_until_parked();
            m.events.take()
        };
        let key = {
            let m = mount_with(cx, options.clone());
            m.vcx.update(|window, cx| {
                let handle = gpui_kit::Focusable::focus_handle(m.tabs.read(cx), cx);
                handle.focus(window, cx);
            });
            m.vcx.simulate_keystrokes("right right enter");
            m.events.take()
        };
        let bus = {
            let m = mount_with(cx, options.clone());
            m.vcx.update(|window, cx| {
                ActionBus::dispatch(
                    m.id,
                    &TabsAction::Select(TabId(3)).to_action(),
                    Source::Script,
                    window,
                    cx,
                )
                .ok();
            });
            m.events.take()
        };
        assert_eq!(click, [TabsEvent::Selected(TabId(3))]);
        assert_eq!(key, click, "arrows move focus, Enter selects");
        assert_eq!(bus, click);
    }
}

#[gpui_kit::test]
fn size_and_variant_setters_restyle_without_events_or_redundant_notifies(cx: &mut TestAppContext) {
    let m = mount(cx);
    let notified = Rc::new(std::cell::Cell::new(0));
    let count = notified.clone();
    let tabs = m.tabs.clone();
    m.vcx.update(|_, cx| {
        cx.observe(&tabs, move |_, _| count.set(count.get() + 1))
            .detach();
    });
    let height = |vcx: &mut VisualTestContext| {
        vcx.update(|w, cx| {
            w.render_frame(cx);
            f32::from(w.find(tab_element_id(TabId(1))).bounds().size.height)
        })
    };
    let sm = height(m.vcx);
    assert_eq!(sm, 32.);
    m.vcx.update(|_, cx| {
        tabs.update(cx, |t, cx| {
            t.set_size(Size::Sm, cx);
            t.set_variant(TabsVariant::Underline, cx);
        })
    });
    assert_eq!(notified.get(), 0, "unchanged look: no notify");
    m.vcx.update(|_, cx| {
        tabs.update(cx, |t, cx| {
            t.set_size(Size::Xs, cx);
            t.set_variant(TabsVariant::Pill, cx);
        })
    });
    assert_eq!(
        notified.get(),
        1,
        "both changes in one update: one observer call"
    );
    assert_eq!(
        m.vcx
            .update(|_, cx| (tabs.read(cx).size(), tabs.read(cx).variant())),
        (Size::Xs, TabsVariant::Pill)
    );
    m.vcx.run_until_parked();
    // Xs row (26 px) minus the pill track's inset on both sides.
    assert!(height(m.vcx) < sm);
    assert!(m.events.borrow().is_empty(), "host writes never echo");
}

#[gpui_kit::test]
fn closing_moves_the_selection_predictably(cx: &mut TestAppContext) {
    let m = mount(cx);
    m.vcx
        .update(|_, cx| m.store.dispatch(SetSelected(TabId(3)), cx));
    m.vcx.run_until_parked();
    m.vcx.update(|w, cx| w.click(tab_close_id(TabId(3)), cx));
    m.vcx.run_until_parked();
    let selected = m.vcx.update(|_, cx| m.tabs.read(cx).selected(cx));
    assert_eq!(
        selected,
        Some(TabId(5)),
        "the next enabled tab (4 is disabled)"
    );
    assert_eq!(
        *m.events.borrow(),
        [TabsEvent::Closed {
            id: TabId(3),
            selected: Some(TabId(5))
        }],
        "the close button doesn't also select its tab"
    );
    // Ctrl+W closes the focused closable tab.
    m.vcx.update(|window, cx| {
        let handle = gpui_kit::Focusable::focus_handle(m.tabs.read(cx), cx);
        handle.focus(window, cx);
    });
    m.vcx.simulate_keystrokes("ctrl-w");
    assert_eq!(
        m.vcx.update(|_, cx| m.tabs.read(cx).selected(cx)),
        Some(TabId(2))
    );
    assert_eq!(
        m.vcx.update(|_, cx| m.tabs.read(cx).doc(cx).tabs().len()),
        3
    );
}

#[gpui_kit::test]
fn host_writes_never_echo(cx: &mut TestAppContext) {
    let m = mount(cx);
    m.vcx.update(|_, cx| {
        m.store.dispatch(SetSelected(TabId(2)), cx);
        let mut tabs = m.store.state(cx).tabs().to_vec();
        tabs.reverse();
        tabs.push(TabItem::new(9, "New"));
        m.store.dispatch(SetTabs(tabs), cx);
    });
    m.vcx.run_until_parked();
    assert_eq!(
        m.vcx.update(|_, cx| m.tabs.read(cx).selected(cx)),
        Some(TabId(2))
    );
    assert!(m.events.borrow().is_empty());
    let _ = Modifiers::default();
}

#[test]
fn tabs_light_and_dark() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let mut s = Snapshotter::new()
            .unwrap_or_else(|e| panic!("{e}"))
            .with_setup(guiraffe_components::init);
        let img = s
            .render(size(px(560.), px(80.)), move |window, cx| {
                apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
                let store = tabs_store(doc(), cx);
                store.dispatch_now(SetSelected(TabId(2)), cx);
                let tabs = Tabs::new(store, TabsOptions::default(), cx);
                // Keyboard focus on the Browser tab, so the golden shows the focus ring.
                tabs.update(cx, |t, cx| t.focus_for_snapshot(TabId(3), window, cx));
                cx.new(|_| Root(tabs))
            })
            .unwrap_or_else(|e| panic!("{e:#}"));
        let name = if scheme == Scheme::Light {
            "tabs-light"
        } else {
            "tabs-dark"
        };
        assert_snapshot(name, &img, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
    }
}

/// Every size (Xs, Sm, Md) in both variants, stacked, in both themes.
struct Gallery(Vec<Entity<Tabs>>);
impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .p(px(12.))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(10.))
            .children(self.0.iter().cloned())
    }
}

#[test]
fn tab_sizes_and_variants_light_and_dark() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let mut s = Snapshotter::new()
            .unwrap_or_else(|e| panic!("{e}"))
            .with_setup(guiraffe_components::init);
        let img = s
            .render(size(px(640.), px(300.)), move |_, cx| {
                apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
                let mut rows = Vec::new();
                for variant in [TabsVariant::Underline, TabsVariant::Pill] {
                    for size in [Size::Xs, Size::Sm, Size::Md] {
                        let store = tabs_store(doc(), cx);
                        store.dispatch_now(SetSelected(TabId(2)), cx);
                        let options = TabsOptions::default().size(size).variant(variant);
                        rows.push(Tabs::new(store, options, cx));
                    }
                }
                cx.new(|_| Gallery(rows))
            })
            .unwrap_or_else(|e| panic!("{e:#}"));
        let name = if scheme == Scheme::Light {
            "tabs-variants-light"
        } else {
            "tabs-variants-dark"
        };
        assert_snapshot(name, &img, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
    }
}
