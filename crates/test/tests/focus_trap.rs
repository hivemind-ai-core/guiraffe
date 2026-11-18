//! The focus traps: a node that keeps the keyboard focus is reported, and a
//! screen without one is not.

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, FocusHandle, IntoElement, Render, Role, TestAppContext, TestSupportExt, Window, div,
    px,
};
use guiraffe_core::GuiraffeTheme;
use guiraffe_test::checks::{self, CheckReport, Severity};
use guiraffe_test::drive::Driver;
use guiraffe_test::inspect::{EventSink, NodeId};

/// A button of the screen.
fn button(id: &'static str, label: &'static str, focus: &FocusHandle) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .track_focus(focus)
        .focusable()
        .role(Role::Button)
        .aria_label(label)
        .w(px(120.))
        .h(px(40.))
        .child(label)
}

/// The screen under inspection: two buttons, and (when the screen traps the
/// focus) a node that takes the focus back after every frame.
struct Screen {
    trapped: bool,
    first_focus: FocusHandle,
    second_focus: FocusHandle,
}

impl Render for Screen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.trapped && !self.first_focus.is_focused(window) {
            // The trap takes the focus back, so the keyboard cannot leave it.
            self.first_focus.focus(window, cx);
        }
        div()
            .id("panel")
            .test_support()
            .child(button("first", "First", &self.first_focus))
            .child(button("second", "Second", &self.second_focus))
    }
}

fn report(cx: &mut TestAppContext, trapped: bool) -> CheckReport {
    let events = EventSink::new();
    let (_, vcx) = cx.add_window_view(move |_, cx| Screen {
        trapped,
        first_focus: cx.focus_handle().tab_stop(true),
        second_focus: cx.focus_handle().tab_stop(true),
    });
    vcx.run_until_parked();
    let mut driver = Driver::new(vcx, events);
    checks::run(&mut driver, &GuiraffeTheme::default())
}

#[gpui_kit::test]
fn a_node_that_keeps_the_focus_is_reported_as_a_trap(cx: &mut TestAppContext) {
    let report = report(cx, true);
    let findings = report.of("focus-trap");
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(
        findings[0].node.as_ref().map(|id| id.as_str()),
        Some("first")
    );
    assert_eq!(findings[0].severity, Severity::Error);
}

#[gpui_kit::test]
fn a_screen_without_a_trap_has_no_finding(cx: &mut TestAppContext) {
    let report = report(cx, false);
    assert!(
        report.of("focus-trap").is_empty(),
        "{:?}",
        report.of("focus-trap")
    );
}

#[gpui_kit::test]
fn the_button_behind_a_trap_is_unreachable(cx: &mut TestAppContext) {
    let report = report(cx, true);
    let findings = report.of("unreachable-focusable");
    assert!(
        findings
            .iter()
            .any(|finding| finding.node.as_ref() == Some(&NodeId::new("second"))),
        "{findings:?}"
    );
}
