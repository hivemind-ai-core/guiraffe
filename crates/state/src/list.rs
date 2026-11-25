//! Store-bound lists (provisional, 0.3): virtualized by default, fully mounted on request.
//!
//! The binding layer is [`RowViews`]: one retained view per item key, each watching only its
//! own item. A list view watches the list's key exactly, so an item edit costs the list one
//! order-stamp comparison, and an insert, remove or move notifies the list and the item alone.
//! How rows are laid out is the list element's business: [`BoundList`] uses `uniform_list`
//! (uniform row height) by default and plain children when mounted. A variable-height layout
//! (prefix sums) can drive the same [`RowViews`].

use std::{cell::{Cell, RefCell}, collections::HashMap, hash::Hash, ops::Range, panic::Location, rc::Rc};

use gpui_kit::{
    AnyElement, AnyView, App, AppContext, Context, ElementId, Entity, IntoElement, ParentElement,
    Pixels, Render, RenderOnce, StyleRefinement, Styled, WeakEntity, Window,
};

use crate::{
    Store,
    bind::{Holder, Watch},
    keyed::KeyedList,
    lens::{ByKey, Lens, ListLens},
    store::Slot,
};

type RowBody<S, L, K, T> = dyn Fn(ByKey<L, K>, &T, &Store<S>, &mut Window, &mut App) -> AnyElement;

/// How a [`BoundList`] lays out its rows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ListMode {
    /// Mount only the rows in view (`uniform_list`; rows share one height). The default.
    #[default]
    Virtual,
    /// Mount every row. Costs grow with the row count; see docs/performance.md.
    Mounted,
}

/// The retained view for one item.
pub struct RowView<S: Clone + PartialEq + 'static, L, K: 'static, T: 'static> {
    store: Store<S>,
    item: ByKey<L, K>,
    value: Option<T>,
    body: Rc<RefCell<Option<Rc<RowBody<S, L, K, T>>>>>,
    pending: bool,
    renders: usize,
    _watch: Option<Watch>,
}

impl<S, L, K, T> RowView<S, L, K, T>
where
    S: Clone + PartialEq + 'static,
    L: Lens<Root = S, Target = KeyedList<K, T>>,
    K: Copy + Hash + Eq + 'static,
    T: Clone + PartialEq + 'static,
{
    /// How often this row rendered (tests and devtools).
    pub fn renders(&self) -> usize {
        self.renders
    }
}

impl<S, L, K, T> Render for RowView<S, L, K, T>
where
    S: Clone + PartialEq + 'static,
    L: Lens<Root = S, Target = KeyedList<K, T>>,
    K: Copy + Hash + Eq + 'static,
    T: Clone + PartialEq + 'static,
{
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.pending = false;
        self.renders += 1;
        let body = self.body.borrow().clone();
        match (body, &self.value) {
            (Some(body), Some(value)) => body(self.item, value, &self.store, window, cx),
            _ => gpui_kit::Empty.into_any_element(),
        }
    }
}

struct RowSlot<S: Clone + PartialEq + 'static, L, K: 'static, T: 'static> {
    view: WeakEntity<RowView<S, L, K, T>>,
}

impl<S, L, K, T> Slot<S> for RowSlot<S, L, K, T>
where
    S: Clone + PartialEq + 'static,
    L: Lens<Root = S, Target = KeyedList<K, T>>,
    K: Copy + Hash + Eq + 'static,
    T: Clone + PartialEq + 'static,
{
    fn refresh(&self, state: &S, cx: &mut App) {
        let Some(view) = self.view.upgrade() else {
            return;
        };
        let row = view.read(cx);
        let current = row.item.get(state);
        if current == row.value.as_ref() {
            return;
        }
        view.update(cx, |row, cx| {
            match (&mut row.value, row.item.get(state)) {
                (Some(old), Some(new)) => old.clone_from(new),
                (value, new) => *value = new.cloned(),
            }
            if !row.pending {
                row.pending = true;
                cx.notify();
            }
        });
    }
    fn alive(&self) -> bool {
        self.view.upgrade().is_some()
    }
}

/// One retained row view per item key, created on demand and released when asked.
pub struct RowViews<S: Clone + PartialEq + 'static, L, K: 'static, T: 'static> {
    store: Store<S>,
    list: L,
    rows: HashMap<K, Entity<RowView<S, L, K, T>>>,
    body: Rc<RefCell<Option<Rc<RowBody<S, L, K, T>>>>>,
}

impl<S, L, K, T> RowViews<S, L, K, T>
where
    S: Clone + PartialEq + 'static,
    L: Lens<Root = S, Target = KeyedList<K, T>>,
    K: Copy + Hash + Eq + 'static,
    T: Clone + PartialEq + 'static,
{
    pub fn new(store: Store<S>, list: L) -> Self {
        Self {
            store,
            list,
            rows: HashMap::new(),
            body: Rc::default(),
        }
    }

    /// Replace the row body (when the list's parent renders). Rows re-render lazily.
    pub fn set_body(&self, body: Rc<RowBody<S, L, K, T>>) {
        *self.body.borrow_mut() = Some(body);
    }

    /// The view for `key`, created and subscribed to its item on first use.
    pub fn row(&mut self, key: K, cx: &mut App) -> Entity<RowView<S, L, K, T>> {
        if let Some(row) = self.rows.get(&key) {
            return row.clone();
        }
        let item = self.list.by_key(key);
        let store = self.store.clone();
        let body = self.body.clone();
        let row = cx.new(|cx: &mut Context<RowView<S, L, K, T>>| {
            let value = item.get(&store.snapshot(cx)).cloned();
            let slot: Rc<dyn Slot<S>> = Rc::new(RowSlot {
                view: cx.weak_entity(),
            });
            let mut chain = Vec::new();
            item.for_each_key(&mut |k| chain.push(k));
            let watch = store.watch_chains(&[chain], slot, cx);
            RowView {
                store,
                item,
                value,
                body,
                pending: false,
                renders: 0,
                _watch: Some(watch),
            }
        });
        self.rows.insert(key, row.clone());
        row
    }

    /// Release every row view whose key fails `keep`.
    pub fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) {
        self.rows.retain(|key, _| keep(key));
    }

    /// Mounted row views.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn get(&self, key: &K) -> Option<&Entity<RowView<S, L, K, T>>> {
        self.rows.get(key)
    }
}

/// The retained view behind a [`BoundList`].
pub struct ListView<S: Clone + PartialEq + 'static, L, K: Clone + Hash + Eq + 'static, T: 'static> {
    order: imbl::Vector<K>,
    stamp: u64,
    rows: Rc<RefCell<RowViews<S, L, K, T>>>,
    settings: Rc<Cell<(ListMode, Option<Pixels>)>>,
    pending: bool,
    renders: usize,
    _watch: Option<Watch>,
}

impl<S, L, K, T> ListView<S, L, K, T>
where
    S: Clone + PartialEq + 'static,
    L: Lens<Root = S, Target = KeyedList<K, T>>,
    K: Copy + Hash + Eq + 'static,
    T: Clone + PartialEq + 'static,
{
    /// The row views, for tests and custom layouts.
    pub fn rows(&self) -> Rc<RefCell<RowViews<S, L, K, T>>> {
        self.rows.clone()
    }

    /// How often the list itself rendered (tests and devtools).
    pub fn renders(&self) -> usize {
        self.renders
    }
}

struct ListSlot<S: Clone + PartialEq + 'static, L, K: Clone + Hash + Eq + 'static, T: 'static> {
    view: WeakEntity<ListView<S, L, K, T>>,
    list: L,
}

impl<S, L, K, T> Slot<S> for ListSlot<S, L, K, T>
where
    S: Clone + PartialEq + 'static,
    L: Lens<Root = S, Target = KeyedList<K, T>>,
    K: Copy + Hash + Eq + 'static,
    T: Clone + PartialEq + 'static,
{
    fn refresh(&self, state: &S, cx: &mut App) {
        let Some(view) = self.view.upgrade() else {
            return;
        };
        let stamp = self.list.get(state).map_or(0, KeyedList::order_stamp);
        // An item edit leaves the order and its stamp alone: one comparison.
        if view.read(cx).stamp == stamp {
            return;
        }
        let order = self
            .list
            .get(state)
            .map(|list| list.order().clone())
            .unwrap_or_default();
        view.update(cx, |list, cx| {
            list.stamp = stamp;
            list.order = order;
            if list.settings.get().0 == ListMode::Mounted {
                let live: std::collections::HashSet<K> = list.order.iter().copied().collect();
                list.rows.borrow_mut().retain(|key| live.contains(key));
            }
            if !list.pending {
                list.pending = true;
                cx.notify();
            }
        });
    }
    fn alive(&self) -> bool {
        self.view.upgrade().is_some()
    }
}

impl<S, L, K, T> Render for ListView<S, L, K, T>
where
    S: Clone + PartialEq + 'static,
    L: Lens<Root = S, Target = KeyedList<K, T>>,
    K: Copy + Hash + Eq + 'static,
    T: Clone + PartialEq + 'static,
{
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.pending = false;
        self.renders += 1;
        let cached = |row: Entity<RowView<S, L, K, T>>, height: Option<Pixels>| match height {
            Some(height) => AnyView::from(row).cached(
                StyleRefinement::default()
                    .w_full()
                    .h(height)
                    .flex_shrink_0(),
            )
            .into_any_element(),
            None => AnyView::from(row).into_any_element(),
        };
        let (mode, row_height) = self.settings.get();
        match mode {
            ListMode::Mounted => {
                let mut rows = self.rows.borrow_mut();
                let children: Vec<_> = self
                    .order
                    .iter()
                    .map(|key| cached(rows.row(*key, cx), row_height))
                    .collect();
                gpui_kit::div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .children(children)
                    .into_any_element()
            }
            ListMode::Virtual => {
                let order = self.order.clone();
                let rows = self.rows.clone();
                let height = row_height;
                gpui_kit::uniform_list(
                    "bound-rows",
                    order.len(),
                    move |range: Range<usize>, _, cx| {
                        let mut rows = rows.borrow_mut();
                        // uniform_list may measure the first row separately.
                        if range != (0..1) && order.len() > 1 {
                            let keep = range.start.saturating_sub(2)
                                ..range.end.saturating_add(2).min(order.len());
                            if rows.len() > keep.len() {
                                let window: std::collections::HashSet<K> =
                                    order.clone().slice(keep).into_iter().collect();
                                rows.retain(|key| window.contains(key));
                            }
                        }
                        range
                            .filter_map(|index| order.get(index).copied())
                            .map(|key| cached(rows.row(key, cx), height))
                            .collect()
                    },
                )
                .size_full()
                .into_any_element()
            }
        }
    }
}

/// A list bound to a store [`KeyedList`], as an element. Built by `ui!`'s
/// `for row in <= list { … }`; see [`bound_list`].
pub struct BoundList<S: Clone + PartialEq + 'static, L, K: 'static, T: 'static> {
    id: ElementId,
    store: Store<S>,
    list: L,
    mode: ListMode,
    row_height: Option<Pixels>,
    body: Rc<RowBody<S, L, K, T>>,
}

/// Bind a list to a store [`KeyedList`]. `body` builds one row from the item's lens and value.
#[track_caller]
pub fn bound_list<S, L, K, T>(
    store: &Store<S>,
    list: L,
    body: impl Fn(ByKey<L, K>, &T, &Store<S>, &mut Window, &mut App) -> AnyElement + 'static,
) -> BoundList<S, L, K, T>
where
    S: Clone + PartialEq + 'static,
    L: Lens<Root = S, Target = KeyedList<K, T>>,
    K: Copy + Hash + Eq + 'static,
    T: Clone + PartialEq + 'static,
{
    BoundList {
        id: ElementId::CodeLocation(*Location::caller()),
        store: store.clone(),
        list,
        mode: ListMode::Virtual,
        row_height: None,
        body: Rc::new(body),
    }
}

impl<S, L, K, T> BoundList<S, L, K, T>
where
    S: Clone + PartialEq + 'static,
    L: Lens<Root = S, Target = KeyedList<K, T>>,
    K: Copy + Hash + Eq + 'static,
    T: Clone + PartialEq + 'static,
{
    /// Mount every row instead of virtualizing.
    pub fn mounted(mut self) -> Self {
        self.mode = ListMode::Mounted;
        self
    }

    pub fn mode(mut self, mode: ListMode) -> Self {
        self.mode = mode;
        self
    }

    /// Give rows a definite height so each row is a cached view: an edit then repaints one
    /// row on both backends instead of every visible row on upstream GPUI.
    pub fn row_height(mut self, height: impl Into<Pixels>) -> Self {
        self.row_height = Some(height.into());
        self
    }

    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }
}

impl<S, L, K, T> IntoElement for BoundList<S, L, K, T>
where
    S: Clone + PartialEq + 'static,
    L: Lens<Root = S, Target = KeyedList<K, T>>,
    K: Copy + Hash + Eq + 'static,
    T: Clone + PartialEq + 'static,
{
    type Element = gpui_kit::ViewElement<Self>;
    fn into_element(self) -> Self::Element {
        gpui_kit::ViewElement::new(self)
    }
}

impl<S, L, K, T> RenderOnce for BoundList<S, L, K, T>
where
    S: Clone + PartialEq + 'static,
    L: Lens<Root = S, Target = KeyedList<K, T>>,
    K: Copy + Hash + Eq + 'static,
    T: Clone + PartialEq + 'static,
{
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            id,
            store,
            list,
            mode,
            row_height,
            body,
        } = self;
        let init = store.clone();
        let holder = window.use_keyed_state(id, cx, move |_, cx| {
            let state = init.snapshot(cx);
            let (order, stamp) = list
                .get(&state)
                .map(|l| (l.order().clone(), l.order_stamp()))
                .unwrap_or_default();
            let rows = Rc::new(RefCell::new(RowViews::new(init.clone(), list)));
            let settings = Rc::new(Cell::new((mode, row_height)));
            let view = cx.new(|cx: &mut Context<ListView<S, L, K, T>>| {
                let slot: Rc<dyn Slot<S>> = Rc::new(ListSlot {
                    view: cx.weak_entity(),
                    list,
                });
                let mut chain = Vec::new();
                list.for_each_key(&mut |k| chain.push(k));
                let watch = init.watch_chains(&[chain], slot, cx);
                ListView {
                    order,
                    stamp,
                    rows: rows.clone(),
                    settings: settings.clone(),
                    pending: false,
                    renders: 0,
                    _watch: Some(watch),
                }
            });
            Holder {
                view,
                cell: (rows, settings),
                fresh: Cell::new(true),
            }
        });
        let holder = holder.read(cx);
        let (rows, settings) = holder.cell.clone();
        let view = holder.view.clone();
        rows.borrow().set_body(body);
        settings.set((mode, row_height));
        // The parent rendered with a new row body: the list and its mounted rows render again.
        if holder.remounted() {
            let mut ids: Vec<_> = rows.borrow().rows.values().map(Entity::entity_id).collect();
            ids.push(view.entity_id());
            crate::bind::refresh_later(ids, cx);
        }
        AnyView::from(view)
    }
}
