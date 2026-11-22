//! Warm-repaint allocation gate: a draw of an unchanged mounted document must
//! not allocate. Counts on this thread only, so any allocation is attributable
//! to the repaint itself.

use std::sync::Arc;

use gpui_kit::{
    Entity, IntoElement, ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window,
    div, px,
};
use guiraffe_test::alloc::{CountingAllocator, count_allocations};
use guiraffe_ui::*;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const DOC: &str = r#"{
  "version": 1,
  "root": { "kind": "stack", "children": [
    { "kind": "title", "props": { "text": "Inbox", "order": 2 } },
    { "kind": "text", "id": "status", "props": { "text": "loading…" }, "bind": { "text": "status" } },
    { "kind": "text", "id": "live", "props": { "text": "LIVE", "color": "gray" }, "bind": { "color": "tone" } },
    { "kind": "button", "id": "refresh", "props": { "label": "Refresh" }, "on": { "click": "refresh" } },
    { "kind": "stack", "children": [
      { "kind": "text", "props": { "text": "row" } }
    ] }
  ] }
}"#;

struct Inbox {
    status: Option<SlotId>,
}

impl Logic for Inbox {
    fn bind(&mut self, names: &Names, _: &mut PatchBuf) {
        self.status = names.slot("status");
    }
    fn init(&mut self, out: &mut PatchBuf) {
        if let Some(status) = self.status {
            out.set(status, "3 unread");
        }
    }
}

struct Shell {
    ui: Entity<DataUi>,
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .child(div().w(px(120.)).child("static sidebar"))
            .child(self.ui.clone())
    }
}

fn mount(cx: &mut TestAppContext) -> (&mut VisualTestContext, Entity<DataUi>) {
    cx.update(guiraffe_components::init);
    let doc = Document::from_json(DOC).unwrap_or_else(|e| panic!("{e}"));
    let mut ui = None;
    let (_shell, vcx) = cx.add_window_view(|_, cx| {
        let u = DataUi::new(
            &doc,
            Arc::new(Registry::with_builtins()),
            Inbox { status: None },
            cx,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        ui = Some(u.clone());
        Shell { ui: u }
    });
    vcx.run_until_parked();
    let ui = ui.unwrap_or_else(|| unreachable!("window view built the host"));
    (vcx, ui)
}

fn warm_draw(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    vcx.run_until_parked();
}

fn median(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

/// The host itself (empty stack + shell) costs 14 allocations per repaint and
/// each extra node ~9 more (AnyElement erasure, the children `Vec`), with the
/// widget's own render on top. Both numbers are recorded in plans/ALLOC-AUDIT.md.
const DOC_BUDGET: u64 = 200;

#[gpui_kit::test]
fn warm_document_repaint_stays_within_budget_and_does_not_grow(cx: &mut TestAppContext) {
    let (vcx, _ui) = mount(cx);
    warm_draw(vcx);
    warm_draw(vcx);
    let series: Vec<u64> = (0..6)
        .map(|_| count_allocations(|| warm_draw(vcx)).1.allocations)
        .collect();
    eprintln!("DataUi warm repaint series: {series:?}");
    let last = series[series.len() - 1];
    assert!(
        last <= DOC_BUDGET,
        "an unchanged document repaint allocated {last}, over its budget of {DOC_BUDGET}: {series:?}"
    );
    let early = median(&series[..3]);
    let late = median(&series[3..]);
    assert!(
        late <= early,
        "repaint cost grows over frames (median): early {early}, late {late}: {series:?}"
    );
}
