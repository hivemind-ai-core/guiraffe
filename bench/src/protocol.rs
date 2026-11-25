//! Mounted 1,000-control fixture for the provisional protocol gates.
use gpui_kit::{
    App, AppContext, Context, Entity, HeadlessAppContext, IntoElement, ParentElement, Render,
    Window, WindowHandle, div, px, size,
};
use guiraffe_core::{protocol::*, widget_value::Value};
use std::sync::Arc;

struct Counter(bool);
impl Observe for Counter {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        ControlSnapshot {
            checked: Some(self.0),
            ..Default::default()
        }
    }
}
impl Drive for Counter {
    type Action = ();
    fn decode(action: &Action) -> Result<(), ActionError> {
        if action.name.as_ref() == names::TOGGLE {
            Ok(())
        } else {
            Err(ActionError::Unsupported)
        }
    }
    fn apply(&mut self, _: (), _: &mut Window, _: &mut Context<Self>) -> Result<(), ActionError> {
        self.0 = !self.0;
        Ok(())
    }
}
struct Host(Vec<Entity<Counter>>);
impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().children(self.0.iter().map(|e| {
            if e.read(cx).0 {
                div().child("on")
            } else {
                div()
            }
        }))
    }
}
pub struct Fixture {
    app: HeadlessAppContext,
    window: WindowHandle<Host>,
    id: Option<ControlId>,
    action: Action,
}
impl Fixture {
    pub fn new(registered: bool) -> anyhow::Result<Self> {
        let text = gpui_kit::platform::current_platform(true).text_system();
        let mut app =
            HeadlessAppContext::with_platform(text, Arc::new(gpui_kit::assets::Assets), || {
                Ok(None)
            });
        let mut id = None;
        let window = app.open_window(size(px(800.), px(600.)), |window, cx| {
            let controls = (0..1000)
                .map(|i| {
                    let entity = cx.new(|_| Counter(false));
                    if registered {
                        id = Some(ActionBus::register(
                            &format!("page/{i}"),
                            &entity,
                            "toggle",
                            Capabilities::TOGGLE,
                            window,
                            cx,
                        ));
                    }
                    entity
                })
                .collect();
            cx.new(|_| Host(controls))
        })?;
        let mut fixture = Self {
            app,
            window,
            id,
            action: Action {
                name: names::TOGGLE.into(),
                payload: Value::Null,
            },
        };
        for _ in 0..4 {
            fixture.draw()?;
        }
        Ok(fixture)
    }
    pub fn draw(&mut self) -> anyhow::Result<()> {
        self.app
            .update_window(self.window.into(), |_, w, cx| w.draw(cx).clear(cx))?;
        Ok(())
    }
    /// Execute the measured operation outside Callgrind's generated wrapper module.
    pub fn dispatch_once(&mut self) -> anyhow::Result<()> {
        self.dispatch(|call| call())
    }
    /// Measure only the synchronous body within an existing window update.
    pub fn dispatch<R>(
        &mut self,
        measure: impl FnOnce(&mut dyn FnMut()) -> R,
    ) -> anyhow::Result<R> {
        let id = self
            .id
            .ok_or_else(|| anyhow::anyhow!("fixture has no registered target"))?;
        self.app.update_window(self.window.into(), |_, w, cx| {
            measure(&mut || {
                assert_eq!(ControlId::lookup("page/999", cx), Some(id));
                assert!(ActionBus::dispatch(id, &self.action, Source::Script, w, cx).is_ok());
            })
        })
    }
}
