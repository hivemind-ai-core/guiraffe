//! Authoring-time validation for the built-in catalogue.

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    Document, Node,
    catalog::{WidgetSpec, all_widgets},
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

fn valid_iso_date(text: &str) -> bool {
    let Some((year, month_day)) = text.split_once('-') else {
        return false;
    };
    let Some((month, day)) = month_day.split_once('-') else {
        return false;
    };
    if year.len() != 4
        || month.len() != 2
        || day.len() != 2
        || !text.bytes().all(|b| b.is_ascii_digit() || b == b'-')
    {
        return false;
    }
    let Ok(year) = year.parse::<u32>() else {
        return false;
    };
    let Ok(month) = month.parse::<u32>() else {
        return false;
    };
    let Ok(day) = day.parse::<u32>() else {
        return false;
    };
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return false,
    };
    (1..=max).contains(&day)
}

fn valid_iso_time(text: &str) -> bool {
    let mut parts = text.split(':');
    let segment = |part: &str, max: u8| {
        part.len() == 2
            && part.bytes().all(|b| b.is_ascii_digit())
            && part.parse::<u8>().is_ok_and(|value| value <= max)
    };
    parts.next().is_some_and(|hour| segment(hour, 23))
        && parts.next().is_some_and(|minute| segment(minute, 59))
        && parts.next().is_none_or(|second| segment(second, 59))
        && parts.next().is_none()
}

fn calendar_value(value: &Value) -> bool {
    let endpoint = |value: &Value| {
        value.is_null()
            || value.as_str().is_some_and(|text| {
                text.split_once('T').map_or_else(
                    || valid_iso_date(text),
                    |(date, time)| valid_iso_date(date) && valid_iso_time(time),
                )
            })
    };
    value.as_array().map_or_else(
        || endpoint(value),
        |range| range.len() == 2 && range.iter().all(endpoint),
    )
}

fn check_timeline_tracks(value: &Value) -> bool {
    let Some(tracks) = value.as_array() else {
        return false;
    };
    let schema = crate::widget_spec::timeline_track_schema();
    tracks.iter().all(|track| {
        if !matches_data_schema(track, &schema) {
            return false;
        }
        let mut ids = BTreeSet::new();
        let Some(clips) = track["clips"].as_array() else {
            return false;
        };
        let mut spans = Vec::with_capacity(clips.len());
        for clip in clips {
            let Some(id) = clip["id"].as_u64() else {
                return false;
            };
            let Some((start, length)) = clip["start"].as_f64().zip(clip["length"].as_f64()) else {
                return false;
            };
            let end = start + length;
            if !ids.insert(id) || !end.is_finite() || end <= start {
                return false;
            }
            spans.push((start, end));
        }
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        spans.windows(2).all(|pair| pair[1].0 >= pair[0].1 - 1e-12)
    })
}

fn check_flame_frames(value: &Value) -> bool {
    let Some(frames) = value.as_array() else {
        return false;
    };
    let schema = crate::widget_spec::flame_frame_schema();
    let mut ids = BTreeSet::new();
    let mut ancestors: Vec<(u64, f64, f64)> = Vec::new();
    let mut extent = 0.0;
    for frame in frames {
        if !matches_data_schema(frame, &schema) {
            return false;
        }
        let Some(id) = frame["id"].as_u64() else {
            return false;
        };
        let parent = frame["parent"].as_u64();
        let Some((own, total)) = frame["self"].as_f64().zip(frame["total"].as_f64()) else {
            return false;
        };
        if !ids.insert(id) || own > total {
            return false;
        }
        while ancestors.last().map(|(id, _, _)| *id) != parent {
            if ancestors.pop().is_none() {
                return false;
            }
        }
        let (left, right) = if let Some((_, cursor, limit)) = ancestors.last_mut() {
            let left = *cursor;
            let right = left + total;
            let tolerance = right.abs().max(limit.abs()) * (4.0 * f64::EPSILON);
            if !right.is_finite() || right - *limit > tolerance {
                return false;
            }
            let right = right.min(*limit);
            *cursor = right;
            (left, right)
        } else {
            let left = extent;
            extent += total;
            if !extent.is_finite() {
                return false;
            }
            (left, extent)
        };
        ancestors.push((id, left, (right - own).max(left)));
    }
    true
}

fn check_tree_nodes(values: &Value) -> bool {
    let Some(values) = values.as_array() else {
        return false;
    };
    let schema = crate::widget_spec::tree_node_schema();
    let mut ids = BTreeSet::new();
    let mut maximum_depth = 0;
    for value in values {
        if !matches_data_schema(value, &schema) {
            return false;
        }
        let Some(id) = value["id"].as_u64() else {
            return false;
        };
        let depth = value["depth"].as_u64().unwrap_or_default();
        if depth > maximum_depth || !ids.insert(id) {
            return false;
        }
        maximum_depth = depth + 1;
        let property = &value["property"];
        if let Some(kind) = property["type"].as_str() {
            let matches_value = |value: &Value| match kind {
                "number" => value.is_number(),
                "bool" => value.is_boolean(),
                "text" | "enum" | "path" => value.is_string(),
                "colour" => value
                    .as_str()
                    .and_then(|text| text.strip_prefix('#'))
                    .is_some_and(|hex| {
                        hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit())
                    }),
                "vector" | "rect" => value.as_array().is_some_and(|array| {
                    array.len() == if kind == "vector" { 2 } else { 4 }
                        && array.iter().all(Value::is_number)
                }),
                "flags" => value
                    .as_array()
                    .is_some_and(|array| array.iter().all(Value::is_string)),
                _ => false,
            };
            if !matches_value(&property["value"])
                || property.get("default").is_some_and(|v| !matches_value(v))
            {
                return false;
            }
        }
    }
    true
}

// Validate the JSON Schema subset used by structured authoring props. Document links and bindings have already
// been resolved by the caller before checking a literal data shape.
fn matches_data_schema(value: &Value, schema: &Value) -> bool {
    let type_matches = |ty: &str| match ty {
        "array" => value.is_array(),
        "object" => value.is_object(),
        "boolean" => value.is_boolean(),
        "integer" => value.as_u64().is_some(),
        "number" => value.is_number(),
        "null" => value.is_null(),
        "string" => value.is_string(),
        _ => false,
    };
    let ty = &schema["type"];
    if ty.as_str().is_some_and(|ty| !type_matches(ty))
        || ty
            .as_array()
            .is_some_and(|types| !types.iter().filter_map(Value::as_str).any(type_matches))
        || schema["enum"]
            .as_array()
            .is_some_and(|choices| !choices.contains(value))
    {
        return false;
    }
    if let Some(number) = value.as_f64()
        && (schema["minimum"].as_f64().is_some_and(|min| number < min)
            || schema["maximum"].as_f64().is_some_and(|max| number > max))
    {
        return false;
    }
    if let Some(array) = value.as_array()
        && let Some(item_schema) = schema.get("items")
    {
        return array
            .iter()
            .all(|value| matches_data_schema(value, item_schema));
    }
    if let Some(fields) = value.as_object() {
        return schema["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .all(|name| fields.contains_key(name))
            && fields.iter().all(|(key, value)| {
                if let Some(field_schema) = schema["properties"].get(key) {
                    return matches_data_schema(value, field_schema);
                }
                match &schema["additionalProperties"] {
                    Value::Bool(false) => false,
                    additional @ Value::Object(_) => matches_data_schema(value, additional),
                    _ => true,
                }
            });
    }
    true
}

impl Validation<'_> {
    pub fn check(&self, document: &Document) -> Vec<Issue> {
        self.check_with_specs(document, all_widgets())
    }

    /// Validate against descriptors available to a particular host.
    pub fn check_with_registry(
        &self,
        document: &Document,
        registry: &crate::Registry,
    ) -> Vec<Issue> {
        self.check_with_specs(document, registry.specs())
    }

    fn check_with_specs(
        &self,
        document: &Document,
        specs: impl Iterator<Item = &'static WidgetSpec>,
    ) -> Vec<Issue> {
        let specs: std::collections::BTreeMap<_, _> = specs.map(|spec| (spec.id, spec)).collect();
        let mut issues = Vec::new();
        let mut ids = BTreeSet::new();
        let mut pending = vec![(&document.root, "/root".to_owned())];
        while let Some((node, path)) = pending.pop() {
            self.check_node(node, &path, &mut ids, &mut issues, &specs);
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
        specs: &std::collections::BTreeMap<&str, &WidgetSpec>,
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
        let Some(spec) = specs.get(node.kind.as_str()) else {
            if !(self.mocket_placeholders && node.kind.starts_with("mocket:")) {
                let kinds: Vec<_> = specs.keys().copied().collect();
                issue(
                    "kind",
                    json!(node.kind),
                    json!(kinds),
                    suggestion(&node.kind, &kinds),
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
                "iso-date" => got.as_str().is_some_and(valid_iso_date),
                "iso-time" => got.as_str().is_some_and(valid_iso_time),
                "calendar-value" => calendar_value(got),
                "object" => got.is_object(),
                "flow-graph" => {
                    let schema = crate::widget_spec::flow_graph_schema();
                    matches_data_schema(got, &schema)
                        && ["nodes", "edges"].into_iter().all(|key| {
                            got[key].as_object().is_some_and(|items| {
                                items.iter().all(|(key, item)| {
                                    key.parse::<u64>()
                                        .ok()
                                        .zip(item["id"].as_u64())
                                        .is_some_and(|(key, id)| key == id)
                                })
                            })
                        })
                }
                "calendar-presets" => got.as_array().is_some_and(|items| {
                    items.iter().all(|preset| {
                        preset
                            .as_object()
                            .is_some_and(|fields| fields.len() == 2 && fields.contains_key("value"))
                            && preset["label"].is_string()
                            && calendar_value(&preset["value"])
                    })
                }),
                "integer" => got.as_u64().is_some_and(|number| {
                    prop.range
                        .is_none_or(|(min, max)| (min..=max).contains(&(number as f64)))
                }),
                "dock-state" => matches_data_schema(got, &crate::widget_spec::dock_state_schema()),
                "dock-panels" => got.as_array().is_some_and(|panels| {
                    let schema = crate::widget_spec::dock_panel_schema();
                    panels
                        .iter()
                        .all(|panel| matches_data_schema(panel, &schema))
                }),
                "number" => got.as_f64().is_some_and(|number| {
                    prop.range
                        .is_none_or(|(min, max)| (min..=max).contains(&number))
                }),
                "integer-array" => got
                    .as_array()
                    .is_some_and(|items| items.iter().all(|item| item.as_u64().is_some())),
                "code-folds" => got.as_array().is_some_and(|folds| {
                    let schema = crate::widget_spec::code_fold_schema();
                    folds.iter().all(|fold| {
                        matches_data_schema(fold, &schema)
                            && fold["start"]
                                .as_u64()
                                .zip(fold["end"].as_u64())
                                .is_some_and(|(start, end)| end > start + 1)
                    })
                }),
                "code-annotations" => got.as_array().is_some_and(|annotations| {
                    let schema = crate::widget_spec::code_annotation_schema();
                    annotations.iter().all(|annotation| {
                        annotation.as_object().is_some_and(|fields| {
                            let integer = |name: &str| {
                                annotation[name].as_u64().filter(|v| *v < (1_u64 << 53))
                            };
                            let enum_member = |name: &str| {
                                schema["properties"][name]["enum"]
                                    .as_array()
                                    .is_some_and(|choices| choices.contains(&annotation[name]))
                            };
                            let start = integer("line").zip(integer("start"));
                            let end_line = if fields.contains_key("endLine") {
                                integer("endLine")
                            } else {
                                integer("line")
                            };
                            let end = end_line.zip(integer("end"));
                            integer("id").is_some()
                                && start.zip(end).is_some_and(|(start, end)| start <= end)
                                && enum_member("kind")
                                && fields.get("payload").is_none_or(Value::is_string)
                                && (!fields.contains_key("color") || enum_member("color"))
                                && fields
                                    .keys()
                                    .all(|key| schema["properties"].get(key).is_some())
                        })
                    })
                }),
                "range-value" => got.as_array().is_some_and(|values| {
                    values.len() == 2
                        && values[0]
                            .as_f64()
                            .zip(values[1].as_f64())
                            .is_some_and(|(low, high)| low <= high)
                }),
                "track-selection" => got.as_array().is_some_and(|values| {
                    values.is_empty()
                        || (values.len() == 2
                            && values
                                .iter()
                                .all(|v| v.as_u64().is_some_and(|v| u32::try_from(v).is_ok())))
                }),
                "timeline-tracks" => check_timeline_tracks(got),
                "flame-frames" => check_flame_frames(got),
                "grid-rows" => got.as_array().is_some_and(|rows| {
                    let schema = crate::widget_spec::grid_row_schema();
                    rows.iter().all(|row| matches_data_schema(row, &schema))
                }),
                "chart-layers" | "chart-axes" | "chart-labels" => {
                    let schema = match prop.type_name {
                        "chart-layers" => crate::widget_spec::chart_layer_schema(),
                        "chart-axes" => crate::widget_spec::chart_axis_schema(),
                        _ => crate::widget_spec::chart_label_schema(),
                    };
                    got.as_array().is_some_and(|items| {
                        items.iter().all(|item| matches_data_schema(item, &schema))
                    })
                }
                "chat-messages" | "chat-attachments" | "chat-commands" => {
                    let schema = match prop.type_name {
                        "chat-messages" => crate::widget_spec::chat_message_schema(),
                        "chat-attachments" => crate::widget_spec::chat_attachment_schema(),
                        _ => crate::widget_spec::chat_command_schema(),
                    };
                    let mut ids = BTreeSet::new();
                    got.as_array().is_some_and(|items| {
                        items.iter().all(|item| {
                            matches_data_schema(item, &schema) && ids.insert(item["id"].to_string())
                        })
                    })
                }
                "board-columns" => got.as_array().is_some_and(|columns| {
                    let schema = crate::widget_spec::board_column_schema();
                    let mut ids = BTreeSet::new();
                    columns.iter().all(|column| {
                        matches_data_schema(column, &schema)
                            && column["cards"].as_array().is_some_and(|cards| {
                                cards.iter().all(|card| {
                                    card["id"].as_u64().is_some_and(|id| ids.insert(id))
                                })
                            })
                    })
                }),
                "grid-columns" => got.as_array().is_some_and(|columns| {
                    let schema = crate::widget_spec::grid_column_schema();
                    let mut ids = BTreeSet::new();
                    columns.iter().all(|column| {
                        matches_data_schema(column, &schema)
                            && column["id"]
                                .as_str()
                                .is_some_and(|id| !id.is_empty() && ids.insert(id))
                    })
                }),
                "tree-nodes" => check_tree_nodes(got),
                "ring-sections" => got.as_array().is_some_and(|sections| {
                    sections.iter().all(|section| {
                        section.as_object().is_some_and(|fields| {
                            fields.len() == 2
                                && section["value"]
                                    .as_f64()
                                    .is_some_and(|v| (0.0..=100.0).contains(&v))
                                && section["color"]
                                    .as_str()
                                    .is_some_and(|c| prop.accepted.contains(&c))
                        })
                    })
                }),
                "array" => got
                    .as_array()
                    .is_some_and(|items| items.iter().all(Value::is_string)),
                _ => false,
            };
            if !valid {
                let accepted = if !prop.token_backed {
                    prop.schema()
                } else {
                    json!(prop.accepted)
                };
                issue(
                    prop.name,
                    got.clone(),
                    accepted,
                    if !prop.token_backed {
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
    use crate::catalog::{WIDGETS, lookup};

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
