//! Warm-repaint allocation gate for the command menu: the same menu, drawn
//! again with nothing changed, must not allocate more than the previous frame.

use gpui_kit::{
    AppContext, Bounds, Context, FocusHandle, IntoElement, ParentElement, Render, Styled,
    TestAppContext, Window, WindowBounds, WindowOptions, div, point, px, size,
};
use guiraffe_commands::{
    Command, CommandId, CommandScope, Commands, Keybinding, Menu, Payload, When, init,
};
use guiraffe_test::alloc::{CountingAllocator, count_allocations};

fn noop(_: &Payload, _: &mut Window, _: &mut gpui_kit::App) {}

/// 18 rows in 3 categories; measured ~858 upstream: see plans/ALLOC-AUDIT.md.
const MENU_BUDGET: u64 = 1_000;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct Host {
    commands: Commands,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        CommandScope::new(&self.commands).size_full().child(
            div()
                .size_full()
                .child(Menu::new("command-menu", &self.commands)),
        )
    }
}

/// A menu over 3 categories × 6 commands, mounted in a real headless window.
fn mount(cx: &mut TestAppContext) -> gpui_kit::WindowHandle<gpui_kit::base::Root> {
    cx.update(init);
    cx.update(|cx| {
        let commands = Commands::new(cx);
        for (index, category) in ["File", "Edit", "View"].into_iter().enumerate() {
            for step in 0..6_u8 {
                let id = CommandId::parse(&format!("{}.action{step}", category.to_lowercase()))
                    .unwrap_or_else(|e| panic!("{e}"));
                let key = char::from(b'a' + index as u8 * 6 + step);
                commands
                    .register(
                        Command::new(
                            id,
                            format!("{category} action {step}"),
                            noop as fn(&Payload, &mut Window, &mut gpui_kit::App),
                        )
                        .category(category)
                        .keybinding(
                            Keybinding::parse(&format!("secondary+{key}"))
                                .unwrap_or_else(|e| panic!("{e}")),
                        )
                        .when(When::always()),
                        cx,
                    )
                    .unwrap_or_else(|e| panic!("{e}"));
            }
        }
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(600.), px(500.)),
                ))),
                ..Default::default()
            },
            move |window, cx| {
                let focus: FocusHandle = cx.focus_handle();
                focus.focus(window, cx);
                let host = cx.new(|_| Host { commands });
                guiraffe_overlays::root(host, window, cx)
            },
        )
        .expect("window")
    })
}

fn median(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn count_repaint(
    cx: &mut TestAppContext,
    window: gpui_kit::WindowHandle<gpui_kit::base::Root>,
) -> guiraffe_test::alloc::Counts {
    let (_, counts) = count_allocations(|| {
        cx.update_window(window.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .expect("frame");
        cx.run_until_parked();
    });
    counts
}

#[gpui_kit::test]
fn warm_menu_repaint_stays_within_budget_and_does_not_grow(cx: &mut TestAppContext) {
    let window = mount(cx);
    cx.run_until_parked();
    for _ in 0..3 {
        count_repaint(cx, window);
    }
    let series: Vec<u64> = (0..6)
        .map(|_| count_repaint(cx, window).allocations)
        .collect();
    eprintln!("commands menu warm repaint series: {series:?}");
    let peak = series.iter().copied().max().unwrap_or(0);
    assert!(
        peak <= MENU_BUDGET,
        "the command menu repaint peaks at {peak}, over its budget of {MENU_BUDGET}: {series:?}"
    );
    let early = median(&series[..3]);
    let late = median(&series[3..]);
    assert!(
        late <= early,
        "the command menu repaint grows (median): early {early}, late {late}: {series:?}"
    );
}
