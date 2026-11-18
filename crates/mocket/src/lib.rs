//! Mocket → GUIraffe importer (spec/10 "Authoring: Mocket").
//!
//! Converts a Mocket document (its JSON: `screens` + a `nodes` map) into a GUIraffe
//! [`Document`]. Every node maps through one table ([`MAPPING`]); each import reports
//! **coverage** per node (mapped / partial / unsupported), like Mocket's own export targets.
//!
//! Design choices (spec/14, spec/10):
//! - Mocket pixel gaps become the nearest `Size` token, so imported screens stay on the app's
//!   spacing scale instead of carrying one-off pixels.
//! - Every text-bearing node binds its text to a slot named after the Mocket node id (with the
//!   designer's text as default), so compiled logic can drive any text by id.
//! - Buttons emit `<node-id>.click`.

use std::collections::{BTreeMap, HashMap};

use guiraffe_ui::{Document, Node};
use serde_json::Value as Json;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Coverage {
    /// Fully represented.
    Mapped,
    /// Represented with simplifications (e.g. a KPI tile without its sparkline).
    Partial,
    /// No equivalent yet; rendered as a visible placeholder.
    Unsupported,
}

/// Mocket component type → (GUIraffe kind, coverage). One table, one truth.
pub const MAPPING: &[(&str, &str, Coverage)] = &[
    ("frame", "stack", Coverage::Mapped),
    ("box", "stack", Coverage::Mapped),
    ("stack", "stack", Coverage::Mapped),
    ("row", "group", Coverage::Mapped),
    ("grid", "grid", Coverage::Mapped),
    ("card", "card", Coverage::Partial),
    ("heading", "title", Coverage::Mapped),
    ("text", "text", Coverage::Mapped),
    ("button", "button", Coverage::Mapped),
    ("spacer", "spacer", Coverage::Mapped),
    ("kpi", "card", Coverage::Partial),
    ("last-updated", "text", Coverage::Partial),
    ("progress", "ring", Coverage::Partial),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeReport {
    pub mocket_id: String,
    pub mocket_type: String,
    pub coverage: Coverage,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub screen: String,
    pub document: Document,
    pub report: Vec<NodeReport>,
}

impl Import {
    pub fn count(&self, c: Coverage) -> usize {
        self.report.iter().filter(|r| r.coverage == c).count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    Json(String),
    NoScreens,
    ScreenNotFound(String),
    MissingNode(String),
    Cycle(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Json(e) => write!(f, "invalid Mocket document: {e}"),
            ImportError::NoScreens => write!(f, "the document has no screens"),
            ImportError::ScreenNotFound(s) => write!(f, "no screen named or with id `{s}`"),
            ImportError::MissingNode(n) => write!(f, "node `{n}` is referenced but not defined"),
            ImportError::Cycle(n) => write!(f, "node `{n}` contains itself"),
        }
    }
}

impl std::error::Error for ImportError {}

/// Nearest spacing token for a pixel gap (Mantine scale: 10/12/16/20/32).
pub fn gap_token(px: f64) -> &'static str {
    const SCALE: [(f64, &str); 5] = [
        (10.0, "xs"),
        (12.0, "sm"),
        (16.0, "md"),
        (20.0, "lg"),
        (32.0, "xl"),
    ];
    SCALE
        .iter()
        .min_by(|a, b| (a.0 - px).abs().total_cmp(&(b.0 - px).abs()))
        .map(|s| s.1)
        .unwrap_or("sm")
}

/// List the screens in a Mocket document as `(id, name)`.
pub fn screens(mocket_json: &str) -> Result<Vec<(String, String)>, ImportError> {
    let doc: Json =
        serde_json::from_str(mocket_json).map_err(|e| ImportError::Json(e.to_string()))?;
    Ok(doc["screens"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|s| {
                    (
                        s["id"].as_str().unwrap_or_default().to_owned(),
                        s["name"].as_str().unwrap_or_default().to_owned(),
                    )
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Import one screen (by id or name; the first screen if `None`).
pub fn import(mocket_json: &str, screen: Option<&str>) -> Result<Import, ImportError> {
    let doc: Json =
        serde_json::from_str(mocket_json).map_err(|e| ImportError::Json(e.to_string()))?;
    let screens = doc["screens"].as_array().ok_or(ImportError::NoScreens)?;
    let s = match screen {
        None => screens.first().ok_or(ImportError::NoScreens)?,
        Some(want) => screens
            .iter()
            .find(|s| {
                s["id"].as_str() == Some(want)
                    || s["name"].as_str() == Some(want)
                    || s["uid"].as_str() == Some(want)
            })
            .ok_or_else(|| ImportError::ScreenNotFound(want.to_owned()))?,
    };
    let root_uid = s["root"].as_str().ok_or(ImportError::NoScreens)?;
    let nodes: HashMap<&str, &Json> = doc["nodes"]
        .as_object()
        .map(|m| m.iter().map(|(k, v)| (k.as_str(), v)).collect())
        .unwrap_or_default();
    let mut cx = Cx {
        nodes,
        report: Vec::new(),
        visiting: Vec::new(),
    };
    let root = cx.node(root_uid)?;
    Ok(Import {
        screen: s["name"].as_str().unwrap_or_default().to_owned(),
        document: Document::new(root),
        report: cx.report,
    })
}

struct Cx<'a> {
    nodes: HashMap<&'a str, &'a Json>,
    report: Vec<NodeReport>,
    visiting: Vec<&'a str>,
}

fn str_prop(n: &Json, key: &str) -> Option<String> {
    n["props"][key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

impl<'a> Cx<'a> {
    fn node(&mut self, uid: &'a str) -> Result<Node, ImportError> {
        if self.visiting.contains(&uid) {
            return Err(ImportError::Cycle(uid.to_owned()));
        }
        let n = *self
            .nodes
            .get(uid)
            .ok_or_else(|| ImportError::MissingNode(uid.to_owned()))?;
        self.visiting.push(uid);
        let ty = n["type"].as_str().unwrap_or("box");
        let id = n["id"].as_str().unwrap_or(uid).to_owned();
        let (kind, coverage) = MAPPING
            .iter()
            .find(|(m, _, _)| *m == ty)
            .map(|(_, k, c)| (*k, *c))
            .unwrap_or(("unsupported", Coverage::Unsupported));
        self.report.push(NodeReport {
            mocket_id: id.clone(),
            mocket_type: ty.to_owned(),
            coverage,
        });
        let mut out = Node {
            id: Some(id.clone()),
            ..Node::new(if coverage == Coverage::Unsupported {
                format!("mocket:{ty}")
            } else {
                kind.to_owned()
            })
        };
        let layout = &n["layout"];
        if matches!(kind, "stack" | "group" | "grid")
            && let Some(g) = layout["gap"].as_f64().filter(|g| *g > 0.0)
        {
            out = out.prop("gap", gap_token(g));
        }
        let text_slot = |out: Node, prop: &str, text: Option<String>, slot: String| match text {
            Some(t) => out.prop(prop, t).bind(prop, &slot),
            None => out,
        };
        out = match ty {
            "heading" => {
                let level = n["props"]["level"].as_f64().unwrap_or(2.0).clamp(1.0, 6.0);
                text_slot(
                    out.prop("order", level),
                    "text",
                    Some(str_prop(n, "text").unwrap_or_default()),
                    format!("{id}.text"),
                )
            }
            "text" | "last-updated" => {
                let o = text_slot(
                    out,
                    "text",
                    Some(str_prop(n, "text").unwrap_or_default()),
                    format!("{id}.text"),
                );
                if ty == "last-updated" || n["props"]["tone"].as_str() == Some("muted") {
                    o.prop("dimmed", true)
                } else {
                    o
                }
            }
            "button" => out
                .prop(
                    "label",
                    str_prop(n, "label").unwrap_or_else(|| "Button".into()),
                )
                .on("click", &format!("{id}.click")),
            "grid" => {
                let cols = layout["columns"]
                    .as_str()
                    .map(|c| c.split_whitespace().count())
                    .filter(|c| *c > 0)
                    .unwrap_or(2);
                out.prop("columns", cols.min(12) as f64)
            }
            _ => out,
        };
        // Card / KPI chrome becomes leading children (title, subtitle; label, value, delta).
        let mut lead: Vec<Node> = Vec::new();
        match ty {
            "card" => {
                if let Some(t) = str_prop(n, "title") {
                    lead.push(
                        Node {
                            id: Some(format!("{id}.title")),
                            ..Node::new("title").prop("order", 4.0)
                        }
                        .prop("text", t)
                        .bind("text", &format!("{id}.title")),
                    );
                }
                if let Some(t) = str_prop(n, "subtitle") {
                    lead.push(
                        Node::new("text")
                            .prop("text", t)
                            .prop("dimmed", true)
                            .prop("size", "sm"),
                    );
                }
            }
            "kpi" => {
                lead.push(
                    Node::new("text")
                        .prop("text", str_prop(n, "label").unwrap_or_default())
                        .prop("dimmed", true)
                        .prop("size", "sm"),
                );
                lead.push(
                    Node {
                        id: Some(format!("{id}.value")),
                        ..Node::new("title").prop("order", 2.0)
                    }
                    .prop("text", str_prop(n, "value").unwrap_or_default())
                    .bind("text", &format!("{id}.value")),
                );
                if let Some(d) = str_prop(n, "delta") {
                    let tone = if n["props"]["deltaTone"].as_str() == Some("negative") {
                        "red"
                    } else {
                        "teal"
                    };
                    lead.push(
                        Node::new("text")
                            .prop("text", d)
                            .prop("size", "sm")
                            .prop("color", tone),
                    );
                }
            }
            _ => {}
        }
        out.children = lead;
        if let Some(kids) = n["children"].as_array() {
            for k in kids {
                if let Some(kid) = k.as_str() {
                    out.children.push(self.node(kid)?);
                }
            }
        }
        self.visiting.pop();
        // Mocket sizing: `hug` = content-sized; anything else (`fill`, fixed) stretches.
        if layout["width"].as_str() == Some("hug") {
            return Ok(Node::new("hug").child(out));
        }
        Ok(out)
    }
}

/// `mocket.plugin/1` result: write `<screen>.guiraffe.json` per screen plus a coverage report.
pub fn plugin_run(request_json: &str) -> Json {
    let req: Json = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return serde_json::json!({ "ok": false, "message": format!("bad request: {e}") }),
    };
    if req["protocol"].as_str() != Some("mocket.plugin/1") {
        return serde_json::json!({ "ok": false, "message": "unsupported protocol (expected mocket.plugin/1)" });
    }
    let doc = req["document"].to_string();
    let list = match screens(&doc) {
        Ok(l) => l,
        Err(e) => return serde_json::json!({ "ok": false, "message": e.to_string() }),
    };
    let mut files = Vec::new();
    let mut summary = BTreeMap::new();
    for (id, name) in &list {
        match import(&doc, Some(id)) {
            Ok(imp) => {
                let file = format!("guiraffe/{}.guiraffe.json", sanitize(id));
                files.push(serde_json::json!({ "path": file, "text": serde_json::to_string_pretty(&imp.document).unwrap_or_default() }));
                summary.insert(
                    name.clone(),
                    (
                        imp.count(Coverage::Mapped),
                        imp.count(Coverage::Partial),
                        imp.count(Coverage::Unsupported),
                    ),
                );
            }
            Err(e) => {
                return serde_json::json!({ "ok": false, "message": format!("screen {name}: {e}") });
            }
        }
    }
    let message = summary
        .iter()
        .map(|(n, (m, p, u))| format!("{n}: {m} mapped, {p} partial, {u} unsupported"))
        .collect::<Vec<_>>()
        .join("; ");
    serde_json::json!({ "ok": true, "message": format!("GUIraffe export — {message}"), "files": files })
}

/// File-name-safe screen id (the plugin protocol refuses `..` and absolute paths anyway).
fn sanitize(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if out.is_empty() { "screen".into() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gap_tokens_snap_to_the_scale() {
        assert_eq!(gap_token(4.0), "xs");
        assert_eq!(gap_token(16.0), "md");
        assert_eq!(gap_token(24.0), "lg");
        assert_eq!(gap_token(48.0), "xl");
    }

    #[test]
    fn mapping_targets_are_registered_builtins() {
        let builtins = guiraffe_ui::builtins::KINDS;
        for (m, kind, _) in MAPPING {
            assert!(
                builtins.contains(kind),
                "{m} maps to unknown builtin {kind}"
            );
        }
        // Kinds the importer emits outside the table must exist too.
        for kind in ["hug", "title", "text"] {
            assert!(
                builtins.contains(&kind),
                "importer emits unregistered kind {kind}"
            );
        }
    }

    #[test]
    fn structural_errors_are_reported() {
        let cyc = r#"{"screens":[{"id":"s","name":"S","root":"a"}],"nodes":{"a":{"type":"stack","children":["b"]},"b":{"type":"stack","children":["a"]}}}"#;
        assert!(matches!(import(cyc, None), Err(ImportError::Cycle(_))));
        let missing = r#"{"screens":[{"id":"s","name":"S","root":"x"}],"nodes":{}}"#;
        assert!(matches!(
            import(missing, None),
            Err(ImportError::MissingNode(_))
        ));
        assert!(matches!(import("{}", None), Err(ImportError::NoScreens)));
        assert!(matches!(import("nope", None), Err(ImportError::Json(_))));
        let one =
            r#"{"screens":[{"id":"s","name":"S","root":"a"}],"nodes":{"a":{"type":"stack"}}}"#;
        assert!(matches!(
            import(one, Some("other")),
            Err(ImportError::ScreenNotFound(_))
        ));
    }

    #[test]
    fn plugin_protocol_round_trip_and_rejections() {
        let one = r#"{"screens":[{"id":"home","name":"Home","root":"a"}],"nodes":{"a":{"id":"root","type":"stack","children":["t"]},"t":{"id":"t","type":"text","props":{"text":"Hi"}}}}"#;
        let req =
            format!(r#"{{"protocol":"mocket.plugin/1","event":"export.after","document":{one}}}"#);
        let res = plugin_run(&req);
        assert_eq!(res["ok"], true, "{res}");
        assert_eq!(res["files"][0]["path"], "guiraffe/home.guiraffe.json");
        assert!(
            res["message"]
                .as_str()
                .unwrap_or_default()
                .contains("Home: 2 mapped")
        );
        assert_eq!(plugin_run(r#"{"protocol":"other"}"#)["ok"], false);
        assert_eq!(plugin_run("garbage")["ok"], false);
    }
}
