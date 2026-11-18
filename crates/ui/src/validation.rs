//! Authoring-time validation for the built-in catalogue.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    Document, Node,
    catalog::{KINDS, lookup},
};

#[derive(Debug, Serialize)]
pub struct Issue {
    pub node: String,
    pub property: String,
    pub got: Value,
    pub accepted: Value,
    pub hint: String,
}

#[derive(Default)]
pub struct Validation<'a> {
    /// Event targets available in the host. Omit when the document is being handed to a host later.
    pub bound_events: Option<&'a BTreeSet<String>>,
    /// Mocket imports preserve unsupported nodes so the preview can show a placeholder.
    pub mocket_placeholders: bool,
}

fn suggestion(got: &str, choices: &[&str]) -> String {
    fn distance(a: &str, b: &str) -> usize {
        let mut row: Vec<_> = (0..=b.chars().count()).collect();
        for (i, a) in a.chars().enumerate() {
            let mut diagonal = row[0];
            row[0] = i + 1;
            for (j, b) in b.chars().enumerate() {
                let original = row[j + 1];
                row[j + 1] = (diagonal + usize::from(a != b)).min(original.min(row[j]) + 1);
                diagonal = original;
            }
        }
        row.last().copied().unwrap_or_default()
    }
    choices
        .iter()
        .map(|choice| (distance(got, choice), choice))
        .min()
        .filter(|(distance, _)| *distance <= 3)
        .map_or_else(
            || "Choose a supported value.".into(),
            |(_, choice)| format!("Did you mean {choice:?}?"),
        )
}

impl Validation<'_> {
    pub fn check(&self, document: &Document) -> Vec<Issue> {
        let mut issues = Vec::new();
        let mut ids = BTreeSet::new();
        let mut pending = vec![(&document.root, "/root".to_owned())];
        while let Some((node, path)) = pending.pop() {
            self.check_node(node, &path, &mut ids, &mut issues);
            pending.extend(
                node.children
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(index, child)| (child, format!("{path}/children/{index}"))),
            );
        }
        issues
    }

    fn check_node<'a>(
        &self,
        node: &'a Node,
        path: &str,
        ids: &mut BTreeSet<&'a str>,
        issues: &mut Vec<Issue>,
    ) {
        let mut issue = |property: &str, got: Value, accepted: Value, hint: String| {
            issues.push(Issue {
                node: path.into(),
                property: property.into(),
                got,
                accepted,
                hint,
            });
        };
        if let Some(id) = &node.id
            && (id.is_empty() || !ids.insert(id))
        {
            issue(
                "id",
                json!(id),
                json!("unique nonempty document id"),
                "Assign a unique id to this node.".into(),
            );
        }
        let Some(spec) = lookup(&node.kind) else {
            if !(self.mocket_placeholders && node.kind.starts_with("mocket:")) {
                issue(
                    "kind",
                    json!(node.kind),
                    json!(KINDS),
                    suggestion(&node.kind, KINDS),
                );
            }
            return;
        };
        if !spec.accepts_children && !node.children.is_empty() {
            issue(
                "children",
                json!(node.children.iter().map(|n| &n.kind).collect::<Vec<_>>()),
                json!([]),
                "Move these children into a stack, group, grid or card.".into(),
            );
        }
        for token in &node.theme_tokens {
            if !spec.theme_tokens.contains(&token.as_str()) {
                issue(
                    "themeTokens",
                    json!(token),
                    json!(spec.theme_tokens),
                    if spec.theme_tokens.is_empty() {
                        "This widget declares no theme tokens.".into()
                    } else {
                        format!("Use one of: {}.", spec.theme_tokens.join(", "))
                    },
                );
            }
        }
        let prop_names = spec.props.iter().map(|p| p.name).collect::<Vec<_>>();
        for name in node.props.keys().chain(node.bind.keys()) {
            if !prop_names.contains(&name.as_str()) {
                issue(
                    name,
                    node.props
                        .get(name)
                        .cloned()
                        .unwrap_or_else(|| json!(node.bind.get(name))),
                    json!(prop_names),
                    suggestion(name, &prop_names),
                );
            }
        }
        for prop in spec.props {
            if let Some(binding) = node.bind.get(prop.name)
                && binding.is_empty()
            {
                issue(
                    prop.name,
                    json!(binding),
                    json!("nonempty slot name"),
                    "Name a slot or supply a literal prop.".into(),
                );
            }
            let Some(got) = node.props.get(prop.name) else {
                if prop.required && !node.bind.contains_key(prop.name) {
                    issue(
                        prop.name,
                        Value::Null,
                        prop.schema(),
                        "Supply this prop as a literal or bind it to a slot.".into(),
                    );
                }
                continue;
            };
            if let Some(reference) = got.as_str().and_then(|text| text.strip_prefix('@'))
                && !reference.is_empty()
            {
                continue;
            }
            let valid = match prop.type_name {
                "string" => got
                    .as_str()
                    .is_some_and(|text| prop.accepted.is_empty() || prop.accepted.contains(&text)),
                "boolean" => got.is_boolean(),
                "number" => got.as_f64().is_some_and(|number| {
                    prop.range
                        .is_none_or(|(min, max)| (min..=max).contains(&number))
                }),
                "array" => got
                    .as_array()
                    .is_some_and(|items| items.iter().all(Value::is_string)),
                _ => false,
            };
            if !valid {
                let accepted = if prop.accepted.is_empty() {
                    prop.schema()
                } else {
                    json!(prop.accepted)
                };
                issue(
                    prop.name,
                    got.clone(),
                    accepted,
                    if prop.accepted.is_empty() {
                        "Use a value matching the property schema.".into()
                    } else {
                        format!("Use one of: {}.", prop.accepted.join(", "))
                    },
                );
            }
        }
        let event_names = spec.events.iter().map(|event| event.id).collect::<Vec<_>>();
        for (event, target) in &node.on {
            if !event_names.contains(&event.as_str()) {
                issue(
                    event,
                    json!(target),
                    json!(event_names),
                    suggestion(event, &event_names),
                );
            } else if target.is_empty()
                || self
                    .bound_events
                    .is_some_and(|names| !names.contains(target))
            {
                let accepted = self
                    .bound_events
                    .map_or_else(|| json!("nonempty event target"), |names| json!(names));
                let hints = self
                    .bound_events
                    .into_iter()
                    .flatten()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                issue(
                    event,
                    json!(target),
                    accepted,
                    if hints.is_empty() {
                        "Bind this event to a named host handler.".into()
                    } else {
                        suggestion(target, &hints)
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::WIDGETS;

    #[test]
    fn emits_structured_issues_for_authoring_mistakes() {
        let mut first = Node::new("text")
            .prop("colro", "blue")
            .prop("size", "med")
            .child(Node::new("spacer"));
        first.id = Some("same".into());
        let mut second = Node::new("button")
            .prop("label", "Save")
            .on("click", "svae");
        second.id = first.id.clone();
        let handlers = BTreeSet::from(["save".to_string()]);
        let issues = Validation {
            bound_events: Some(&handlers),
            ..Default::default()
        }
        .check(&Document::new(
            Node::new("stack").child(first).child(second),
        ));
        let properties: Vec<_> = issues.iter().map(|issue| issue.property.as_str()).collect();
        assert_eq!(
            properties,
            ["children", "colro", "text", "size", "id", "click"]
        );
        assert_eq!(issues[1].hint, "Did you mean \"color\"?");
        assert_eq!(issues[3].hint, "Use one of: xs, sm, md, lg, xl.");
        assert_eq!(issues[5].node, "/root/children/1");
        assert_eq!(issues[5].got, json!("svae"));
        assert_eq!(issues[5].accepted, json!(["save"]));
        assert_eq!(issues[5].hint, "Did you mean \"save\"?");
    }

    #[test]
    fn checks_theme_token_names_against_the_widget_descriptor() {
        let document = Document::from_json(r#"{"version":1,"root":{"kind":"text","props":{"text":"Example"},"themeTokens":["surfaces.text","surfaces.typo"]}}"#)
            .expect("theme token assertions");
        let issues = Validation::default().check(&document);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].property, "themeTokens");
        assert_eq!(issues[0].got, json!("surfaces.typo"));
        assert_eq!(
            issues[0].accepted,
            json!(lookup("text").expect("text descriptor").theme_tokens)
        );
    }

    #[test]
    fn accepts_inline_bindings_for_typed_props() {
        let document = Document::new(
            Node::new("text")
                .prop("text", "@item.name")
                .prop("size", "@size")
                .prop("dimmed", "@muted"),
        );
        assert!(Validation::default().check(&document).is_empty());
    }

    #[test]
    fn each_descriptor_example_is_renderable_by_the_registry() {
        let registry = crate::Registry::with_builtins();
        assert_eq!(registry.kinds().count(), WIDGETS.len());
        for spec in WIDGETS {
            let document: Document = serde_json::from_value(spec.example())
                .unwrap_or_else(|error| panic!("{}: {error}", spec.id));
            assert!(Validation::default().check(&document).is_empty());
            assert!(
                crate::Template::compile(&document, &registry).is_ok(),
                "{}",
                spec.id
            );
        }
    }
}
