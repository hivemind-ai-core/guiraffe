//! DataUi requests delegated to the window's existing overlay host.

use std::{cell::RefCell, collections::BTreeMap, rc::Rc, time::Duration};

use gpui_kit::{
    App, ElementId, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window,
    component::{
        button::Button,
        dialog::{DialogAction, DialogClose, DialogFooter},
    },
    div, px,
};
use guiraffe_core::{
    Size,
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value,
};

use crate::{
    Drawer, Modal, OverlayHost,
    position::Side,
    stack::{OverlayId, Policy},
    toast::{Toast, ToastId, ToastPosition},
    widget_specs,
};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[
            WidgetRegistration {
                spec: &widget_specs::MODAL,
                schema: guiraffe_core::widget_schema!(&widget_specs::MODAL),
                render: |node, _, _, _| DocumentModal::new(node, false).into_any_element(),
            },
            WidgetRegistration {
                spec: &widget_specs::DRAWER,
                schema: guiraffe_core::widget_schema!(&widget_specs::DRAWER),
                render: |node, _, _, _| DocumentModal::new(node, true).into_any_element(),
            },
            WidgetRegistration {
                spec: &widget_specs::TOAST_TRAY,
                schema: guiraffe_core::widget_schema!(&widget_specs::TOAST_TRAY),
                render: |node, children, _, _| {
                    DocumentTray {
                        key: node.state_id(),
                        toasts: node.prop(0).clone(),
                        limit: node
                            .prop(1)
                            .as_f64()
                            .map(|limit| limit.clamp(0., 100.) as usize),
                        error: node.handler(0),
                        children,
                    }
                    .into_any_element()
                },
            },
        ],
    );
}

#[derive(Clone, PartialEq)]
struct ModalContent {
    title: SharedString,
    message: SharedString,
    ok: Option<SharedString>,
    cancel: Option<SharedString>,
    width: Option<f32>,
    offset_top: Option<f32>,
    label: Option<SharedString>,
    overlay: bool,
    close_button: bool,
    resizable: bool,
}

type SharedModalContent = Rc<RefCell<(ModalContent, [Option<EventHandler>; 3])>>;

struct ModalState {
    requested: Option<ModalTarget>,
    opened: Option<OverlayId>,
    content: SharedModalContent,
}

#[derive(Clone, PartialEq, Eq)]
struct ModalTarget {
    policy: Policy,
    context: Option<SharedString>,
    sheet: Option<(Side, Size)>,
}

#[derive(IntoElement)]
struct DocumentModal {
    key: ElementId,
    target: Option<ModalTarget>,
    content: ModalContent,
    handlers: [Option<EventHandler>; 3],
    error: Option<EventHandler>,
}

impl DocumentModal {
    fn new(node: &dyn WidgetNode, drawer: bool) -> Self {
        let text = |index| {
            node.prop(index)
                .as_str()
                .map(|value| value.to_owned().into())
        };
        if drawer {
            return Self {
                key: node.state_id(),
                target: node
                    .prop(0)
                    .as_bool()
                    .unwrap_or(false)
                    .then(|| ModalTarget {
                        policy: Policy {
                            escape: node.prop(5).as_bool().unwrap_or(true),
                            outside: node.prop(6).as_bool().unwrap_or(true),
                        },
                        context: None,
                        sheet: Some((
                            match node.prop(3).as_str() {
                                Some("top") => Side::Top,
                                Some("left") => Side::Left,
                                Some("bottom") => Side::Bottom,
                                _ => Side::Right,
                            },
                            node.size(4).unwrap_or(Size::Md),
                        )),
                    }),
                content: ModalContent {
                    title: text(1).unwrap_or_default(),
                    message: text(2).unwrap_or_default(),
                    ok: None,
                    cancel: None,
                    width: None,
                    offset_top: None,
                    label: None,
                    close_button: false,
                    overlay: node.prop(7).as_bool().unwrap_or(true),
                    resizable: node.prop(8).as_bool().unwrap_or(true),
                },
                handlers: [None, None, node.handler(0)],
                error: node.handler(1),
            };
        }
        Self {
            key: node.state_id(),
            target: node
                .prop(0)
                .as_bool()
                .unwrap_or(false)
                .then(|| ModalTarget {
                    policy: Policy {
                        escape: node.prop(5).as_bool().unwrap_or(true),
                        outside: node.prop(6).as_bool().unwrap_or(true),
                    },
                    context: text(10),
                    sheet: None,
                }),
            content: ModalContent {
                title: text(1).unwrap_or_default(),
                message: text(2).unwrap_or_default(),
                ok: text(3),
                cancel: text(4),
                overlay: node.prop(7).as_bool().unwrap_or(true),
                close_button: node.prop(8).as_bool().unwrap_or(true),
                resizable: false,
                width: node.prop(9).as_f64().map(|width| width as f32),
                offset_top: node
                    .prop(11)
                    .as_f64()
                    .filter(|top| top.is_finite() && *top >= 0.)
                    .map(|top| top as f32),
                label: text(12),
            },
            handlers: std::array::from_fn(|index| node.handler(index)),
            error: node.handler(3),
        }
    }
}

fn modal_contents(mut modal: Modal, content: &ModalContent, latest: SharedModalContent) -> Modal {
    modal = modal
        .overlay(content.overlay)
        .close_button(content.close_button);
    if !content.title.is_empty() {
        modal = modal.title(content.title.clone());
    }
    if !content.message.is_empty() {
        let message = content.message.clone();
        modal = modal.content(move |content, _, _| content.child(message.clone()));
    }
    if let Some(width) = content.width {
        modal = modal.width(px(width));
    }
    if let Some(top) = content.offset_top {
        modal = modal.offset_top(px(top));
    }
    if let Some(label) = content.label.clone() {
        modal = modal.accessibility_label(label);
    }
    if content.ok.is_some() || content.cancel.is_some() {
        let mut footer = DialogFooter::new();
        if let Some(label) = content.cancel.clone() {
            footer =
                footer.child(DialogClose::new().trigger(move |button| button.label(label.clone())));
        }
        if let Some(label) = content.ok.clone() {
            footer = footer
                .child(DialogAction::new().child(Button::new("document-modal-ok").label(label)));
        }
        modal = modal.footer(footer);
    }
    let accept = latest.clone();
    let cancel = latest.clone();
    modal
        .on_ok(move |_, window, cx| {
            invoke(&accept, 0, window, cx);
            true
        })
        .on_cancel(move |_, window, cx| {
            invoke(&cancel, 1, window, cx);
            true
        })
        .on_close(move |_, window, cx| {
            invoke(&latest, 2, window, cx);
        })
}

fn invoke(content: &SharedModalContent, index: usize, window: &mut Window, cx: &mut App) {
    let handler = content.borrow().1[index].clone();
    if let Some(handler) = handler {
        handler(Value::Null, window, cx);
    }
}

impl RenderOnce for DocumentModal {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let window = window.window_handle();
            cx.on_release(move |state: &mut ModalState, cx| {
                if let Some(id) = state.opened {
                    cx.defer(move |cx| {
                        let _ =
                            window.update(cx, |_, window, cx| OverlayHost::close(window, cx, id));
                    });
                }
            })
            .detach();
            ModalState {
                requested: None,
                opened: None,
                content: Rc::new(RefCell::new((self.content.clone(), [None, None, None]))),
            }
        });
        if OverlayHost::entity(window, cx).is_ok() {
            let changed = state.update(cx, |state, _| {
                let mut content = state.content.borrow_mut();
                let changed = content.0 != self.content || state.requested != self.target;
                *content = (self.content, self.handlers);
                changed
            });
            if changed {
                window.defer(cx, move |window, cx| {
                    state.update(cx, |state, cx| {
                        if state.requested == self.target {
                            if let Some(id) = state.opened {
                                if state
                                    .requested
                                    .as_ref()
                                    .is_some_and(|target| target.sheet.is_some())
                                {
                                    window.refresh();
                                } else {
                                    OverlayHost::refresh_document_modal(window, cx, id);
                                }
                            }
                            return;
                        }
                        if let Some(id) = state.opened.take() {
                            let _ = OverlayHost::close(window, cx, id);
                        }
                        state.requested = self.target;
                        if let Some(target) = &state.requested {
                            let result = if let Some(context) = &target.context {
                                OverlayHost::open_context(window, cx, context, target.policy)
                            } else if let Some((side, size)) = target.sheet {
                                let content = state.content.clone();
                                let close = state.content.clone();
                                Drawer {
                                    side,
                                    size,
                                    policy: target.policy,
                                }
                                .open_with_close(
                                    window,
                                    cx,
                                    move |sheet, _, _| {
                                        let config = &content.borrow().0;
                                        sheet
                                            .title(config.title.clone())
                                            .child(config.message.clone())
                                            .overlay(config.overlay)
                                            .resizable(config.resizable)
                                    },
                                    move |window, cx| invoke(&close, 2, window, cx),
                                )
                            } else {
                                let content = state.content.clone();
                                OverlayHost::open(window, cx, target.policy, move |modal, _, _| {
                                    let config = &content.borrow().0;
                                    modal_contents(modal, config, content.clone())
                                })
                            };
                            match result {
                                Ok(id) => state.opened = Some(id),
                                Err(error) => {
                                    if let Some(handler) = self.error {
                                        handler(error.to_string().into(), window, cx);
                                    }
                                }
                            }
                        }
                    });
                });
            }
        }
        div().hidden()
    }
}

#[derive(IntoElement)]
struct DocumentTray {
    key: ElementId,
    toasts: Value,
    limit: Option<usize>,
    error: Option<EventHandler>,
    children: Vec<gpui_kit::AnyElement>,
}

#[derive(Default)]
struct TrayState {
    records: BTreeMap<SharedString, (ToastId, Value)>,
    limit: Option<usize>,
}

fn position(value: &Value) -> ToastPosition {
    match value.as_str() {
        Some("top-left") => ToastPosition::TopLeft,
        Some("top-center") => ToastPosition::TopCenter,
        Some("bottom-left") => ToastPosition::BottomLeft,
        Some("bottom-center") => ToastPosition::BottomCenter,
        Some("bottom-right") => ToastPosition::BottomRight,
        _ => ToastPosition::TopRight,
    }
}

impl RenderOnce for DocumentTray {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let window = window.window_handle();
            cx.on_release(move |state: &mut TrayState, cx| {
                let ids: Vec<_> = state.records.values().map(|(id, _)| *id).collect();
                if !ids.is_empty() {
                    cx.defer(move |cx| {
                        let _ = window.update(cx, |_, window, cx| {
                            for id in ids {
                                let _ = OverlayHost::dismiss_toast(window, cx, id);
                            }
                        });
                    });
                }
            })
            .detach();
            TrayState::default()
        });
        let records = &self.toasts;
        let before = state.read(cx);
        let id_field = ["id".into()];
        let mut desired = BTreeMap::new();
        for index in 0..records.list_len().unwrap_or_default() {
            if let Some(record) = records.list_get(index)
                && let Some(id) = record.field(&id_field).as_str()
            {
                desired.insert(SharedString::from(id.to_owned()), (index, record.clone()));
            }
        }
        let changed = desired.len() != before.records.len()
            || self.limit != before.limit
            || desired.iter().any(|(id, (_, record))| {
                before
                    .records
                    .get(id)
                    .is_none_or(|(_, previous)| previous != record)
            });
        if changed && OverlayHost::entity(window, cx).is_ok() {
            window.defer(cx, move |window, cx| {
                state.update(cx, |state, cx| {
                    let result = sync_tray(state, desired, self.limit, window, cx);
                    if let Err(error) = result
                        && let Some(handler) = self.error
                    {
                        handler(error.to_string().into(), window, cx);
                    }
                });
            });
        }
        div().children(self.children)
    }
}

fn sync_tray(
    state: &mut TrayState,
    desired: BTreeMap<SharedString, (usize, Value)>,
    limit: Option<usize>,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), crate::OverlayError> {
    if state.limit != limit {
        if let Some(limit) = limit {
            OverlayHost::toast_limit(window, cx, limit)?;
        }
        state.limit = limit;
    }
    let mut removed = Vec::new();
    state.records.retain(|id, (native, _)| {
        let keep = desired.contains_key(id);
        if !keep {
            removed.push(*native);
        }
        keep
    });
    for id in removed {
        OverlayHost::dismiss_toast(window, cx, id)?;
    }
    let mut desired: Vec<_> = desired.into_iter().collect();
    desired.sort_unstable_by_key(|(_, (index, _))| *index);
    for (id, (_, record)) in desired {
        if state
            .records
            .get(&id)
            .is_some_and(|(_, previous)| *previous == record)
        {
            continue;
        }
        let field = |name: &str| record.field(&[name.into()]);
        let native = match state.records.get(&id) {
            Some((native, _)) => *native,
            None => OverlayHost::allocate_document_toast(window, cx)?,
        };
        let mut toast = Toast::new(native, field("message").as_str().unwrap_or_default());
        toast.position = position(field("position"));
        if field("persistent").as_bool() == Some(true) {
            toast.auto_close = None;
        } else if let Some(duration) = field("durationMs").as_f64() {
            toast.auto_close = Some(Duration::from_millis(duration.clamp(0., 600_000.) as u64));
        }
        OverlayHost::notify(window, cx, toast)?;
        state.records.insert(id, (native, record));
    }
    Ok(())
}
