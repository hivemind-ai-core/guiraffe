//! Setting a mounted flow's selection by id (#161): exact, a no-op when unchanged, outside
//! undo history, leaving the viewport, positions and keyboard focus alone, and tagged with
//! its source in `FlowEvent::SelectionChanged`.
use std::{cell::RefCell, rc::Rc};

use gpui_kit::{Entity, Focusable, TestAppContext, VisualTestContext};
use guiraffe_core::protocol::{ActionBus, Source};
use guiraffe_flow::{
    FlowAction, FlowEvent, FlowState,
    core::{Edge, Graph, Node, Point, Viewport},
};

fn graph() -> Graph {
    Graph::from_parts(
        [
            Node::new(1, "One", Point::new(80., 120.)),
            Node::new(2, "Two", Point::new(480., 120.)),
            Node::new(3, "Three", Point::new(480., 320.)),
        ],
        [Edge::new(1, 1, 2), Edge::new(2, 1, 3)],
    )
    .unwrap_or_else(|e| panic!("{e:?}"))
}

type Sources = Rc<RefCell<Vec<Source>>>;

fn mount(cx: &mut TestAppContext) -> (Entity<FlowState>, Sources, Sources, &mut VisualTestContext) {
    cx.update(guiraffe_core::init);
    let (view, vcx) =
        cx.add_window_view(|_, cx| FlowState::new(graph(), cx).unwrap_or_else(|e| panic!("{e:?}")));
    vcx.run_until_parked();
    let model: Sources = Rc::default();
    let canvas: Sources = Rc::default();
    vcx.update(|_, cx| {
        let sink = model.clone();
        cx.subscribe(&view, move |_, event: &FlowEvent, _| {
            if let FlowEvent::SelectionChanged { source } = event {
                sink.borrow_mut().push(*source);
            }
        })
        .detach();
        let sink = canvas.clone();
        let viewport = view.read(cx).viewport_entity().clone();
        cx.subscribe(&viewport, move |_, event: &FlowEvent, _| {
            if let FlowEvent::SelectionChanged { source } = event {
                sink.borrow_mut().push(*source);
            }
        })
        .detach();
    });
    (view, model, canvas, vcx)
}

fn selected(
    view: &Entity<FlowState>,
    vcx: &mut VisualTestContext,
) -> (Vec<u64>, Vec<u64>, Vec<u64>) {
    vcx.update(|_, cx| {
        let state = view.read(cx);
        let canvas = state.viewport_entity().read(cx);
        (
            state.selected_nodes().iter().copied().collect(),
            state.selected_edges().iter().copied().collect(),
            canvas.selected_nodes().iter().copied().collect(),
        )
    })
}

#[gpui_kit::test]
fn sets_the_selection_and_the_same_selection_is_a_no_op(cx: &mut TestAppContext) {
    let (view, model, canvas, vcx) = mount(cx);
    let changed = vcx.update(|_, cx| {
        view.update(cx, |state, cx| {
            state.set_selection([1, 3, 99], [2], Source::Host, cx)
        })
    });
    vcx.run_until_parked();
    assert!(changed);
    assert_eq!(selected(&view, vcx), (vec![1, 3], vec![2], vec![1, 3]));
    assert_eq!(*model.borrow(), [Source::Host]);
    assert_eq!(*canvas.borrow(), [Source::Host]);

    let again = vcx.update(|_, cx| {
        view.update(cx, |state, cx| {
            state.set_selection([3, 1], [2], Source::Host, cx)
        })
    });
    vcx.run_until_parked();
    assert!(!again, "the current selection again changes nothing");
    assert_eq!(model.borrow().len(), 1, "and emits nothing");
    assert_eq!(canvas.borrow().len(), 1);
}

#[gpui_kit::test]
fn history_viewport_positions_and_focus_are_left_alone(cx: &mut TestAppContext) {
    let (view, _, _, vcx) = mount(cx);
    let camera = Viewport {
        pan: Point::new(-40., 25.),
        zoom: 1.4,
    };
    vcx.update(|window, cx| {
        view.update(cx, |state, cx| state.set_viewport(camera, cx))
            .unwrap_or_else(|e| panic!("{e:?}"));
        let focus = view.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    });
    vcx.run_until_parked();
    let before = vcx.update(|window, cx| {
        let state = view.read(cx);
        (
            state.graph().clone(),
            state.viewport(),
            state.focus_handle(cx).is_focused(window),
            state.can_undo(),
        )
    });
    assert!(before.2, "the canvas has focus");
    vcx.update(|_, cx| {
        view.update(cx, |state, cx| {
            state.set_selection([2], [], Source::Host, cx)
        });
        view.update(cx, |state, cx| {
            state.set_selection([3], [1], Source::Host, cx)
        });
    });
    vcx.run_until_parked();
    let after = vcx.update(|window, cx| {
        let state = view.read(cx);
        (
            state.graph().clone(),
            state.viewport(),
            state.focus_handle(cx).is_focused(window),
            state.can_undo(),
        )
    });
    assert_eq!(
        after, before,
        "graph, viewport, focus and history unchanged"
    );
    // Undo has nothing to take back: the selection stays.
    vcx.update(|_, cx| view.update(cx, |state, cx| state.undo(cx)));
    vcx.run_until_parked();
    assert_eq!(selected(&view, vcx).0, vec![3]);
}

#[gpui_kit::test]
fn the_bus_and_the_keyboard_tag_their_changes(cx: &mut TestAppContext) {
    let (view, model, canvas, vcx) = mount(cx);
    vcx.update(|window, cx| {
        let id = FlowState::register(&view, "patch/flow", window, cx);
        let action = FlowAction::Select {
            nodes: vec![2],
            edges: vec![],
        }
        .to_action();
        assert!(ActionBus::dispatch(id, &action, Source::Mcp, window, cx).is_ok());
        // Again: a no-op, so no second event.
        assert!(ActionBus::dispatch(id, &action, Source::Mcp, window, cx).is_ok());
    });
    vcx.run_until_parked();
    assert_eq!(selected(&view, vcx).0, vec![2]);
    assert_eq!(*model.borrow(), [Source::Mcp]);

    vcx.update(|window, cx| {
        let focus = view.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    });
    vcx.simulate_keystrokes("ctrl-a");
    vcx.run_until_parked();
    assert_eq!(selected(&view, vcx).0, vec![1, 2, 3]);
    assert_eq!(model.borrow().last(), Some(&Source::Keyboard));
    assert_eq!(canvas.borrow().last(), Some(&Source::Keyboard));
}
