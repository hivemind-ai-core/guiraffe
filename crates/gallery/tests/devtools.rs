#![cfg(feature = "devtools")]
use gpui_kit::{TestAppContext, px, size};
use guiraffe_gallery::{Page, shell::Gallery};
#[gpui_kit::test]
fn gallery_toolbar_and_shortcut_toggle_devtools(cx: &mut TestAppContext) {
    cx.update(guiraffe_gallery::init);
    let (gallery, vcx) = cx.add_window_view(|window, cx| {
        window.resize(size(px(1000.), px(850.)));
        let gallery = Gallery::new(Page::Overview, cx);
        gallery.devtools().focus(window, cx);
        gallery
    });
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("devtools-panel").is_none());
    let button = vcx
        .debug_bounds("gallery-devtools")
        .unwrap_or_else(|| panic!("Devtools toggle"))
        .center();
    vcx.simulate_click(button, gpui_kit::Modifiers::default());
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("devtools-panel").is_some());
    vcx.simulate_keystrokes("ctrl-shift-i");
    vcx.run_until_parked();
    assert!(!vcx.update(|_, cx| gallery.read(cx).devtools().is_open()));
}
