//! The checks: the findings an agent gets about a screen.

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, FocusHandle, IntoElement, Render, Role, TestAppContext, TestSupportExt, Window, div,
    px,
};
use guiraffe_core::GuiraffeTheme;
use guiraffe_test::checks::{self, CheckReport, Finding, Severity};
use guiraffe_test::drive::Driver;
use guiraffe_test::inspect::EventSink;

/// The app under inspection: the defective one or the clean one.
struct App {
    defective: bool,
    tiny_focus: FocusHandle,
    first_focus: FocusHandle,
    second_focus: FocusHandle,
}

impl Render for App {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut panel = div().id("panel").test_support();
        if self.defective {
            panel = panel
                // A target a pointer cannot hit.
                .child(
                    div()
                        .id("tiny")
                        .test_support()
                        .track_focus(&self.tiny_focus)
                        .focusable()
                        .role(Role::Button)
                        .aria_label("Tiny")
                        .size(px(10.))
                        .child("x"),
                )
                // A button with no name.
                .child(
                    div()
                        .id("unnamed")
                        .test_support()
                        .role(Role::Button)
                        .w(px(120.))
                        .h(px(40.))
                        .child("?"),
                )
                // Text that does not fit its box.
                .child(
                    div()
                        .id("clipped")
                        .test_support()
                        .w(px(40.))
                        .h(px(20.))
                        .overflow_hidden()
                        .child("A very long sentence that cannot fit"),
                );
        } else {
            panel = panel
                .child(
                    div()
                        .id("first")
                        .test_support()
                        .track_focus(&self.first_focus)
                        .focusable()
                        .role(Role::Button)
                        .aria_label("First")
                        .size(px(120.))
                        .h(px(40.))
                        .child("First"),
                )
                .child(
                    div()
                        .id("second")
                        .test_support()
                        .track_focus(&self.second_focus)
                        .focusable()
                        .role(Role::Button)
                        .aria_label("Second")
                        .size(px(120.))
                        .h(px(40.))
                        .child("Second"),
                );
        }
        panel
    }
}

fn report(cx: &mut TestAppContext, defective: bool) -> CheckReport {
    let events = EventSink::new();
    let (_, vcx) = cx.add_window_view(move |_, cx| App {
        defective,
        tiny_focus: cx.focus_handle().tab_stop(true),
        first_focus: cx.focus_handle().tab_stop(true),
        second_focus: cx.focus_handle().tab_stop(true),
    });
    vcx.run_until_parked();
    let mut driver = Driver::new(vcx, events);
    checks::run(&mut driver, &GuiraffeTheme::default())
}

#[gpui_kit::test]
fn a_target_that_is_too_small_is_reported(cx: &mut TestAppContext) {
    let report = report(cx, true);
    let findings = report.of("hit-target");
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(
        findings[0].node.as_ref().map(|id| id.as_str()),
        Some("tiny")
    );
    assert_eq!(findings[0].severity, Severity::Error);
}

#[gpui_kit::test]
fn an_interactive_node_without_a_name_is_reported(cx: &mut TestAppContext) {
    let report = report(cx, true);
    let findings = report.of("unlabeled");
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(
        findings[0].node.as_ref().map(|id| id.as_str()),
        Some("unnamed")
    );
}

#[gpui_kit::test]
fn a_clean_screen_has_no_findings(cx: &mut TestAppContext) {
    let report = report(cx, false);
    assert!(report.findings.is_empty(), "{:?}", report.findings);
}

#[test]
fn the_findings_are_grouped_by_check() {
    let report = CheckReport {
        findings: vec![
            Finding {
                check: "hit-target".into(),
                node: None,
                severity: Severity::Error,
                message: "too small".into(),
            },
            Finding {
                check: "unlabeled".into(),
                node: None,
                severity: Severity::Warning,
                message: "no name".into(),
            },
        ],
    };
    assert_eq!(report.of("hit-target").len(), 1);
    assert_eq!(report.of("unlabeled").len(), 1);
    assert!(report.has_errors());
}
