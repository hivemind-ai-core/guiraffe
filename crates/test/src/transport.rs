//! The transport (spec/21 §2): the inspect and drive API over a local socket
//! and as MCP tools.
//!
//! The socket speaks one JSON document per line: a call in, an answer out.
//! The MCP server speaks JSON-RPC 2.0 over stdio and serves the same calls
//! as tools.

use std::io::{BufRead, Read};
#[cfg(unix)]
use std::io::{BufReader, Write};
use std::path::Path;

use guiraffe_core::GuiraffeTheme;
use serde::{Deserialize, Serialize};

use crate::checks::CheckReport;
use crate::drive::{Action, DriveResult, Driver};
use crate::inspect::{
    A11yReport, InspectTarget, LayoutReport, NodeId, NodeIndex, RecordedEvent, StyleReport,
};
use crate::replay::{ReplayResult, Script};

/// A call of the API.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum Call {
    /// The nodes of the last frame.
    Nodes,
    /// The layout of a node, or of every node.
    Layout {
        #[serde(default)]
        id: Option<NodeId>,
    },
    /// The accessibility facts and the Tab order.
    A11y,
    /// The resolved tokens of the app.
    Style,
    /// The events the app emitted.
    Events,
    /// A screenshot of the window, or of a node, as a PNG.
    Screenshot {
        #[serde(default)]
        id: Option<NodeId>,
    },
    /// An action on the app.
    Drive { action: Action },
    /// The checks of the screen.
    Check,
    /// A recorded script, run again.
    Replay { script: Script },
}

/// The answer of a call.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    /// Whether the call was answered.
    pub ok: bool,
    /// Why it was not, when it was not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The nodes of the last frame.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<NodeIndex>,
    /// The layout of the nodes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<LayoutReport>,
    /// The accessibility facts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a11y: Option<A11yReport>,
    /// The resolved tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleReport>,
    /// The events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<RecordedEvent>>,
    /// A screenshot, as the hexadecimal bytes of a PNG.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
    /// What an action changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drive: Option<DriveResult>,
    /// The findings of the checks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checks: Option<CheckReport>,
    /// What a run of a script did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay: Option<ReplayResult>,
}

/// The server: answers the calls of the API.
pub struct Server<'a, T> {
    driver: Driver<'a, T>,
    theme: GuiraffeTheme,
}

impl<'a, T: InspectTarget> Server<'a, T> {
    /// A server over a running app.
    pub fn new(driver: Driver<'a, T>) -> Self {
        Self {
            driver,
            theme: GuiraffeTheme::default(),
        }
    }

    /// The driver of the app, for the calls that drive it.
    pub fn driver_mut(&mut self) -> &mut Driver<'a, T> {
        &mut self.driver
    }

    /// Answers a call.
    pub fn call(&mut self, call: Call) -> Answer {
        match call {
            Call::Nodes => Answer {
                ok: true,
                nodes: Some(self.driver.nodes()),
                ..Default::default()
            },
            Call::Layout { id } => Answer {
                ok: true,
                layout: Some(self.driver.layout(id.as_ref())),
                ..Default::default()
            },
            Call::A11y => Answer {
                ok: true,
                a11y: Some(self.driver.a11y()),
                ..Default::default()
            },
            Call::Style => Answer {
                ok: true,
                style: Some(self.driver.style()),
                ..Default::default()
            },
            Call::Events => Answer {
                ok: true,
                events: Some(self.driver.event_log()),
                ..Default::default()
            },
            Call::Screenshot { id } => match self.driver.screenshot(id.as_ref()) {
                Some(png) => Answer {
                    ok: true,
                    screenshot: Some(hex(&png)),
                    ..Default::default()
                },
                None => Answer {
                    ok: false,
                    error: Some("the platform cannot render the window".into()),
                    ..Default::default()
                },
            },
            Call::Drive { action } => {
                let result = self.driver.drive(&action);
                Answer {
                    ok: result.ok,
                    error: result.error.clone(),
                    drive: Some(result),
                    ..Default::default()
                }
            }
            Call::Check => {
                let report = crate::checks::run(&mut self.driver, &self.theme);
                Answer {
                    ok: true,
                    checks: Some(report),
                    ..Default::default()
                }
            }
            Call::Replay { script } => {
                let result = crate::replay::run(&mut self.driver, &script);
                Answer {
                    ok: result.ok(),
                    replay: Some(result),
                    ..Default::default()
                }
            }
        }
    }
}

/// The hexadecimal bytes of a PNG.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// A tool of the MCP server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    /// The name of the tool.
    pub name: String,
    /// What the tool does.
    pub description: String,
    /// The JSON schema of the arguments.
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// The tools the MCP server serves.
pub fn tools() -> Vec<Tool> {
    let empty = serde_json::json!({ "type": "object", "properties": {} });
    vec![
        Tool {
            name: "inspect_nodes".into(),
            description: "The nodes of the running app, with their bounds and their accessibility facts.".into(),
            input_schema: empty.clone(),
        },
        Tool {
            name: "inspect_layout".into(),
            description: "The layout of a node (or of every node): bounds, content against the box, overflow, clipping and truncation.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "id": { "type": "string" } }
            }),
        },
        Tool {
            name: "inspect_a11y".into(),
            description: "The accessibility facts of the running app and the order the Tab key visits the nodes in.".into(),
            input_schema: empty.clone(),
        },
        Tool {
            name: "inspect_style".into(),
            description: "The resolved tokens of the app and the context they came from.".into(),
            input_schema: empty.clone(),
        },
        Tool {
            name: "inspect_events".into(),
            description: "The events the app emitted, oldest first.".into(),
            input_schema: empty.clone(),
        },
        Tool {
            name: "inspect_screenshot".into(),
            description: "A PNG of the window or of a node, as hexadecimal bytes.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "id": { "type": "string" } }
            }),
        },
        Tool {
            name: "drive".into(),
            description: "Performs an action on the app (click, hover, type, key, focus, scroll, drag, resize, set_theme, set_fixture) and answers what it changed.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "action": { "type": "object" } },
                "required": ["action"]
            }),
        },
        Tool {
            name: "check".into(),
            description: "The findings of the checks of the screen (clipped text, small targets, contrast, unlabeled nodes, focus traps, tab order, layout jumps).".into(),
            input_schema: empty.clone(),
        },
        Tool {
            name: "replay".into(),
            description: "Runs a recorded interaction script again and answers with the failures of its assertions.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "script": { "type": "object" } },
                "required": ["script"]
            }),
        },
    ]
}

/// Answers one JSON-RPC request of the MCP server.
///
/// The answer is the JSON-RPC document, or `None` when the request is a
/// notification (the client does not wait for an answer then).
pub fn mcp<T: InspectTarget>(request: &str, server: &mut Server<'_, T>) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(request).ok()?;
    let id = value.get("id").cloned();
    let method = value.get("method").and_then(|method| method.as_str())?;
    let params = value
        .get("params")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let result = match method {
        "initialize" => Ok(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "guiraffe", "version": env!("CARGO_PKG_VERSION") },
        })),
        "tools/list" => Ok(serde_json::json!({ "tools": tools() })),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(|name| name.as_str())
                .unwrap_or("");
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::json!({}));
            match call_of(name, arguments) {
                Some(call) => {
                    let answer = server.call(call);
                    let text = serde_json::to_string(&answer).unwrap_or_default();
                    Ok(serde_json::json!({
                        "content": [{ "type": "text", "text": text }],
                        "isError": !answer.ok,
                    }))
                }
                None => Err(format!("there is no tool {name:?}")),
            }
        }
        _ => Err(format!("there is no method {method:?}")),
    };
    let id = id?;
    let document = match result {
        Ok(result) => serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": error },
        }),
    };
    serde_json::to_string(&document).ok()
}

/// The call of a tool, from its arguments.
fn call_of(name: &str, arguments: serde_json::Value) -> Option<Call> {
    let call = match name {
        "inspect_nodes" => Call::Nodes,
        "inspect_layout" => Call::Layout {
            id: arguments
                .get("id")
                .and_then(|id| id.as_str())
                .map(NodeId::new),
        },
        "inspect_a11y" => Call::A11y,
        "inspect_style" => Call::Style,
        "inspect_events" => Call::Events,
        "inspect_screenshot" => Call::Screenshot {
            id: arguments
                .get("id")
                .and_then(|id| id.as_str())
                .map(NodeId::new),
        },
        "drive" => Call::Drive {
            action: serde_json::from_value(arguments.get("action")?.clone()).ok()?,
        },
        "check" => Call::Check,
        "replay" => Call::Replay {
            script: serde_json::from_value(arguments.get("script")?.clone()).ok()?,
        },
        _ => return None,
    };
    Some(call)
}

/// Reads one request, with a one-MiB limit including the line delimiter.
pub fn read_request(reader: &mut impl BufRead, line: &mut String) -> std::io::Result<bool> {
    const LIMIT: usize = 1024 * 1024;
    line.clear();
    let bytes = reader.take((LIMIT + 1) as u64).read_line(line)?;
    if bytes > LIMIT {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "request exceeds one MiB",
        ));
    }
    Ok(bytes != 0)
}

/// Serves the API on a local socket, one JSON document per line.
///
/// The socket is a Unix socket, so the API is only reachable from the
/// machine it runs on. The server takes at most [`MAX_CLIENTS`] clients at a
/// time, every read and write has a timeout, and a request or an answer
/// larger than a mebibyte is refused.
#[cfg(unix)]
pub fn serve<T: InspectTarget>(path: &Path, server: &mut Server<'_, T>) -> std::io::Result<()> {
    // A socket left over from a run that died would keep the path busy.
    let _ = std::fs::remove_file(path);
    let listener = std::os::unix::net::UnixListener::bind(path)?;
    let mut clients = 0usize;
    for stream in listener.incoming() {
        if clients >= MAX_CLIENTS {
            // Too many clients: the connection is closed again right away.
            let _ = stream?;
            continue;
        }
        clients += 1;
        let outcome = serve_client(stream?, server);
        clients -= 1;
        outcome?;
    }
    Ok(())
}

/// How many clients the socket serves at the same time.
pub const MAX_CLIENTS: usize = 8;

/// How long a client may take to send a request or to read an answer.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// How large a request or an answer may be.
const LIMIT: usize = 1024 * 1024;

#[cfg(unix)]
fn serve_client<T: InspectTarget>(
    mut stream: std::os::unix::net::UnixStream,
    server: &mut Server<'_, T>,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    while read_request(&mut reader, &mut line)? {
        if line.trim().is_empty() {
            continue;
        }
        let answer = match serde_json::from_str::<Call>(&line) {
            Ok(call) => server.call(call),
            Err(error) => Answer {
                ok: false,
                error: Some(format!("the call does not parse: {error}")),
                ..Default::default()
            },
        };
        let text = serde_json::to_string(&answer)?;
        if text.len() > LIMIT {
            let answer = Answer {
                ok: false,
                error: Some("the answer is too large".into()),
                ..Default::default()
            };
            serde_json::to_writer(&mut stream, &answer)?;
            writeln!(stream)?;
            continue;
        }
        writeln!(stream, "{text}")?;
        stream.flush()?;
    }
    Ok(())
}

#[cfg(not(unix))]
/// Reports that the local socket transport requires Unix sockets.
pub fn serve<T: InspectTarget>(_: &Path, _: &mut Server<'_, T>) -> std::io::Result<()> {
    Err(std::io::ErrorKind::Unsupported.into())
}
