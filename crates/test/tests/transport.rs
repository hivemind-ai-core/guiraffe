//! The transport: the API over a local socket and as MCP tools.

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, FocusHandle, IntoElement, Render, Role, TestAppContext, TestSupportExt, Window, div,
};
use guiraffe_test::drive::{Action, Driver};
use guiraffe_test::inspect::{EventSink, NodeId};
use guiraffe_test::transport::{Call, Server, mcp, tools};

struct App {
    events: EventSink,
    focus: FocusHandle,
}

impl Render for App {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let events = self.events.clone();
        div().id("panel").test_support().child(
            div()
                .id("save")
                .test_support()
                .track_focus(&self.focus)
                .focusable()
                .role(Role::Button)
                .aria_label("Save")
                .on_click(move |_, _, _| {
                    events.emit("save.clicked", None);
                })
                .child("Save"),
        )
    }
}

#[gpui_kit::test]
fn a_call_answers_with_what_the_app_says(cx: &mut TestAppContext) {
    let events = EventSink::new();
    let host_events = events.clone();
    let (_, vcx) = cx.add_window_view(move |_, cx| App {
        events: host_events,
        focus: cx.focus_handle().tab_stop(true),
    });
    vcx.run_until_parked();
    let mut server = Server::new(Driver::new(vcx, events));

    let nodes = server.call(Call::Nodes);
    assert!(nodes.ok);
    let index = nodes.nodes.expect("the nodes");
    assert!(index.node(&NodeId::new("save")).is_some());

    let a11y = server.call(Call::A11y);
    assert!(a11y.ok);
    assert!(!a11y.a11y.expect("the a11y report").nodes.is_empty());

    let style = server.call(Call::Style);
    assert!(style.ok);
    assert!(style.style.expect("the style").radius > 0.0);
}

#[gpui_kit::test]
fn a_drive_call_answers_with_what_it_changed(cx: &mut TestAppContext) {
    let events = EventSink::new();
    let host_events = events.clone();
    let (_, vcx) = cx.add_window_view(move |_, cx| App {
        events: host_events,
        focus: cx.focus_handle().tab_stop(true),
    });
    vcx.run_until_parked();
    let mut server = Server::new(Driver::new(vcx, events));
    let answer = server.call(Call::Drive {
        action: Action::Click {
            id: NodeId::new("save"),
        },
    });
    assert!(answer.ok, "{answer:?}");
    let result = answer.drive.expect("the result");
    assert_eq!(result.events.len(), 1);
    assert_eq!(result.events[0].name, "save.clicked");
}

#[gpui_kit::test]
fn the_mcp_server_answers_the_handshake_and_lists_its_tools(cx: &mut TestAppContext) {
    let events = EventSink::new();
    let host_events = events.clone();
    let (_, vcx) = cx.add_window_view(move |_, cx| App {
        events: host_events,
        focus: cx.focus_handle().tab_stop(true),
    });
    vcx.run_until_parked();
    let mut server = Server::new(Driver::new(vcx, events));

    let handshake = mcp(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        &mut server,
    )
    .expect("the handshake is answered");
    assert!(handshake.contains("\"protocolVersion\""), "{handshake}");
    assert!(handshake.contains("\"serverInfo\""), "{handshake}");

    let list = mcp(
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        &mut server,
    )
    .expect("the tools are listed");
    let list: serde_json::Value = serde_json::from_str(&list).expect("valid JSON");
    let listed = list["result"]["tools"].as_array().expect("tool array");
    assert_eq!(listed.len(), tools().len());
    for (entry, tool) in listed.iter().zip(tools()) {
        assert_eq!(entry["name"], tool.name);
        assert_eq!(entry["inputSchema"], tool.input_schema);
        assert_eq!(entry["inputSchema"]["type"], "object");
    }
}

#[gpui_kit::test]
fn the_mcp_server_runs_a_tool(cx: &mut TestAppContext) {
    let events = EventSink::new();
    let host_events = events.clone();
    let (_, vcx) = cx.add_window_view(move |_, cx| App {
        events: host_events,
        focus: cx.focus_handle().tab_stop(true),
    });
    vcx.run_until_parked();
    let mut server = Server::new(Driver::new(vcx, events));
    let answer = mcp(
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"drive","arguments":{"action":{"action":"click","id":"save"}}}}"#,
        &mut server,
    )
    .expect("the call is answered");
    assert!(answer.contains("save.clicked"), "{answer}");
}

#[gpui_kit::test]
fn the_mcp_server_says_when_it_does_not_know_a_method(cx: &mut TestAppContext) {
    let events = EventSink::new();
    let host_events = events.clone();
    let (_, vcx) = cx.add_window_view(move |_, cx| App {
        events: host_events,
        focus: cx.focus_handle().tab_stop(true),
    });
    vcx.run_until_parked();
    let mut server = Server::new(Driver::new(vcx, events));
    let answer = mcp(
        r#"{"jsonrpc":"2.0","id":4,"method":"nonsense"}"#,
        &mut server,
    )
    .expect("the request is answered");
    assert!(answer.contains("\"error\""), "{answer}");
}

#[test]
fn the_tools_cover_the_api() {
    let names: Vec<String> = tools().into_iter().map(|tool| tool.name).collect();
    for expected in [
        "inspect_nodes",
        "inspect_layout",
        "inspect_a11y",
        "inspect_style",
        "inspect_events",
        "inspect_screenshot",
        "drive",
        "check",
        "replay",
        "query",
        "apply",
    ] {
        assert!(
            names.contains(&expected.to_string()),
            "{expected:?} is missing"
        );
    }
}

#[test]
fn request_reader_bounds_an_unterminated_line() {
    let mut oversized = std::io::repeat(b'x');
    let mut reader = std::io::BufReader::new(&mut oversized);
    let mut line = String::new();
    let error = guiraffe_test::transport::read_request(&mut reader, &mut line)
        .expect_err("oversized request");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(line.len(), 1024 * 1024 + 1);
}
