use gpui_kit::TestAppContext;
use gpui_kit::component::theme::{Theme, ThemeMode};

use crate::*;

#[gpui_kit::test]
fn init_bridges_primary_into_controls(cx: &mut TestAppContext) {
    cx.update(|cx| {
        init(cx);
        let t = Theme::global(cx);
        assert_eq!(t.primary, Rgb::hex(0x228be6).to_hsla());
        assert_eq!(GuiraffeTheme::global(cx).primary, ColorName::Blue);
    });
}

#[gpui_kit::test]
fn scheme_switch_updates_both(cx: &mut TestAppContext) {
    cx.update(|cx| {
        init(cx);
        apply(
            GuiraffeTheme {
                primary: ColorName::Violet,
                ..Default::default()
            },
            cx,
        );
        set_scheme(Scheme::Dark, cx);
        assert_eq!(Theme::global(cx).mode, ThemeMode::Dark);
        assert_eq!(
            GuiraffeTheme::global(cx).primary,
            ColorName::Violet,
            "set_scheme keeps primary"
        );
        assert_eq!(
            Theme::global(cx).primary,
            ColorName::Violet.shade(8).to_hsla()
        );
    });
}

#[gpui_kit::test]
fn global_falls_back_without_init(cx: &mut TestAppContext) {
    cx.update(|cx| assert_eq!(*GuiraffeTheme::global(cx), GuiraffeTheme::default()));
}

mod style_context {
    use std::{cell::RefCell, rc::Rc};

    use gpui_kit::{
        AppContext, Context, Entity, IntoElement, ParentElement, Render, RenderOnce,
        TestAppContext, Window, div,
    };

    use crate::{Density, Emphasis, Size, StyleContext, StylePatch, provide};

    type Log = Rc<RefCell<Vec<(&'static str, StyleContext)>>>;

    #[derive(IntoElement)]
    struct Probe(&'static str, Log);

    impl RenderOnce for Probe {
        fn render(self, _: &mut Window, _: &mut gpui_kit::App) -> impl IntoElement {
            self.1.borrow_mut().push((self.0, StyleContext::current()));
            div()
        }
    }

    struct ChildView(Log);

    impl Render for ChildView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.0.borrow_mut().push(("view", StyleContext::current()));
            div()
        }
    }

    struct Root {
        log: Log,
        child: Entity<ChildView>,
    }

    impl Render for Root {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let log = self.log.clone();
            div()
                .child(Probe("outside", log.clone()))
                .child(provide(
                    StylePatch::new()
                        .control_size(Size::Xs)
                        .emphasis(Emphasis::Quiet),
                    div()
                        .child(Probe("toolbar", log.clone()))
                        .child(provide(
                            StylePatch::new().density(Density::Compact),
                            Probe("nested", log.clone()),
                        ))
                        .child(Probe("after-nested", log.clone()))
                        .child(self.child.clone()),
                ))
                .child(Probe("after", log))
        }
    }

    #[gpui_kit::test]
    fn context_reaches_components_nested_providers_and_views(cx: &mut TestAppContext) {
        let log: Log = Rc::default();
        let l2 = log.clone();
        let (_root, vcx) = cx.add_window_view(move |_, cx| {
            let child = cx.new(|_| ChildView(l2.clone()));
            Root { log: l2, child }
        });
        vcx.run_until_parked();
        let seen: Vec<(&str, Size, Density, Emphasis)> = log
            .borrow()
            .iter()
            .map(|(n, c)| (*n, c.control_size, c.density, c.emphasis))
            .collect();
        let get = |name: &str| seen.iter().find(|s| s.0 == name).copied();
        let d = StyleContext::default();
        assert_eq!(get("outside").map(|s| s.1), Some(d.control_size));
        assert_eq!(
            get("toolbar").map(|s| (s.1, s.3)),
            Some((Size::Xs, Emphasis::Quiet))
        );
        assert_eq!(
            get("nested").map(|s| (s.1, s.2)),
            Some((Size::Xs, Density::Compact)),
            "nested inherits and overrides"
        );
        assert_eq!(
            get("after-nested").map(|s| s.2),
            Some(Density::Comfortable),
            "restored after nested provider"
        );
        assert_eq!(
            get("view").map(|s| s.1),
            Some(Size::Xs),
            "child views inherit too"
        );
        assert_eq!(
            get("after").map(|s| s.1),
            Some(d.control_size),
            "restored after provider"
        );
        assert_eq!(StyleContext::current(), d, "stack empty after the frame");
    }
}
