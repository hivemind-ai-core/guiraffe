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
    type Output = ();
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
    batch: Vec<Action>,
}
/// Actions in the measured batch: the inline capacity of `Outcomes`.
pub const BATCH: usize = 8;
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
            batch: vec![
                Action {
                    name: names::TOGGLE.into(),
                    payload: Value::Null,
                };
                BATCH
            ],
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
            // Path resolution is a client-boundary cost; the measured body is dispatch alone.
            assert_eq!(ControlId::lookup("page/999", cx), Some(id));
            let mut result = Ok(Outcome::EMPTY);
            let measured = measure(&mut || {
                result = ActionBus::dispatch(id, &self.action, Source::Script, w, cx);
            });
            result
                .map(|_| measured)
                .map_err(|e| anyhow::anyhow!("dispatch failed: {e:?}"))
        })?
    }
    /// Execute one measured batch outside Callgrind's generated wrapper module.
    pub fn dispatch_batch_once(&mut self) -> anyhow::Result<()> {
        self.dispatch_batch(|call| call())
    }
    /// Measure one `dispatch_batch` of [`BATCH`] toggles within an existing window update.
    pub fn dispatch_batch<R>(
        &mut self,
        measure: impl FnOnce(&mut dyn FnMut()) -> R,
    ) -> anyhow::Result<R> {
        let id = self
            .id
            .ok_or_else(|| anyhow::anyhow!("fixture has no registered target"))?;
        self.app.update_window(self.window.into(), |_, w, cx| {
            let mut result = Ok(Outcomes::new());
            let measured = measure(&mut || {
                result = ActionBus::dispatch_batch(id, &self.batch, Source::Pointer, w, cx)
                    .into_result();
            });
            match result {
                Ok(outcomes) if outcomes.len() == BATCH => Ok(measured),
                other => Err(anyhow::anyhow!("batch failed: {other:?}")),
            }
        })?
    }
}

/// 1,000 adopted control entities of every kind, bound to paths or unbound.
pub struct Controls {
    app: HeadlessAppContext,
    window: WindowHandle<ControlHost>,
}
struct ControlHost(Vec<gpui_kit::AnyView>);
impl Render for ControlHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().children(self.0.iter().cloned())
    }
}
impl Controls {
    pub fn new(bound: bool) -> anyhow::Result<Self> {
        use guiraffe_components::controls::*;
        let text = gpui_kit::platform::current_platform(true).text_system();
        let mut app =
            HeadlessAppContext::with_platform(text, Arc::new(gpui_kit::assets::Assets), || {
                Ok(None)
            });
        app.update(guiraffe_components::init);
        let window = app.open_window(size(px(800.), px(600.)), |window, cx| {
            let views = (0..1000)
                .map(|i| {
                    let path = (bound, format!("page/{i}"));
                    let view: gpui_kit::AnyView = match i % 8 {
                        0 => with_path(
                            ButtonControl::new().label("Go"),
                            path,
                            ButtonBuilder::path,
                            ButtonBuilder::element_id,
                        )
                        .build(window, cx)
                        .into(),
                        1 => with_path(
                            CheckboxControl::new().label("On"),
                            path,
                            CheckboxBuilder::path,
                            CheckboxBuilder::element_id,
                        )
                        .build(window, cx)
                        .into(),
                        2 => with_path(
                            SwitchControl::new(),
                            path,
                            SwitchBuilder::path,
                            SwitchBuilder::element_id,
                        )
                        .build(window, cx)
                        .into(),
                        3 => with_path(
                            SliderControl::new(),
                            path,
                            SliderBuilder::path,
                            SliderBuilder::element_id,
                        )
                        .build(window, cx)
                        .into(),
                        4 => with_path(
                            TextInputControl::new().value("x"),
                            path,
                            TextInputBuilder::path,
                            TextInputBuilder::element_id,
                        )
                        .build(window, cx)
                        .into(),
                        5 => with_path(
                            SelectControl::new().options(["a", "b"]),
                            path,
                            SelectBuilder::path,
                            SelectBuilder::element_id,
                        )
                        .build(window, cx)
                        .into(),
                        6 => with_path(
                            TabsControl::new().tabs(["a", "b"]),
                            path,
                            TabsBuilder::path,
                            TabsBuilder::element_id,
                        )
                        .build(window, cx)
                        .into(),
                        _ => with_path(
                            MenuControl::new().item("a"),
                            path,
                            MenuBuilder::path,
                            MenuBuilder::element_id,
                        )
                        .build(window, cx)
                        .into(),
                    };
                    view
                })
                .collect();
            cx.new(|_| ControlHost(views))
        })?;
        let mut controls = Self { app, window };
        for _ in 0..4 {
            controls.draw()?;
        }
        Ok(controls)
    }
    pub fn draw(&mut self) -> anyhow::Result<()> {
        self.app
            .update_window(self.window.into(), |_, w, cx| w.draw(cx).clear(cx))?;
        Ok(())
    }
    pub fn registered(&mut self) -> anyhow::Result<usize> {
        self.app.update_window(self.window.into(), |_, w, cx| {
            ActionBus::controls(w, cx).count()
        })
    }
}

/// Bound controls get `path`; unbound ones the same text as an element id, so the
/// two fixtures differ only by registration.
fn with_path<B>(
    builder: B,
    path: (bool, String),
    set: fn(B, String) -> B,
    name: fn(B, String) -> B,
) -> B {
    match path {
        (true, path) => set(builder, path),
        (false, path) => name(builder, path),
    }
}
