//! Zero cost when closed: a window with a closed ribbon repaints with exactly the allocations
//! of the same window without the ribbon (the leader is a once-per-window keystroke
//! interceptor, not a per-frame listener). An open, unchanged ribbon
//! stays within budget and does not grow.
use gpui_kit::{
    AppContext, Bounds, Context, Entity, IntoElement, ParentElement, Render, Styled,
    TestAppContext, Window, WindowBounds, WindowHandle, WindowOptions, base::Root, div, point, px,
    size,
};
use guiraffe_commands::{
    Command, CommandId, Commands, Payload, init,
    ribbon::{
        CommandRibbon, RibbonButton, RibbonCategory, RibbonEntry, RibbonLayout, RibbonMenu,
        RibbonModel, RibbonState, ribbon_scope,
    },
};
use guiraffe_test::alloc::{CountingAllocator, count_allocations};
use std::{collections::BTreeMap, rc::Rc};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// 3 categories x 6 rows open (18 Kit buttons); measured 2 120 upstream on 2026-11-25.
const OPEN_BUDGET: u64 = 2_500;

fn noop(_: &Payload, _: &mut Window, _: &mut gpui_kit::App) {}

struct Host {
    ribbon: Option<Entity<RibbonState>>,
    button: bool,
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = div().flex_1().child("content");
        match &self.ribbon {
            // The status-bar button is ordinary visible UI, so neither window has one: the
            // comparison isolates the scope and the closed bar.
            Some(ribbon) if self.button => ribbon_scope(ribbon)
                .size_full()
                .flex()
                .flex_col()
                .child(content)
                .children(CommandRibbon::when_open(ribbon, cx))
                .child(div().child(RibbonButton::new(ribbon)))
                .into_any_element(),
            Some(ribbon) => ribbon_scope(ribbon)
                .size_full()
                .flex()
                .flex_col()
                .child(content)
                .children(CommandRibbon::when_open(ribbon, cx))
                .into_any_element(),
            // Baseline: the same window without the ribbon.
            None => div()
                .size_full()
                .flex()
                .flex_col()
                .child(content)
                .into_any_element(),
        }
    }
}

fn mount(
    cx: &mut TestAppContext,
    with_ribbon: bool,
    with_button: bool,
) -> (WindowHandle<Root>, Option<Entity<RibbonState>>) {
    cx.update(init);
    cx.update(|cx| {
        let commands = Commands::new(cx);
        let mut root = RibbonMenu::new("root", "Commands");
        for (c, name) in ["File", "Edit", "View"].into_iter().enumerate() {
            let mut category = RibbonCategory::new(name);
            for step in 0..6_u8 {
                let id = CommandId::parse(&format!("{}.action{step}", name.to_lowercase()))
                    .unwrap_or_else(|e| panic!("{e}"));
                commands
                    .register(Command::new(id.clone(), format!("{name} {step}"), noop), cx)
                    .unwrap_or_else(|e| panic!("{e}"));
                let key = char::from(b'a' + c as u8 * 6 + step).to_string();
                category = category.entry(RibbonEntry::command(format!("{name}{step}"), key, id));
            }
            root = root.category(category);
        }
        let model = {
            let state = commands.entity().read(cx);
            Rc::new(
                RibbonModel::build(
                    &RibbonLayout::new("root").menu(root),
                    state.catalog(),
                    &BTreeMap::new(),
                )
                .unwrap_or_else(|e| panic!("{e}")),
            )
        };
        let slot = Rc::new(std::cell::RefCell::new(None));
        let out = slot.clone();
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(1200.), px(500.)),
                    ))),
                    ..Default::default()
                },
                move |window, cx| {
                    let focus = cx.focus_handle();
                    focus.focus(window, cx);
                    let ribbon = with_ribbon
                        .then(|| cx.new(|cx| RibbonState::new(&commands, model, window, cx)));
                    *out.borrow_mut() = ribbon.clone();
                    let host = cx.new(|_| Host {
                        button: ribbon.is_some() && with_button,
                        ribbon,
                    });
                    guiraffe_overlays::root(host, window, cx)
                },
            )
            .expect("window");
        let ribbon = slot.borrow_mut().take();
        (window, ribbon)
    })
}

fn repaint(cx: &mut TestAppContext, window: WindowHandle<Root>) -> u64 {
    let (_, counts) = count_allocations(|| {
        cx.update_window(window.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .expect("frame");
        cx.run_until_parked();
    });
    counts.allocations
}

fn series(cx: &mut TestAppContext, window: WindowHandle<Root>) -> Vec<u64> {
    cx.run_until_parked();
    for _ in 0..3 {
        repaint(cx, window);
    }
    (0..6).map(|_| repaint(cx, window)).collect()
}

#[gpui_kit::test]
fn a_closed_ribbon_costs_nothing_per_frame(cx: &mut TestAppContext) {
    let (baseline, _) = mount(cx, false, false);
    let (closed, _) = mount(cx, true, false);
    let base = series(cx, baseline);
    let with = series(cx, closed);
    eprintln!("ribbon closed repaint: baseline {base:?}, with ribbon {with:?}");
    let base_min = base.iter().copied().min().unwrap_or(0);
    let with_min = with.iter().copied().min().unwrap_or(0);
    assert!(
        with == base && with_min == base_min,
        "a closed ribbon allocates per frame: {with:?} vs placeholder {base:?}"
    );
}

#[gpui_kit::test]
fn an_open_unchanged_ribbon_stays_within_budget_and_does_not_grow(cx: &mut TestAppContext) {
    let (window, ribbon) = mount(cx, true, true);
    let ribbon = ribbon.expect("ribbon");
    cx.update_window(window.into(), |_, window, cx| {
        ribbon.update(cx, |state, cx| state.open(window, cx));
    })
    .expect("open");
    let values = series(cx, window);
    eprintln!("ribbon open repaint series: {values:?}");
    let peak = values.iter().copied().max().unwrap_or(0);
    assert!(
        peak <= OPEN_BUDGET,
        "open ribbon repaint peaks at {peak}: {values:?}"
    );
    let mut early = values[..3].to_vec();
    let mut late = values[3..].to_vec();
    early.sort_unstable();
    late.sort_unstable();
    assert!(late[1] <= early[1], "open ribbon repaint grows: {values:?}");
}
