//! Near-zero cost when closed: a window with a closed ribbon repaints with the allocations of
//! the same window with an empty placeholder plus the leader's two scope listeners
//! (`CLOSED_OVERHEAD`, constant: no rows, no projection, no growth; see KNOWN_ISSUES.md). An open, unchanged ribbon
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
        RibbonModel, RibbonScope, RibbonState,
    },
};
use guiraffe_test::alloc::{CountingAllocator, count_allocations};
use std::{collections::BTreeMap, rc::Rc};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// 3 categories x 6 rows open (18 Kit buttons); measured 2 120 upstream on 2026-11-25.
const OPEN_BUDGET: u64 = 2_500;
/// The leader action and key-up listeners (boxed closures and their listener lists).
const CLOSED_OVERHEAD: u64 = 5;

fn noop(_: &Payload, _: &mut Window, _: &mut gpui_kit::App) {}

struct Host {
    ribbon: Option<Entity<RibbonState>>,
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let content = div().flex_1().child("content");
        match &self.ribbon {
            Some(ribbon) => RibbonScope::new(ribbon)
                .size_full()
                .flex()
                .flex_col()
                .child(content)
                .child(CommandRibbon::new(ribbon))
                .child(div().child(RibbonButton::new(ribbon)))
                .into_any_element(),
            // Baseline: the same tree with the ribbon replaced by an empty placeholder
            // and a plain status-bar button.
            None => div()
                .size_full()
                .flex()
                .flex_col()
                .child(content)
                .child(div())
                .child(div().child({
                    use gpui_kit::component::{Sizable, button::ButtonVariants};
                    gpui_kit::component::button::Button::new("ribbon-toggle")
                        .ghost()
                        .xsmall()
                        .label("Commands")
                        .tooltip("Command ribbon · Ctrl+Space")
                        .accessibility_label("Command ribbon")
                        .tab_stop(false)
                        .toggled(false)
                        .on_click(|_, _, _| {})
                }))
                .into_any_element(),
        }
    }
}

fn mount(
    cx: &mut TestAppContext,
    with_ribbon: bool,
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
                    let ribbon =
                        with_ribbon.then(|| cx.new(|cx| RibbonState::new(&commands, model, cx)));
                    *out.borrow_mut() = ribbon.clone();
                    let host = cx.new(|_| Host { ribbon });
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
    let (baseline, _) = mount(cx, false);
    let (closed, _) = mount(cx, true);
    let base = series(cx, baseline);
    let with = series(cx, closed);
    eprintln!("ribbon closed repaint: baseline {base:?}, with ribbon {with:?}");
    let base_min = base.iter().copied().min().unwrap_or(0);
    let with_min = with.iter().copied().min().unwrap_or(0);
    let with_max = with.iter().copied().max().unwrap_or(0);
    assert!(
        with_max <= base_min + CLOSED_OVERHEAD && with_min == with_max,
        "a closed ribbon allocates per frame: {with:?} vs placeholder {base:?}"
    );
}

#[gpui_kit::test]
fn an_open_unchanged_ribbon_stays_within_budget_and_does_not_grow(cx: &mut TestAppContext) {
    let (window, ribbon) = mount(cx, true);
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
