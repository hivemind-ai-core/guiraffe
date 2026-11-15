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
