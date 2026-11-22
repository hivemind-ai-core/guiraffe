//! Context-driven styling: the same widget adapts to the layout it is in.

use gpui_kit::{
    Context, IntoElement, ParentElement, Render, Styled, TestAppContext, Window, assets::IconName,
    div, prelude::*,
};
use guiraffe_components::{ActionIcon, icons::icon_box};
use guiraffe_core::{GuiraffeTheme, Size, StyleContext, provide};

struct Scene;

impl Render for Scene {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let toolbar = GuiraffeTheme::global(cx).recipes.toolbar;
        div()
            .flex()
            .flex_col()
            .items_start()
            .child(
                div()
                    .debug_selector(|| "plain".into())
                    .child(ActionIcon::new("a", IconName::Star)),
            )
            .child(provide(
                toolbar,
                div()
                    .debug_selector(|| "toolbar".into())
                    .child(ActionIcon::new("b", IconName::Star)),
            ))
            .child(provide(
                toolbar,
                div()
                    .debug_selector(|| "explicit".into())
                    .child(ActionIcon::new("c", IconName::Star).size(Size::Lg)),
            ))
    }
}

#[gpui_kit::test]
fn widgets_inherit_size_from_their_layout_and_explicit_props_win(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let (_v, vcx) = cx.add_window_view(|_, _| Scene);
    vcx.run_until_parked();
    let mut w = |sel: &'static str| {
        vcx.debug_bounds(sel)
            .map(|b| f32::from(b.size.width))
            .unwrap_or(-1.0)
    };
    let default = StyleContext::default().control_size;
    assert_eq!(
        w("plain"),
        icon_box(default),
        "outside any layout: default control size"
    );
    assert_eq!(
        w("toolbar"),
        icon_box(Size::Xs),
        "toolbar recipe makes it compact"
    );
    assert_eq!(
        w("explicit"),
        icon_box(Size::Lg),
        "explicit prop is an escape hatch"
    );
}

/// A view inside a provider, re-rendered on its own (gpui-fast does not re-run the
/// provider's view), keeps the provided size.
struct Bar {
    renders: usize,
}

impl Render for Bar {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders += 1;
        div()
            .debug_selector(|| "bar".into())
            .child(ActionIcon::new("d", IconName::Star))
    }
}

struct Host {
    renders: usize,
    bar: gpui_kit::Entity<Bar>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders += 1;
        let toolbar = GuiraffeTheme::global(cx).recipes.toolbar;
        div()
            .flex()
            .items_start()
            .child(provide(toolbar, div().child(self.bar.clone())))
    }
}

#[gpui_kit::test]
fn isolated_rerender_keeps_the_provided_size(cx: &mut TestAppContext) {
    cx.update(guiraffe_components::init);
    let (host, vcx) = cx.add_window_view(|_, cx| Host {
        renders: 0,
        bar: cx.new(|_| Bar { renders: 0 }),
    });
    vcx.run_until_parked();
    let bar = host.read_with(vcx, |h, _| h.bar.clone());
    let width = |vcx: &mut gpui_kit::VisualTestContext| {
        vcx.debug_bounds("bar")
            .map(|b| f32::from(b.size.width))
            .unwrap_or(-1.0)
    };
    assert_eq!(width(vcx), icon_box(Size::Xs));
    let hosts = host.read_with(vcx, |h, _| h.renders);
    for _ in 0..2 {
        vcx.update(|_, cx| bar.update(cx, |_, cx| cx.notify()));
        vcx.run_until_parked();
    }
    assert_eq!(
        bar.read_with(vcx, |b, _| b.renders),
        3,
        "the bar re-renders"
    );
    assert_eq!(width(vcx), icon_box(Size::Xs), "still the toolbar size");
    if gpui_kit::GPUI_FAST {
        assert_eq!(
            host.read_with(vcx, |h, _| h.renders),
            hosts,
            "the host never re-renders for the bar"
        );
    }
}
