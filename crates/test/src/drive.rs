//! The drive API (spec/21 §2): the actions an agent performs on a running app.
//!
//! Every action answers with what it changed: the events the app emitted, the
//! focus that moved and the nodes that were laid out again.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{ScrollDelta, VisualTestContext, px};
use serde::{Deserialize, Serialize};

use crate::inspect::{EventSink, NodeId, NodeIndex, RecordedEvent};

/// An action an agent performs on the app.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    /// Clicks the node.
    Click { id: NodeId },
    /// Moves the pointer over the node.
    Hover { id: NodeId },
    /// Types text into the node.
    Type { id: NodeId, text: String },
    /// Presses a key.
    Key { key: String },
    /// Moves the keyboard focus to the node.
    Focus { id: NodeId },
    /// Scrolls the node.
    Scroll { id: NodeId, dx: f32, dy: f32 },
    /// Drags the node onto another node.
    Drag { id: NodeId, to: NodeId },
    /// Resizes the window.
    Resize { width: f32, height: f32 },
    /// Switches the theme of the app.
    SetTheme { scheme: String },
    /// Switches the fixture of the app.
    SetFixture { name: String },
}

impl Action {
    /// The name of the action, as it is reported.
    pub fn name(&self) -> &'static str {
        match self {
            Action::Click { .. } => "click",
            Action::Hover { .. } => "hover",
            Action::Type { .. } => "type",
            Action::Key { .. } => "key",
            Action::Focus { .. } => "focus",
            Action::Scroll { .. } => "scroll",
            Action::Drag { .. } => "drag",
            Action::Resize { .. } => "resize",
            Action::SetTheme { .. } => "set_theme",
            Action::SetFixture { .. } => "set_fixture",
        }
    }
}

/// The focus before and after an action.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FocusChange {
    /// The node that had the focus before the action.
    pub before: Option<NodeId>,
    /// The node that has the focus after the action.
    pub after: Option<NodeId>,
}

impl FocusChange {
    /// Whether the focus moved.
    pub fn moved(&self) -> bool {
        self.before != self.after
    }
}

/// What an action changed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DriveResult {
    /// The name of the action.
    pub action: String,
    /// Whether the action was performed.
    pub ok: bool,
    /// Why the action failed, when it did.
    pub error: Option<String>,
    /// The events the app emitted while the action ran.
    pub events: Vec<RecordedEvent>,
    /// The focus before and after the action.
    pub focus: FocusChange,
    /// The nodes whose bounds changed.
    pub rerendered: Vec<NodeId>,
}

/// A hook the app under inspection lets the driver use.
pub type Hook = Box<dyn FnMut(&str, &mut gpui_kit::App) -> bool>;

/// The driver: performs actions on a running app.
pub struct Driver<'a> {
    vcx: &'a mut VisualTestContext,
    events: EventSink,
    theme: Option<Hook>,
    fixture: Option<Hook>,
}

impl<'a> Driver<'a> {
    /// A driver over a running window.
    pub fn new(vcx: &'a mut VisualTestContext, events: EventSink) -> Self {
        Self {
            vcx,
            events,
            theme: None,
            fixture: None,
        }
    }

    /// The hook that switches the theme of the app.
    pub fn with_theme(mut self, hook: Hook) -> Self {
        self.theme = Some(hook);
        self
    }

    /// The hook that switches the fixture of the app.
    pub fn with_fixture(mut self, hook: Hook) -> Self {
        self.fixture = Some(hook);
        self
    }

    /// Performs an action and answers what it changed.
    pub fn drive(&mut self, action: &Action) -> DriveResult {
        let before = self.nodes();
        let events_before = self.events.len();
        let focus_before = self.focused(&before);
        let mut result = DriveResult {
            action: action.name().to_string(),
            ..Default::default()
        };
        match self.perform(action, &mut result) {
            Ok(()) => result.ok = true,
            Err(error) => result.error = Some(error),
        }
        self.vcx.run_until_parked();
        let after = self.nodes();
        result.events = self
            .events
            .events()
            .into_iter()
            .skip(events_before)
            .collect();
        result.focus = FocusChange {
            before: focus_before,
            after: self.focused(&after),
        };
        result.rerendered = moved_nodes(&before, &after);
        result
    }

    /// Performs the action, without measuring what it changed.
    fn perform(&mut self, action: &Action, result: &mut DriveResult) -> Result<(), String> {
        match action {
            Action::Click { id } => {
                let id = self.require(id)?;
                self.vcx.update(|window, cx| {
                    window.click(id, cx);
                });
            }
            Action::Hover { id } => {
                let id = self.require(id)?;
                self.vcx.update(|window, cx| {
                    window.hover(id, cx);
                });
            }
            Action::Type { id, text } => {
                self.focus_node(id)?;
                let text = text.clone();
                self.vcx.update(|window, cx| {
                    window.input(&text, cx);
                });
            }
            Action::Key { key } => {
                let key = key.clone();
                if key.eq_ignore_ascii_case("tab") {
                    // The Tab key moves the focus; the framework owns the
                    // order, so the driver asks it for the next stop.
                    self.vcx.update(|window, cx| {
                        window.focus_next(cx);
                    });
                } else {
                    self.vcx.update(|window, cx| {
                        window.press(&key, cx);
                    });
                }
            }
            Action::Focus { id } => self.focus_node(id)?,
            Action::Scroll { id, dx, dy } => {
                let id = self.require(id)?;
                let delta = ScrollDelta::Pixels(gpui_kit::Point {
                    x: px(*dx),
                    y: px(*dy),
                });
                self.vcx.update(|window, cx| {
                    window.scroll(id, delta, cx);
                });
            }
            Action::Drag { id, to } => {
                let from = self.require(id)?;
                let to = self.require(to)?;
                self.vcx.update(|window, cx| {
                    window.drag_to(from, to, cx);
                });
            }
            Action::Resize { width, height } => {
                let size = gpui_kit::Size {
                    width: px(*width),
                    height: px(*height),
                };
                self.vcx.update(|window, _| {
                    window.resize(size);
                });
            }
            Action::SetTheme { scheme } => {
                let scheme = scheme.clone();
                let Some(mut hook) = self.theme.take() else {
                    return Err("the app does not let the driver switch the theme".into());
                };
                let changed = self.vcx.update(|_, cx| hook(&scheme, cx));
                self.theme = Some(hook);
                if !changed {
                    return Err(format!("the app does not know the theme {scheme:?}"));
                }
            }
            Action::SetFixture { name } => {
                let name = name.clone();
                let Some(mut hook) = self.fixture.take() else {
                    return Err("the app does not let the driver switch the fixture".into());
                };
                let changed = self.vcx.update(|_, cx| hook(&name, cx));
                self.fixture = Some(hook);
                if !changed {
                    return Err(format!("the app does not know the fixture {name:?}"));
                }
            }
        }
        let _ = result;
        Ok(())
    }

    /// The element id of a node, when the node is there.
    fn require(&mut self, id: &NodeId) -> Result<gpui_kit::ElementId, String> {
        let index = self.nodes();
        if index.node(id).is_none() {
            return Err(format!("there is no node {id}"));
        }
        Ok(id.element_id())
    }

    /// Moves the keyboard focus to a node, with the Tab key.
    fn focus_node(&mut self, id: &NodeId) -> Result<(), String> {
        let index = self.nodes();
        if index.node(id).is_none() {
            return Err(format!("there is no node {id}"));
        }
        for _ in 0..index.nodes().len().saturating_add(2) {
            if self.focused(&index).as_ref() == Some(id) {
                return Ok(());
            }
            self.vcx.update(|window, cx| {
                window.focus_next(cx);
            });
            self.vcx.run_until_parked();
            let index = self.nodes();
            if self.focused(&index).as_ref() == Some(id) {
                return Ok(());
            }
        }
        Err(format!("the focus never reached {id}"))
    }

    /// The nodes of the last frame.
    pub fn nodes(&mut self) -> NodeIndex {
        self.vcx.update(|window, _| NodeIndex::collect(window))
    }

    /// The accessibility facts of the last frame, with the Tab order.
    pub fn a11y(&mut self) -> crate::inspect::A11yReport {
        crate::inspect::a11y_report(self.vcx)
    }

    /// The node that has the keyboard focus now.
    fn focused(&mut self, index: &NodeIndex) -> Option<NodeId> {
        index
            .nodes()
            .iter()
            .find(|node| node.a11y.focused)
            .map(|node| node.id.clone())
    }
}

/// The nodes whose bounds changed between two frames.
fn moved_nodes(before: &NodeIndex, after: &NodeIndex) -> Vec<NodeId> {
    after
        .nodes()
        .iter()
        .filter(|node| {
            before
                .node(&node.id)
                .is_some_and(|old| old.layout.bounds != node.layout.bounds)
        })
        .map(|node| node.id.clone())
        .collect()
}
