//! Lists, conditions and native two-way inputs from one compiled document.
use gpui_kit::{AppContext, SharedString, WindowOptions};
use guiraffe_ui::{
    Bindings, DataUi, Document, Event, Logic, Names, PatchBuf, Payload, Registry, Slot, Value,
};
use std::sync::Arc;
#[derive(Default)]
struct Demo {
    title: Slot<SharedString>,
    rows: Slot<Value>,
    add: Event,
    delete: Event,
    next_id: usize,
    text: SharedString,
    count: usize,
}
impl Logic for Demo {
    fn bind(&mut self, names: &Names, out: &mut PatchBuf) {
        let mut b = Bindings::new(names);
        self.title = b.slot("title");
        self.rows = b.slot("rows");
        self.add = b.event("add");
        self.delete = b.event("delete");
        for diagnostic in b.diagnostics() {
            eprintln!("{}: {}", diagnostic.path, diagnostic.message);
        }
        out.set(self.title, "New item");
        self.text = "New item".into();
        out.set(self.rows, Value::List(Arc::from([])));
        self.count = 0;
    }
    fn on_event(&mut self, event: guiraffe_ui::EventId, _: &Value, out: &mut PatchBuf) {
        if self.add.matches(event) {
            let record = Value::Record(Arc::new(
                [
                    ("id".into(), Value::from(format!("row-{}", self.next_id))),
                    ("title".into(), Value::Str(self.text.clone())),
                    ("done".into(), Value::Bool(false)),
                ]
                .into_iter()
                .collect(),
            ));
            out.splice(self.rows, self.count..self.count, Arc::from([record]));
            self.count += 1;
            self.next_id += 1;
        }
    }
    fn on_payload(&mut self, event: guiraffe_ui::EventId, payload: &Payload, out: &mut PatchBuf) {
        if let Some(item) = payload.item() {
            if self.delete.matches(event) {
                out.splice(item.slot, item.index..item.index + 1, Arc::from([]));
                self.count = self.count.saturating_sub(1);
            }
            if let Payload::Text(text) = payload.input()
                && text.len() > 80
            {
                out.veto();
            }
            return;
        }
        if let Payload::Text(text) = payload.input() {
            if text.len() > 80 {
                out.veto();
            } else {
                self.text = text.clone();
            }
        }
        self.on_event(event, &payload.value(), out);
    }
}
const DOC: &str = r#"{"version":1,"root":{"kind":"stack","children":[
{"kind":"title","props":{"text":"Data UI bindings"}},
{"kind":"text-input","props":{"placeholder":"Item title"},"bind":{"value":"title"}},
{"kind":"button","props":{"label":"Add item"},"on":{"click":"add"}},
{"kind":"checkbox","props":{"label":"Show rows","value":true},"bind":{"value":"visible"}},
{"kind":"switch","props":{"label":"Rows visible"},"bind":{"value":"visible"}},
{"kind":"select","props":{"options":["Compact","Comfortable"],"value":"Comfortable"},"bind":{"value":"mode"}},
{"kind":"text","bind":{"text":"mode"}},
{"kind":"stack","when":"visible","each":"rows","key":"id","children":[{"kind":"stack","children":[
{"kind":"text-input","bind":{"value":"@item.title"}},
{"kind":"checkbox","props":{"label":"Done"},"bind":{"value":"@item.done"}},
{"kind":"button","props":{"label":"Delete row"},"on":{"click":"delete"}}
]}]}
]}}"#;
fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            guiraffe_components::init(cx);
            let document = match Document::from_json(DOC) {
                Ok(document) => document,
                Err(error) => {
                    eprintln!("{error}");
                    cx.quit();
                    return;
                }
            };
            let result = cx.open_window(WindowOptions::default(), |window, cx| {
                let host = DataUi::new(
                    &document,
                    Arc::new(Registry::with_builtins()),
                    Demo::default(),
                    cx,
                )
                .expect("valid example document");
                cx.new(|cx| gpui_kit::component::Root::new(host, window, cx))
            });
            if let Err(error) = result {
                eprintln!("Cannot open example: {error}");
                cx.quit();
            }
        });
}
