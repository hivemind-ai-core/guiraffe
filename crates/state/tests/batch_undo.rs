//! `ActionBus::dispatch_batch` writes to a store with history as one undo step.
use gpui_kit::{AppContext, Context, Empty, Entity, TestAppContext, VisualTestContext, Window};
use guiraffe_core::{protocol::*, widget_value::Value};
use guiraffe_state::{Event, Fx, Store};

#[derive(Clone, Debug, Default, PartialEq)]
struct Canvas {
    cells: [bool; 8],
}

#[derive(Debug)]
struct Paint(usize);
impl Event<Canvas> for Paint {
    fn handle(self, fx: &mut Fx<'_, Canvas>) {
        fx.update(|c| c.cells[self.0] = true);
    }
}

fn painted(store: &Store<Canvas>, cx: &mut VisualTestContext) -> Vec<usize> {
    let state = cx.update(|_, cx| store.state(cx));
    (0..8).filter(|&i| state.cells[i]).collect()
}

/// A paint brush control: `set_value(i)` paints cell `i`; a negative `i` is rejected.
struct Brush {
    store: Store<Canvas>,
    now: bool,
}
impl Observe for Brush {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &gpui_kit::App) -> ControlSnapshot {
        ControlSnapshot::default()
    }
}
impl Drive for Brush {
    type Action = usize;
    type Output = ();
    fn decode(action: &Action) -> Result<usize, ActionError> {
        match (action.name.as_ref(), &action.payload) {
            (names::SET_VALUE, Value::Num(n)) if *n >= 0. => Ok(*n as usize),
            _ => Err(ActionError::Invalid("cell")),
        }
    }
    fn apply(
        &mut self,
        i: usize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ActionError> {
        if self.now {
            self.store.dispatch_now(Paint(i), cx);
        } else {
            self.store.dispatch(Paint(i), cx);
        }
        Ok(())
    }
}

/// Fills a range by running a nested batch on the brush.
struct Filler {
    brush: ControlId,
}
impl Observe for Filler {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &gpui_kit::App) -> ControlSnapshot {
        ControlSnapshot::default()
    }
}
impl Drive for Filler {
    type Action = usize;
    type Output = ();
    fn decode(action: &Action) -> Result<usize, ActionError> {
        match &action.payload {
            Value::Num(n) => Ok(*n as usize),
            _ => Err(ActionError::Invalid("start")),
        }
    }
    fn apply(
        &mut self,
        start: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), ActionError> {
        let actions = [paint(start as f64), paint(start as f64 + 1.)];
        ActionBus::dispatch_batch(self.brush, &actions, Source::Script, window, cx)
            .into_result()
            .map(drop)
            .map_err(|e| e.error)
    }
}

fn paint(i: f64) -> Action {
    Action {
        name: names::SET_VALUE.into(),
        payload: Value::Num(i),
    }
}

struct Fixture {
    store: Store<Canvas>,
    brush: ControlId,
    filler: ControlId,
    _keep: (Entity<Brush>, Entity<Filler>),
}

fn fixture(cx: &mut TestAppContext, now: bool) -> (Fixture, &mut VisualTestContext) {
    let (_, vcx) = cx.add_window_view(|_, _| Empty);
    let f = vcx.update(|window, cx| {
        let store = Store::new(Canvas::default(), cx);
        store.enable_undo(50, cx);
        let brush_entity = cx.new(|_| Brush {
            store: store.clone(),
            now,
        });
        let brush = ActionBus::register(
            "canvas/brush",
            &brush_entity,
            "brush",
            Capabilities::SET_VALUE,
            window,
            cx,
        );
        let filler_entity = cx.new(|_| Filler { brush });
        let filler = ActionBus::register(
            "canvas/fill",
            &filler_entity,
            "fill",
            Capabilities::SET_VALUE,
            window,
            cx,
        );
        Fixture {
            store,
            brush,
            filler,
            _keep: (brush_entity, filler_entity),
        }
    });
    (f, vcx)
}

fn undo(f: &Fixture, cx: &mut VisualTestContext) -> bool {
    let undone = cx.update(|_, cx| {
        let can = f.store.can_undo(cx);
        f.store.dispatch(guiraffe_state::UndoEvent, cx);
        can
    });
    cx.run_until_parked();
    undone
}

#[gpui_kit::test]
fn a_drag_paint_batch_undoes_in_one_step(cx: &mut TestAppContext) {
    for now in [false, true] {
        let (f, vcx) = fixture(cx, now);
        vcx.update(|window, cx| {
            // Queued before the batch: its own step, not part of the group.
            f.store.dispatch(Paint(7), cx);
            let strokes: Vec<_> = (0..5).map(|i| paint(i as f64)).collect();
            let batch = ActionBus::dispatch_batch(f.brush, &strokes, Source::Pointer, window, cx);
            assert!(batch.error.is_none());
        });
        vcx.run_until_parked();
        assert_eq!(painted(&f.store, vcx), [0, 1, 2, 3, 4, 7], "now={now}");
        assert!(undo(&f, vcx));
        assert_eq!(
            painted(&f.store, vcx),
            [7],
            "the batch undid at once (now={now})"
        );
        assert!(undo(&f, vcx));
        assert_eq!(painted(&f.store, vcx), Vec::<usize>::new());
        assert!(!undo(&f, vcx));
    }
}

#[gpui_kit::test]
fn a_rejected_action_mid_batch_leaves_one_consistent_step(cx: &mut TestAppContext) {
    let (f, vcx) = fixture(cx, false);
    vcx.update(|window, cx| {
        let strokes = [paint(0.), paint(1.), paint(-1.), paint(3.)];
        let batch = ActionBus::dispatch_batch(f.brush, &strokes, Source::Pointer, window, cx);
        assert_eq!(batch.error.map(|e| e.index), Some(2));
        assert_eq!(batch.outcomes.len(), 2);
    });
    vcx.run_until_parked();
    assert_eq!(painted(&f.store, vcx), [0, 1], "applied prefix only");
    assert!(undo(&f, vcx));
    assert_eq!(painted(&f.store, vcx), Vec::<usize>::new());
    assert!(!vcx.update(|_, cx| f.store.can_undo(cx)));
    vcx.update(|_, cx| f.store.dispatch(guiraffe_state::RedoEvent, cx));
    vcx.run_until_parked();
    assert_eq!(
        painted(&f.store, vcx),
        [0, 1],
        "redo restores the whole prefix"
    );
    // A later single write is a new step after the rejected batch.
    vcx.update(|window, cx| {
        assert!(ActionBus::dispatch(f.brush, &paint(5.), Source::Script, window, cx).is_ok());
    });
    vcx.run_until_parked();
    assert!(undo(&f, vcx));
    assert_eq!(painted(&f.store, vcx), [0, 1]);
}

#[gpui_kit::test]
fn single_dispatches_and_consecutive_batches_stay_separate_steps(cx: &mut TestAppContext) {
    let (f, vcx) = fixture(cx, false);
    vcx.update(|window, cx| {
        for i in [0., 1.] {
            assert!(ActionBus::dispatch(f.brush, &paint(i), Source::Script, window, cx).is_ok());
        }
        // Two batches in the same tick are two steps.
        for pair in [[2., 3.], [4., 5.]] {
            let strokes = pair.map(paint);
            assert!(
                ActionBus::dispatch_batch(f.brush, &strokes, Source::Pointer, window, cx)
                    .error
                    .is_none()
            );
        }
    });
    vcx.run_until_parked();
    let mut seen = Vec::new();
    while undo(&f, vcx) {
        seen.push(painted(&f.store, vcx));
    }
    assert_eq!(
        seen,
        [vec![0, 1, 2, 3], vec![0, 1], vec![0], vec![]],
        "batch, batch, then one step per single dispatch"
    );
}

#[gpui_kit::test]
fn nested_batches_are_one_step(cx: &mut TestAppContext) {
    let (f, vcx) = fixture(cx, false);
    vcx.update(|window, cx| {
        // Each fill runs an inner batch of two paints: six writes in all.
        let fills = [paint(0.), paint(2.), paint(4.)];
        assert!(
            ActionBus::dispatch_batch(f.filler, &fills, Source::Script, window, cx)
                .error
                .is_none()
        );
    });
    vcx.run_until_parked();
    assert_eq!(painted(&f.store, vcx), [0, 1, 2, 3, 4, 5]);
    assert!(undo(&f, vcx));
    assert_eq!(painted(&f.store, vcx), Vec::<usize>::new());
    assert!(!undo(&f, vcx));
}
