//! Managed modal surfaces reuse Base focus/action handling and Controls content parts.
use crate::{OverlayHost, stack::OverlayId};
use gpui_kit::base::{Dialog as BaseDialog, TestSupportExt};
use gpui_kit::component::{
    StyledExt,
    dialog::{DialogClose, DialogContent, DialogTitle},
};
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, FocusHandle, InteractiveElement, IntoElement,
    ParentElement, Render, StyleRefinement, Styled, Window, div, prelude::FluentBuilder, px,
};
use guiraffe_core::{GuiraffeTheme, Size};
use guiraffe_motion::{
    MotionPrefs, Presence, Transition, TransitionKind, framed, sample_presence, set_presence,
};
use std::rc::Rc;
type Decision = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool>;
type Closed = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type Content = Rc<dyn Fn(DialogContent, &mut Window, &mut App) -> DialogContent>;
pub(crate) type Builder = Rc<dyn Fn(Modal, &mut Window, &mut App) -> Modal>;
/// Builder for independently identified managed dialogs. Controls' title/content/footer
/// components and Base's focus trap/actions are retained; storage is owned by GUIraffe.
pub struct Modal {
    title: Option<AnyElement>,
    content: Option<Content>,
    children: Vec<AnyElement>,
    footer: Option<AnyElement>,
    style: StyleRefinement,
    keyboard: bool,
    outside: bool,
    overlay: bool,
    close_button: bool,
    width: gpui_kit::Pixels,
    on_ok: Decision,
    on_cancel: Decision,
    on_close: Closed,
}
impl Modal {
    pub fn new(cx: &App) -> Self {
        let theme = GuiraffeTheme::global(cx);
        let surface = theme.surfaces();
        Self {
            title: None,
            content: None,
            children: Vec::new(),
            footer: None,
            style: StyleRefinement::default()
                .bg(surface.surface.to_hsla())
                .text_color(surface.text.to_hsla())
                .rounded(px(theme.default_radius.radius()))
                .p(px(Size::Md.spacing())),
            keyboard: true,
            outside: true,
            overlay: true,
            close_button: true,
            width: px(Size::Xs.container_width()),
            on_ok: Rc::new(|_, _, _| true),
            on_cancel: Rc::new(|_, _, _| true),
            on_close: Rc::new(|_, _, _| {}),
        }
    }
    pub fn title(mut self, title: impl IntoElement) -> Self {
        self.title = Some(title.into_any_element());
        self
    }
    pub fn content(
        mut self,
        build: impl Fn(DialogContent, &mut Window, &mut App) -> DialogContent + 'static,
    ) -> Self {
        self.content = Some(Rc::new(build));
        self
    }
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }
    pub fn keyboard(mut self, enabled: bool) -> Self {
        self.keyboard = enabled;
        self
    }
    pub fn overlay_closable(mut self, enabled: bool) -> Self {
        self.outside = enabled;
        self
    }
    pub fn overlay(mut self, enabled: bool) -> Self {
        self.overlay = enabled;
        self
    }
    pub fn close_button(mut self, enabled: bool) -> Self {
        self.close_button = enabled;
        self
    }
    pub fn width(mut self, width: impl Into<gpui_kit::Pixels>) -> Self {
        self.width = width.into();
        self
    }
    pub fn w(self, width: impl Into<gpui_kit::Pixels>) -> Self {
        self.width(width)
    }
    pub fn on_ok(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.on_ok = Rc::new(handler);
        self
    }
    pub fn on_cancel(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.on_cancel = Rc::new(handler);
        self
    }
    pub fn on_close(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_close = Rc::new(handler);
        self
    }
}
impl ParentElement for Modal {
    fn extend(&mut self, children: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(children);
    }
}
impl Styled for Modal {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
pub(crate) struct ManagedModal {
    pub focus: FocusHandle,
    id: OverlayId,
    host: gpui_kit::WeakEntity<OverlayHost>,
    build: Builder,
    presence: Presence,
    transition: Transition,
    closing: bool,
}
impl ManagedModal {
    pub fn new(
        id: OverlayId,
        host: gpui_kit::WeakEntity<OverlayHost>,
        build: Builder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let transition = Transition::themed(TransitionKind::Fade, &MotionPrefs::global(cx).tokens);
        let mut presence = Presence::new(false, transition.enter, transition.exit);
        set_presence(&mut presence, true, cx);
        Self {
            focus,
            id,
            host,
            build,
            presence,
            transition,
            closing: false,
        }
    }
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if !self.closing {
            self.closing = true;
            set_presence(&mut self.presence, false, cx);
            cx.notify();
        }
    }
}
impl Render for ManagedModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(visibility) = sample_presence(&mut self.presence, window, cx) else {
            let id = self.id;
            let host = self.host.clone();
            window.defer(cx, move |_, cx| {
                let _ = host.update(cx, |host, cx| host.release_modal(id, cx));
            });
            return div().into_any_element();
        };
        // Monotonic IDs keep exiting layers below subsequently opened dialogs.
        // Reusing the active-stack index would give an old exit and new enter the same priority.
        let layer = self.id.0.min((usize::MAX - 10) as u64) as usize;
        let topmost = self.host.upgrade().is_some_and(|host| {
            host.read(cx)
                .stack()
                .top()
                .is_some_and(|entry| entry.id == self.id)
        });
        let modal = (self.build)(Modal::new(cx), window, cx);
        let theme = GuiraffeTheme::global(cx);
        let surface = theme.surfaces();
        let gap = px(Size::Md.spacing());
        let viewport = window.viewport_size();
        let ok = modal.on_ok.clone();
        let cancel = modal.on_cancel.clone();
        let closed = modal.on_close.clone();
        let id = self.id;
        let host = self.host.clone();
        let body = div()
            .id(("managed-dialog-surface", id.0))
            .test_support()
            .relative()
            .occlude()
            .flex()
            .flex_col()
            .gap(gap)
            .w(modal.width.min((viewport.width - gap * 2.).max(px(0.))))
            .max_h((viewport.height - gap * 2.).max(px(0.)))
            .border_1()
            .border_color(surface.border.to_hsla())
            .refine_style(&modal.style)
            .children(modal.title.map(|title| DialogTitle::new().child(title)))
            .children(
                modal
                    .content
                    .map(|content| content(DialogContent::new(), window, cx)),
            )
            .children(modal.children)
            .children(modal.footer)
            .when(modal.close_button, |body| {
                body.child(
                    div()
                        .absolute()
                        .top(gap)
                        .right(gap)
                        .child(DialogClose::new().trigger(|button| button.label("×"))),
                )
            });
        let popup = framed(
            body,
            self.transition.kind.frame(visibility, Size::Md.spacing()),
        );
        let dialog = BaseDialog::new(cx)
            .open(true)
            .layer(layer, topmost && !self.closing)
            .focus_handle(self.focus.clone())
            .close_on_escape(modal.keyboard && topmost && !self.closing)
            .close_on_backdrop_press(modal.outside && !self.closing)
            .when(modal.overlay, |dialog| {
                dialog.backdrop(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(surface.text.alpha(0.2).to_hsla()),
                )
            })
            .on_ok(move |event, window, cx| ok(event, window, cx))
            .on_cancel(move |event, window, cx| cancel(event, window, cx))
            .on_close(move |event, window, cx| closed(event, window, cx))
            .request_close(move |confirmed, window, cx| {
                if let Some(host) = host.upgrade() {
                    OverlayHost::settle_entity(&host, window, cx, id, confirmed);
                }
            })
            .popup(popup);
        div()
            .absolute()
            .inset_0()
            .occlude()
            .child(dialog)
            .into_any_element()
    }
}
