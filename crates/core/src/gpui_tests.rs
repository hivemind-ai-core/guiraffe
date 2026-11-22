use gpui_kit::TestAppContext;
use gpui_kit::component::theme::{Theme, ThemeMode};

use crate::*;

#[gpui_kit::test]
fn init_bridges_primary_into_controls(cx: &mut TestAppContext) {
    cx.update(|cx| {
        init(cx);
        let t = Theme::global(cx);
        let g = GuiraffeTheme::global(cx);
        assert_eq!(
            t.primary,
            g.variant(None, Variant::Filled).background.to_hsla()
        );
        assert_ne!(
            t.button_primary_active, t.button_primary_hover,
            "pressed differs from hover"
        );
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
fn bridge_reaches_derived_kit_fields_in_both_schemes(cx: &mut TestAppContext) {
    cx.update(|cx| {
        init(cx);
        for scheme in [Scheme::Light, Scheme::Dark] {
            let g = GuiraffeTheme {
                scheme,
                primary: ColorName::Teal,
                ..Default::default()
            };
            apply(g.clone(), cx);
            let k = KitColors::from_theme(&g);
            let t = Theme::global(cx);
            assert_eq!(t.mode.is_dark(), scheme == Scheme::Dark);
            assert_eq!(t.button_primary, k.primary.to_hsla(), "{scheme:?}");
            assert_eq!(t.tokens.button_primary.color, k.primary.to_hsla());
            assert_eq!(t.slider_bar, k.primary.to_hsla());
            assert_eq!(t.progress_bar, k.primary.to_hsla());
            assert_eq!(t.background, g.surfaces().body.to_hsla());
            assert_eq!(t.chart_2, k.primary.to_hsla());
        }
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

/// A view re-rendered on its own (gpui-fast draws a notified view again without its
/// ancestors) still reads the context its ancestors' providers gave it.
mod isolated_render {
    use std::{cell::RefCell, rc::Rc};

    use gpui_kit::{
        AppContext, Context, Entity, IntoElement, ParentElement, Render, TestAppContext,
        VisualTestContext, Window, div,
    };

    use crate::{Density, Size, StyleContext, StylePatch, SurfaceKind, focus_scope, provide};

    type Log = Rc<RefCell<Vec<StyleContext>>>;

    struct Leaf(Log);

    impl Render for Leaf {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.0.borrow_mut().push(StyleContext::current());
            div()
        }
    }

    /// A view between the root's providers and the leaf, with a provider of its own.
    struct Middle {
        log: Log,
        leaf: Entity<Leaf>,
    }

    impl Render for Middle {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.log.borrow_mut().push(StyleContext::current());
            div().child(provide(
                StylePatch::new().surface(SurfaceKind::Sunken),
                div().child(self.leaf.clone()),
            ))
        }
    }

    struct Root {
        renders: Rc<RefCell<usize>>,
        patch: StylePatch,
        middle: Entity<Middle>,
    }

    impl Render for Root {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            *self.renders.borrow_mut() += 1;
            div().child(focus_scope(
                div().child(provide(self.patch, div().child(self.middle.clone()))),
            ))
        }
    }

    struct Scene {
        root: Entity<Root>,
        middle: Entity<Middle>,
        leaf: Entity<Leaf>,
        root_renders: Rc<RefCell<usize>>,
        middle_log: Log,
        leaf_log: Log,
    }

    fn scene(cx: &mut TestAppContext) -> (Scene, &mut VisualTestContext) {
        let middle_log: Log = Rc::default();
        let leaf_log: Log = Rc::default();
        let root_renders = Rc::new(RefCell::new(0));
        let (ml, ll, rr) = (middle_log.clone(), leaf_log.clone(), root_renders.clone());
        let mut handles = None;
        let (root, vcx) = cx.add_window_view(|_, cx| {
            let leaf = cx.new(|_| Leaf(ll));
            let middle = cx.new(|_| Middle {
                log: ml,
                leaf: leaf.clone(),
            });
            handles = Some((middle.clone(), leaf));
            Root {
                renders: rr,
                patch: StylePatch::new()
                    .control_size(Size::Lg)
                    .density(Density::Compact),
                middle,
            }
        });
        vcx.run_until_parked();
        let Some((middle, leaf)) = handles else {
            unreachable!("the window builds its root view")
        };
        (
            Scene {
                root,
                middle,
                leaf,
                root_renders,
                middle_log,
                leaf_log,
            },
            vcx,
        )
    }

    fn provided(surface: SurfaceKind) -> StyleContext {
        StyleContext {
            density: Density::Compact,
            control_size: Size::Lg,
            surface,
            keyboard_nav: true,
            ..StyleContext::default()
        }
    }

    fn last(log: &Log) -> Option<StyleContext> {
        log.borrow().last().copied()
    }

    #[gpui_kit::test]
    fn notified_leaf_keeps_nested_providers(cx: &mut TestAppContext) {
        let (s, vcx) = scene(cx);
        assert_eq!(last(&s.leaf_log), Some(provided(SurfaceKind::Sunken)));
        let (roots, leaves) = (*s.root_renders.borrow(), s.leaf_log.borrow().len());
        for _ in 0..3 {
            vcx.update(|_, cx| s.leaf.update(cx, |_, cx| cx.notify()));
            vcx.run_until_parked();
        }
        assert!(s.leaf_log.borrow().len() > leaves, "the leaf re-renders");
        assert!(
            s.leaf_log
                .borrow()
                .iter()
                .all(|c| *c == provided(SurfaceKind::Sunken)),
            "every render of the leaf sees both providers: {:?}",
            s.leaf_log.borrow()
        );
        if gpui_kit::GPUI_FAST {
            assert_eq!(
                *s.root_renders.borrow(),
                roots,
                "the root never re-renders for the leaf"
            );
        }
        assert_eq!(StyleContext::current(), StyleContext::default());
    }

    #[gpui_kit::test]
    fn notified_middle_view_keeps_root_providers(cx: &mut TestAppContext) {
        let (s, vcx) = scene(cx);
        let roots = *s.root_renders.borrow();
        vcx.update(|_, cx| s.middle.update(cx, |_, cx| cx.notify()));
        vcx.run_until_parked();
        vcx.update(|_, cx| s.leaf.update(cx, |_, cx| cx.notify()));
        vcx.run_until_parked();
        assert!(s.middle_log.borrow().len() >= 2, "the middle re-renders");
        assert!(
            s.middle_log
                .borrow()
                .iter()
                .all(|c| *c == provided(SurfaceKind::Body)),
            "{:?}",
            s.middle_log.borrow()
        );
        assert_eq!(last(&s.leaf_log), Some(provided(SurfaceKind::Sunken)));
        if gpui_kit::GPUI_FAST {
            assert_eq!(*s.root_renders.borrow(), roots, "isolation holds");
        }
    }

    #[gpui_kit::test]
    fn a_changed_provider_reaches_retained_views(cx: &mut TestAppContext) {
        let (s, vcx) = scene(cx);
        vcx.update(|_, cx| {
            s.root.update(cx, |r, cx| {
                r.patch = r.patch.density(Density::Spacious);
                cx.notify();
            })
        });
        vcx.run_until_parked();
        let expect = |surface| StyleContext {
            density: Density::Spacious,
            ..provided(surface)
        };
        assert_eq!(last(&s.middle_log), Some(expect(SurfaceKind::Body)));
        assert_eq!(last(&s.leaf_log), Some(expect(SurfaceKind::Sunken)));
        // The new context is the one kept for the next isolated render.
        vcx.update(|_, cx| s.leaf.update(cx, |_, cx| cx.notify()));
        vcx.run_until_parked();
        assert_eq!(last(&s.leaf_log), Some(expect(SurfaceKind::Sunken)));
    }
}
