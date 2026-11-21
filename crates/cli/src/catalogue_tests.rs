use super::*;
use serde_json::Value;

fn has_snapshot(changelog: &str, version: &str, hash: u64) -> bool {
    let prefix = format!("Catalogue {version} snapshot `{hash:016x}`");
    changelog.lines().any(|line| {
        line.strip_prefix(&prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
    })
}

#[test]
fn catalogue_changes_require_a_current_snapshot() {
    let hash = CATALOGUE
        .bytes()
        .chain(SCREEN_SCHEMA.bytes())
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    let version = guiraffe_ui::catalog::VERSION;
    assert!(
        has_snapshot(include_str!("../../../CHANGELOG.md"), version, hash),
        "Record Catalogue {version} snapshot `{hash:016x}` in CHANGELOG.md."
    );
}

#[test]
fn a_version_bump_and_a_second_edit_each_need_a_snapshot() {
    let changelog = "Catalogue 1.1.0 snapshot `0000000000000001`";
    assert!(has_snapshot(changelog, "1.1.0", 1));
    assert!(!has_snapshot(changelog, "1.2.0", 1));
    assert!(!has_snapshot(changelog, "1.1.0", 2));
}

fn members<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value[key].as_array().into_iter().flatten()
}

fn find<'a>(value: &'a Value, key: &str, field: &str, name: &Value) -> Option<&'a Value> {
    members(value, key).find(|item| item[field] == *name)
}

fn types(ty: &Value) -> Vec<&Value> {
    ty.as_array()
        .map_or_else(|| vec![ty], |types| types.iter().collect())
}

fn narrows(old: &Value, new: &Value) -> bool {
    let new_types = types(&new["type"]);
    types(&old["type"]).iter().any(|ty| !new_types.contains(ty))
        || new["enum"].as_array().is_some_and(|values| {
            old["enum"]
                .as_array()
                .is_none_or(|old_values| old_values.iter().any(|value| !values.contains(value)))
        })
        || new["minimum"]
            .as_f64()
            .is_some_and(|min| old["minimum"].as_f64().is_none_or(|old_min| min > old_min))
        || new["maximum"]
            .as_f64()
            .is_some_and(|max| old["maximum"].as_f64().is_none_or(|old_max| max < old_max))
        || (!old["items"].is_null() && narrows(&old["items"], &new["items"]))
}

fn breaking_changes(old: &Value, new: &Value) -> Vec<String> {
    let mut changes = Vec::new();
    for widget in members(old, "widgets") {
        let id = &widget["id"];
        let Some(current) = find(new, "widgets", "id", id) else {
            changes.push(format!("removed widget {id}"));
            continue;
        };
        for (group, name, schema) in [
            ("props", "name", "schema"),
            ("events", "id", "payloadSchema"),
        ] {
            for member in members(widget, group) {
                let member_name = &member[name];
                match find(current, group, name, member_name) {
                    None => changes.push(format!("removed {id} {group} entry {member_name}")),
                    Some(next)
                        if narrows(&member[schema], &next[schema])
                            || (next["required"] == true && member["required"] != true) =>
                    {
                        changes.push(format!("narrowed {id} {group} entry {member_name}"));
                    }
                    Some(_) => {}
                }
            }
        }
        for prop in members(current, "props") {
            if prop["required"] == true && find(widget, "props", "name", &prop["name"]).is_none() {
                changes.push(format!("added required {id} prop {}", prop["name"]));
            }
        }
    }
    changes
}

fn major(catalogue: &Value) -> u64 {
    catalogue["version"]
        .as_str()
        .and_then(|version| version.split('.').next())
        .and_then(|major| major.parse().ok())
        .expect("catalogue has a numerical major version")
}

#[test]
fn breaking_catalogue_changes_require_a_major_bump() {
    let baseline: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/catalogue-1.1.0-contract.json"
    ))
    .expect("published catalogue contract");
    let current: Value = serde_json::from_str(CATALOGUE).expect("generated catalogue");
    let changes = breaking_changes(&baseline, &current);
    assert!(
        major(&current) >= major(&baseline),
        "catalogue major regressed"
    );
    assert!(
        changes.is_empty() || major(&current) > major(&baseline),
        "{changes:?}"
    );
}

#[test]
fn identifies_removed_and_narrowed_descriptor_members() {
    let current: Value = serde_json::from_str(CATALOGUE).expect("generated catalogue");
    let mut changed = current.clone();
    let widgets = changed["widgets"].as_array_mut().expect("widget list");
    widgets.retain(|widget| widget["id"] != "stack");
    for widget in widgets {
        match widget["id"].as_str() {
            Some("button") => widget["events"]
                .as_array_mut()
                .expect("button events")
                .retain(|event| event["id"] != "click"),
            Some("title") => widget["props"]
                .as_array_mut()
                .expect("title props")
                .retain(|prop| prop["name"] != "order"),
            Some("select") => widget["props"][0]["schema"]["type"] = serde_json::json!("number"),
            Some("text") => widget["props"][2]["schema"]["enum"] = serde_json::json!(["sm"]),
            _ => {}
        }
    }
    assert_eq!(breaking_changes(&current, &changed).len(), 5);
}
