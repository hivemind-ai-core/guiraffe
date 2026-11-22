//! DataUi adapters for the tree list and property grid.

use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::Arc,
};

use gpui_kit::{
    App, AppContext as _, Context, ElementId, Entity, IntoElement, RenderOnce, Subscription, Window,
};
use guiraffe_core::{
    widget_catalog::{EventHandler, WidgetNode, WidgetRegistration, WidgetRegistry},
    widget_value::Value as Data,
};

use super::{EditButtons, RowDensity, RowStyle, TreeEvent, TreeView};
use crate::{
    Children, NodeId, Property, PropertyOptions, TreeData, TreeSource, Value, widget_specs,
};

pub(crate) fn register(cx: &mut App) {
    WidgetRegistry::install(
        cx,
        &[
            WidgetRegistration {
                spec: widget_specs::WIDGETS[0],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[0]),
                render: |node, _, _, _| DocumentTree::<false>::from_node(node).into_any_element(),
            },
            WidgetRegistration {
                spec: widget_specs::WIDGETS[1],
                schema: guiraffe_core::widget_schema!(widget_specs::WIDGETS[1]),
                render: |node, _, _, _| DocumentTree::<true>::from_node(node).into_any_element(),
            },
        ],
    );
}

fn items(value: &Data) -> impl Iterator<Item = &Data> {
    (0..value.list_len().unwrap_or_default()).filter_map(|index| value.list_get(index))
}

fn unsigned(value: &Data) -> Option<u64> {
    value
        .as_f64()
        .filter(|v| *v >= 0.0 && *v < (1_u64 << 53) as f64 && v.fract() == 0.0)
        .map(|v| v as u64)
}

fn field<'a>(value: &'a Data, name: &str) -> &'a Data {
    value.field(&[name.into()])
}

#[derive(Default)]
struct DocumentRows {
    roots: Vec<NodeId>,
    nodes: BTreeMap<NodeId, DocumentRow>,
}

struct DocumentRow {
    label: String,
    children: Vec<NodeId>,
    parent: Option<NodeId>,
    unloaded: bool,
    icon: Option<String>,
    badge: Option<String>,
    checked: Option<bool>,
    height: Option<f32>,
    property: Option<DocumentProperty>,
}

struct DocumentProperty {
    source: Data,
    value: Value,
    default: Value,
    choices: Vec<String>,
    read_only: bool,
}

impl DocumentRows {
    fn parse(values: &Data) -> Self {
        let mut result = Self::default();
        let mut parents = Vec::new();
        for value in items(values) {
            let Some(id) = unsigned(field(value, "id")) else {
                continue;
            };
            let Some(label) = field(value, "label").as_str() else {
                continue;
            };
            let depth = unsigned(field(value, "depth")).unwrap_or_default() as usize;
            if depth > parents.len() || result.nodes.contains_key(&id) {
                continue;
            }
            parents.truncate(depth);
            let row = DocumentRow {
                label: label.to_owned(),
                children: Vec::new(),
                parent: parents.last().copied(),
                unloaded: field(value, "unloaded").as_bool().unwrap_or_default(),
                icon: field(value, "icon").as_str().map(str::to_owned),
                badge: field(value, "badge").as_str().map(str::to_owned),
                checked: field(value, "checked").as_bool(),
                height: field(value, "height")
                    .as_f64()
                    .filter(|v| *v > 0.0 && *v <= 4096.0)
                    .map(|v| v as f32),
                property: DocumentProperty::parse(field(value, "property")),
            };
            if let Some(parent) = parents.last().and_then(|id| result.nodes.get_mut(id)) {
                parent.children.push(id);
            } else {
                result.roots.push(id);
            }
            result.nodes.insert(id, row);
            parents.push(id);
        }
        result
    }
}

impl DocumentProperty {
    fn parse(source: &Data) -> Option<Self> {
        let kind = field(source, "type").as_str()?;
        let input = field(source, "value");
        let parse = |value: &Data| {
            Some(match kind {
                "text" => Value::Text(value.as_str()?.to_owned()),
                "number" => Value::Number(value.as_f64().filter(|v| v.is_finite())?),
                "bool" => Value::Bool(value.as_bool()?),
                "enum" => Value::Enum(value.as_str()?.to_owned()),
                "colour" => {
                    Value::Colour(u32::from_str_radix(value.as_str()?.strip_prefix('#')?, 16).ok()?)
                }
                "vector" => {
                    Value::Vector([value.list_get(0)?.as_f64()?, value.list_get(1)?.as_f64()?])
                }
                "rect" => Value::Rect([
                    value.list_get(0)?.as_f64()?,
                    value.list_get(1)?.as_f64()?,
                    value.list_get(2)?.as_f64()?,
                    value.list_get(3)?.as_f64()?,
                ]),
                "flags" => Value::Flags(
                    items(value)
                        .filter_map(Data::as_str)
                        .map(str::to_owned)
                        .collect(),
                ),
                "path" => Value::Path(value.as_str()?.to_owned()),
                _ => return None,
            })
        };
        let value = parse(input)?;
        Some(Self {
            source: source.clone(),
            default: parse(field(source, "default")).unwrap_or_else(|| value.clone()),
            value,
            choices: items(field(source, "choices"))
                .filter_map(Data::as_str)
                .map(str::to_owned)
                .collect(),
            read_only: field(source, "readOnly").as_bool().unwrap_or_default(),
        })
    }
}

impl TreeSource for DocumentRows {
    fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    fn children(&self, id: NodeId) -> Children<'_> {
        match self.nodes.get(&id) {
            Some(row) if row.unloaded && row.children.is_empty() => Children::Unloaded,
            Some(row) => Children::Ready(&row.children),
            None => Children::Ready(&[]),
        }
    }

    fn label(&self, id: NodeId) -> &str {
        self.nodes
            .get(&id)
            .map(|node| node.label.as_str())
            .unwrap_or_default()
    }
}

impl TreeData for DocumentRows {
    fn icon(&self, id: NodeId) -> Option<&str> {
        self.nodes.get(&id)?.icon.as_deref()
    }
    fn badge(&self, id: NodeId) -> Option<&str> {
        self.nodes.get(&id)?.badge.as_deref()
    }
    fn checked(&self, id: NodeId) -> Option<bool> {
        self.nodes.get(&id)?.checked
    }
    fn property(&self, id: NodeId) -> Option<Property<'_>> {
        let property = self.nodes.get(&id)?.property.as_ref()?;
        Some(Property {
            value: &property.value,
            default: &property.default,
            choices: &property.choices,
            read_only: property.read_only,
        })
    }
    fn property_options(&self, id: NodeId) -> PropertyOptions<'_> {
        let Some(property) = self.nodes.get(&id).and_then(|row| row.property.as_ref()) else {
            return PropertyOptions::default();
        };
        let source = &property.source;
        PropertyOptions {
            description: field(source, "description").as_str(),
            units: field(source, "units").as_str(),
            min: field(source, "min").as_f64(),
            max: field(source, "max").as_f64(),
            step: field(source, "step").as_f64(),
            mixed: field(source, "mixed").as_bool().unwrap_or_default(),
            advanced: field(source, "advanced").as_bool().unwrap_or_default(),
            multiline_rows: unsigned(field(source, "multilineRows"))
                .map(|rows| rows.clamp(1, 8) as usize),
            reset_button: field(source, "resetButton").as_bool(),
        }
    }

    fn validate(&self, id: NodeId, value: &Value) -> Result<(), String> {
        let options = self.property_options(id);
        if let Value::Number(value) = value
            && (options.min.is_some_and(|min| *value < min)
                || options.max.is_some_and(|max| *value > max))
        {
            return Err("Value outside the configured range".into());
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq)]
struct InspectorOptions {
    searchable: bool,
    split_ratio: Option<f32>,
    persist_key: Option<String>,
}

/// Property grid editing options, applied through the view's runtime setters.
#[derive(Clone, Copy, PartialEq)]
struct EditOptions {
    focus_row: bool,
    reset_button: bool,
    edit_buttons: EditButtons,
    /// Group expand/collapse animation; `None` follows `MotionPrefs::layout`.
    animate_layout: Option<bool>,
}

#[derive(IntoElement)]
struct DocumentTree<const P: bool> {
    key: ElementId,
    rows: Data,
    row_style: RowStyle,
    editing: EditOptions,
    expanded: Option<BTreeSet<NodeId>>,
    focusable: bool,
    inspector: InspectorOptions,
    handlers: [Option<EventHandler>; 6],
}

impl<const P: bool> DocumentTree<P> {
    fn from_node(node: &dyn WidgetNode) -> Self {
        let flag = |index| node.prop(index).as_bool().unwrap_or_default();
        Self {
            key: node.state_id(),
            rows: node.prop(0).clone(),
            focusable: flag(1),
            row_style: RowStyle {
                density: match node.prop(2).as_str() {
                    Some("compact") => RowDensity::Compact,
                    Some("comfortable") => RowDensity::Comfortable,
                    _ => RowDensity::Default,
                },
                alternating: flag(3),
                grid_lines: flag(4),
                indent_guides: flag(5),
                group_headers: flag(6),
                sticky_headers: flag(7),
            },
            expanded: node
                .prop(8)
                .list_len()
                .map(|_| items(node.prop(8)).filter_map(unsigned).collect()),
            inspector: InspectorOptions {
                searchable: flag(9),
                split_ratio: node.prop(10).as_f64().map(|v| v as f32),
                persist_key: node.prop(11).as_str().map(str::to_owned),
            },
            // Only the property grid declares these; unset values keep the Rust defaults.
            editing: EditOptions {
                focus_row: node.prop(12).as_bool().unwrap_or(true),
                reset_button: node.prop(13).as_bool().unwrap_or(true),
                edit_buttons: match node.prop(14).as_str() {
                    Some("icons") => EditButtons::Icons,
                    Some("labels") => EditButtons::Labels,
                    _ => EditButtons::Hidden,
                },
                animate_layout: node.prop(15).as_bool(),
            },
            handlers: std::array::from_fn(|index| node.handler(index)),
        }
    }
}

struct TreeDocumentState<const P: bool> {
    rows: Data,
    source: Rc<DocumentRows>,
    absent_parents: BTreeMap<NodeId, NodeId>,
    node: Entity<TreeView<P>>,
    inspector: InspectorOptions,
    handlers: [Option<EventHandler>; 6],
    subscription: Subscription,
}

fn event_payload(event: &TreeEvent) -> (usize, Data) {
    let id = |value: NodeId| Data::Num(value as f64);
    let record = |a, b| Data::Record(Arc::new([a, b].into()));
    match event {
        TreeEvent::LoadChildren(node) => (0, id(*node)),
        TreeEvent::Selected(node) => (1, id(*node)),
        TreeEvent::Checked(node, checked) => (
            2,
            record(
                ("id".into(), id(*node)),
                ("checked".into(), Data::Bool(*checked)),
            ),
        ),
        TreeEvent::Reorder { node, before } => (
            3,
            record(("id".into(), id(*node)), ("before".into(), id(*before))),
        ),
        TreeEvent::Edited(node, value) => (
            4,
            record(
                ("id".into(), id(*node)),
                ("value".into(), property_value(value)),
            ),
        ),
        TreeEvent::PickPath(node) => (5, id(*node)),
    }
}

fn property_value(value: &Value) -> Data {
    match value {
        Value::Text(value) | Value::Enum(value) | Value::Path(value) => value.clone().into(),
        Value::Number(value) => Data::Num(*value),
        Value::Bool(value) => Data::Bool(*value),
        Value::Colour(value) => format!("#{value:06x}").into(),
        Value::Vector(values) => Data::List(values.iter().copied().map(Data::Num).collect()),
        Value::Rect(values) => Data::List(values.iter().copied().map(Data::Num).collect()),
        Value::Flags(values) => {
            Data::List(values.iter().map(|value| value.as_str().into()).collect())
        }
    }
}

fn replace_rows<const P: bool>(
    tree: &mut TreeView<P>,
    previous: &DocumentRows,
    source: Rc<DocumentRows>,
    absent_parents: &mut BTreeMap<NodeId, NodeId>,
    cx: &mut Context<TreeView<P>>,
) {
    let mut next = BTreeMap::new();
    let mut path = Vec::new();
    for id in previous.nodes.keys().chain(absent_parents.keys()).copied() {
        if source.nodes.contains_key(&id) {
            continue;
        }
        if tree.model.expanded(id) || tree.model.selected(id) {
            path.clear();
            let mut child = id;
            let keep = loop {
                if let Some(row) = source.nodes.get(&child) {
                    break row.unloaded;
                }
                if next.contains_key(&child) {
                    break true;
                }
                let parent = previous
                    .nodes
                    .get(&child)
                    .and_then(|row| row.parent)
                    .or_else(|| absent_parents.get(&child).copied());
                let Some(parent) = parent else {
                    break false;
                };
                path.push((child, parent));
                child = parent;
            };
            if keep {
                next.extend(path.iter().copied());
                continue;
            }
        }
        tree.forget_removed([id]);
    }
    *absent_parents = next;
    tree.replace_source(source, cx);
}

#[cfg(test)]
#[gpui_kit::test]
fn document_tree_and_property_grid_release_replaced_rows(cx: &mut gpui_kit::TestAppContext) {
    fn churn<const P: bool>(cx: &mut App) {
        let mut previous = Rc::new(DocumentRows::default());
        let tree = cx.new(|cx| TreeView::<P>::new(previous.clone(), cx));
        let mut absent_parents = BTreeMap::new();
        for id in 1..=1024 {
            let source = Rc::new(DocumentRows {
                roots: vec![id],
                nodes: BTreeMap::from([(
                    id,
                    DocumentRow {
                        label: "row".into(),
                        children: vec![],
                        parent: None,
                        unloaded: true,
                        icon: None,
                        badge: None,
                        checked: None,
                        height: None,
                        property: None,
                    },
                )]),
            });
            tree.update(cx, |tree, cx| {
                replace_rows(tree, &previous, source.clone(), &mut absent_parents, cx);
                tree.model.set_expanded(id, true, source.as_ref());
                tree.model.select(id, true);
            });
            previous = source;
        }
        let model = &tree.read(cx).model;
        for id in 1..1024 {
            assert!(!model.selected(id));
            assert!(!model.expanded(id));
        }
        assert!(model.selected(1024));
        assert!(model.expanded(1024));
    }
    cx.update(|cx| {
        churn::<false>(cx);
        churn::<true>(cx);
    });
}

#[cfg(test)]
#[gpui_kit::test]
fn document_rows_keep_only_state_vouched_for_by_unloaded_ancestors(
    cx: &mut gpui_kit::TestAppContext,
) {
    fn snapshot(rows: &[(NodeId, Option<NodeId>, bool)]) -> Rc<DocumentRows> {
        let mut source = DocumentRows::default();
        for &(id, parent, unloaded) in rows {
            source.nodes.insert(
                id,
                DocumentRow {
                    label: "row".into(),
                    children: vec![],
                    parent,
                    unloaded,
                    icon: None,
                    badge: None,
                    checked: None,
                    height: None,
                    property: None,
                },
            );
            if let Some(parent) = parent {
                source
                    .nodes
                    .get_mut(&parent)
                    .expect("parent precedes child")
                    .children
                    .push(id);
            } else {
                source.roots.push(id);
            }
        }
        Rc::new(source)
    }

    cx.update(|cx| {
        let mut previous = snapshot(&[]);
        let mut links = BTreeMap::new();
        let tree = cx.new(|cx| TreeView::<false>::new(previous.clone(), cx));
        for leaf in 3..1027 {
            let complete = [
                (1, None, false),
                (2, Some(1), false),
                (leaf, Some(2), false),
            ];
            let root_unloaded = [(1, None, true)];
            let child_unloaded = [(1, None, false), (2, Some(1), true)];
            let leaf_removed = [(1, None, false), (2, Some(1), false)];
            for (step, (rows, expected_links, keep)) in [
                (&complete[..], 0, true),
                (&root_unloaded[..], 2, true),
                (&child_unloaded[..], 1, true),
                (&complete[..], 0, true),
                (&child_unloaded[..], 1, true),
                (
                    if leaf % 2 == 0 {
                        &leaf_removed[..]
                    } else {
                        &[]
                    },
                    0,
                    false,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let source = snapshot(rows);
                tree.update(cx, |tree, cx| {
                    replace_rows(tree, &previous, source.clone(), &mut links, cx);
                    if step == 0 {
                        tree.model.select(leaf, true);
                        tree.model.set_expanded(leaf, true, source.as_ref());
                    }
                    assert_eq!(tree.model.selected(leaf), keep);
                    assert_eq!(tree.model.expanded(leaf), keep);
                    assert_eq!(links.len(), expected_links);
                });
                previous = source;
            }
        }
        let model = &tree.read(cx).model;
        assert!((3..1027).all(|id| !model.selected(id) && !model.expanded(id)));
    });
}

fn make_tree<const P: bool>(
    source: Rc<DocumentRows>,
    options: &InspectorOptions,
    window: &mut Window,
    cx: &mut gpui_kit::Context<TreeDocumentState<P>>,
) -> (Entity<TreeView<P>>, Subscription) {
    let node = cx.new(|cx| {
        let mut tree = TreeView::new(source.clone(), cx).row_heights(
            source
                .nodes
                .iter()
                .filter_map(|(id, row)| row.height.map(|height| (*id, height))),
        );
        if options.searchable {
            tree = tree.searchable(window, cx);
        }
        if let Some(ratio) = options.split_ratio {
            tree = tree.split_ratio(ratio);
        }
        if let Some(key) = &options.persist_key {
            tree = tree.persist_key(key.clone(), cx);
        }
        tree
    });
    let subscription = cx.subscribe_in(
        &node,
        window,
        |state: &mut TreeDocumentState<P>, _, event, window, cx| {
            let (index, payload) = event_payload(event);
            if let Some(handler) = &state.handlers[index] {
                handler(payload, window, cx);
            }
        },
    );
    (node, subscription)
}

impl<const P: bool> RenderOnce for DocumentTree<P> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.key, cx, |window, cx| {
            let source = Rc::new(DocumentRows::parse(&self.rows));
            let (node, subscription) = make_tree::<P>(source.clone(), &self.inspector, window, cx);
            TreeDocumentState {
                rows: self.rows.clone(),
                source,
                absent_parents: BTreeMap::new(),
                node,
                inspector: self.inspector.clone(),
                handlers: std::array::from_fn(|_| None),
                subscription,
            }
        });
        state.update(cx, |state, cx| {
            let rows_changed = state.rows != self.rows;
            let previous = rows_changed.then(|| {
                state.rows = self.rows;
                let source = Rc::new(DocumentRows::parse(&state.rows));
                std::mem::replace(&mut state.source, source)
            });
            if state.inspector != self.inspector {
                state.inspector = self.inspector;
                state.absent_parents.clear();
                (state.node, state.subscription) =
                    make_tree(state.source.clone(), &state.inspector, window, cx);
            }
            state.handlers = self.handlers;
            state.node.update(cx, |tree, cx| {
                if let Some(previous) = previous {
                    replace_rows(
                        tree,
                        &previous,
                        state.source.clone(),
                        &mut state.absent_parents,
                        cx,
                    );
                    tree.set_row_heights(
                        state
                            .source
                            .nodes
                            .iter()
                            .filter_map(|(id, row)| row.height.map(|height| (*id, height))),
                        cx,
                    );
                }
                if tree.tab_stop != self.focusable {
                    tree.tab_stop = self.focusable;
                    cx.notify();
                }
                if tree.row_style != self.row_style {
                    tree.set_row_style(self.row_style, cx);
                }
                tree.set_focus_row(self.editing.focus_row, cx);
                tree.set_reset_button(self.editing.reset_button, cx);
                tree.set_edit_buttons(self.editing.edit_buttons, cx);
                if tree.collapse_motion.enabled != self.editing.animate_layout {
                    tree.set_animate_layout(self.editing.animate_layout);
                }
                if let Some(expanded) = &self.expanded {
                    for id in state.source.nodes.keys().copied() {
                        let open = expanded.contains(&id);
                        if tree.model.expanded(id) != open {
                            tree.set_expanded(id, open, cx);
                        }
                    }
                }
            });
            state.node.clone()
        })
    }
}
