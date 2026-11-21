use crate::geometry::RowGeometry;
use crate::{
    Children, Navigation, NodeId, Property, PropertyOptions, TreeModel, TreeSource, Value,
};
use gpui_kit::base::Selectable;
use gpui_kit::{
    AnyElement, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement, Render, Role,
    ScrollEvent, SharedString, StatefulInteractiveElement, Styled, TestSupportExt, Window,
    component::{
        Sizable, Size as ControlSize,
        button::{Button, ButtonVariants},
        checkbox::Checkbox,
        color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
        input::{Input, InputEvent, InputState, Textarea, TextareaState},
        menu::{ContextMenuExt, PopupMenuItem},
        slider::{Slider, SliderEvent, SliderState, SliderValue},
    },
    div, px,
};
use guiraffe_core::{
    GuiraffeTheme, Variant,
    control::{focus_ring, ring_visible},
};
use std::{cell::Cell, rc::Rc, time::Instant};

const ROW: f64 = 36.;

/// Default row height for a TreeList or PropertyGrid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowDensity {
    Compact,
    #[default]
    Default,
    Comfortable,
}

impl RowDensity {
    fn height(self) -> f64 {
        match self {
            Self::Compact => 28.,
            Self::Default => ROW,
            Self::Comfortable => 44.,
        }
    }
}

/// Viewport decoration options shared by TreeList and PropertyGrid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowStyle {
    pub density: RowDensity,
    pub alternating: bool,
    pub grid_lines: bool,
    pub indent_guides: bool,
    pub group_headers: bool,
    pub sticky_headers: bool,
}

pub trait TreeData: TreeSource {
    fn icon(&self, _: NodeId) -> Option<&str> {
        None
    }
    /// Render a custom row icon. The default preserves text icons supplied by
    /// [Self::icon].
    fn icon_element(&self, id: NodeId) -> Option<AnyElement> {
        self.icon(id)
            .map(|icon| div().child(icon.to_owned()).into_any_element())
    }
    fn badge(&self, _: NodeId) -> Option<&str> {
        None
    }
    fn checked(&self, _: NodeId) -> Option<bool> {
        None
    }
    fn property(&self, _: NodeId) -> Option<Property<'_>> {
        None
    }
    fn property_options(&self, _: NodeId) -> PropertyOptions<'_> {
        PropertyOptions::default()
    }
    fn validate(&self, _: NodeId, _: &Value) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TreeEvent {
    LoadChildren(NodeId),
    Selected(NodeId),
    Checked(NodeId, bool),
    Reorder { node: NodeId, before: NodeId },
    Edited(NodeId, Value),
    PickPath(NodeId),
}

fn row_control_size(height: f32) -> ControlSize {
    if height <= 28. {
        ControlSize::XSmall
    } else {
        ControlSize::Medium
    }
}

fn text_height(text: &str, max_rows: usize) -> f64 {
    (text.split('\n').take(max_rows).count().max(1) * 22 + 14) as f64
}

struct Edit {
    id: NodeId,
    input: EditorInput,
    height: Option<f64>,
    _subscriptions: Vec<gpui_kit::Subscription>,
}

enum EditorInput {
    Single(Entity<InputState>),
    Multiline(Entity<TextareaState>),
    Components(Vec<Entity<InputState>>),
}

impl EditorInput {
    fn value(&self, cx: &gpui_kit::App) -> SharedString {
        match self {
            Self::Single(input) => input.read(cx).value(),
            Self::Multiline(input) => input.read(cx).value(),
            Self::Components(inputs) => inputs
                .iter()
                .map(|input| input.read(cx).value().to_string())
                .collect::<Vec<_>>()
                .join(",")
                .into(),
        }
    }

    fn focus_adjacent(&self, reverse: bool, window: &mut Window, cx: &mut gpui_kit::App) -> bool {
        let Self::Components(inputs) = self else {
            return false;
        };
        let Some(current) = inputs
            .iter()
            .position(|input| input.read(cx).focus_handle(cx).is_focused(window))
        else {
            return false;
        };
        let next = if reverse {
            current.checked_sub(1)
        } else {
            Some(current + 1)
        };
        if let Some(input) = next.and_then(|index| inputs.get(index)) {
            let focus = input.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
            true
        } else {
            false
        }
    }
}

pub struct TreeView<const PROPERTIES: bool> {
    source: Rc<dyn TreeData>,
    model: TreeModel,
    focus: FocusHandle,
    tab_stop: bool,
    scroll: f64,
    height: f32,
    row_style: RowStyle,
    geometry: RowGeometry,
    split: Option<Split>,
    split_origin: f32,
    search: Option<(Entity<InputState>, gpui_kit::Subscription)>,
    filter: Option<Filter>,
    show_advanced: bool,
    expand_task: Option<gpui_kit::Task<()>>,
    colour_picker: Option<(NodeId, Entity<ColorPickerState>, gpui_kit::Subscription)>,
    slider: Option<(NodeId, Entity<SliderState>, gpui_kit::Subscription)>,
    scrub: Option<Scrub>,
    drag: Option<(NodeId, gpui_kit::Point<gpui_kit::Pixels>)>,
    edit: Option<Edit>,
    error: Option<(NodeId, String)>,
}

struct Filter {
    query: String,
    ranks: Vec<usize>,
    task: Option<gpui_kit::Task<()>>,
    pending: bool,
}

struct Scrub {
    id: NodeId,
    start: f32,
    initial: f64,
    preview: f64,
}

struct Split {
    ratio: f32,
    key: Option<String>,
    width: Rc<Cell<f32>>,
    dragging: bool,
}

impl Split {
    fn clamp(&self, ratio: f32) -> f32 {
        let margin = (96. / self.width.get().max(1.)).min(0.5);
        ratio.clamp(margin, 1. - margin)
    }
}

impl<const P: bool> TreeView<P> {
    /// Enable the property inspector toolbar and persisted split together.
    /// This builder leaves TreeList unchanged.
    pub fn inspector_controls(
        self,
        key: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if P {
            self.searchable(window, cx).persist_key(key, cx)
        } else {
            self
        }
    }

    /// Show a filter for currently expanded properties.
    pub fn searchable(mut self, window: &mut Window, cx: &mut Context<Self>) -> Self {
        if !P {
            return self;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter properties"));
        let subscription = cx.subscribe(&input, |this, input, event, cx| {
            if matches!(event, InputEvent::Change) {
                let query = input.read(cx).value().trim().to_lowercase();
                if this
                    .filter
                    .as_ref()
                    .is_none_or(|filter| filter.query != query)
                {
                    this.filter_rows(query, cx);
                }
            }
        });
        self.search = Some((input, subscription));
        self
    }

    /// Enable a draggable divider with an initial name-column fraction.
    pub fn split_ratio(mut self, ratio: f32) -> Self {
        if P && ratio.is_finite() {
            self.split = Some(Split {
                ratio: ratio.clamp(0.05, 0.95),
                key: None,
                width: Rc::new(Cell::new(0.)),
                dragging: false,
            });
        }
        self
    }

    /// Restore and save the split ratio through the application's layout store.
    pub fn persist_key(self, key: impl Into<String>, cx: &mut Context<Self>) -> Self {
        let key = key.into();
        let initial = self.split.as_ref().map_or(0.5, |split| split.ratio);
        guiraffe_layout::init(cx);
        let ratio = guiraffe_layout::layout_store(cx)
            .and_then(|store| store.splitter_sizes(&key))
            .and_then(|sizes| sizes.get(0))
            .unwrap_or(initial);
        let mut this = self.split_ratio(ratio);
        if let Some(split) = &mut this.split {
            split.key = Some(key);
        }
        this
    }
}

pub type TreeList = TreeView<false>;
pub type PropertyGrid = TreeView<true>;

impl<const P: bool> EventEmitter<TreeEvent> for TreeView<P> {}

impl<const P: bool> TreeView<P> {
    pub fn new(source: Rc<dyn TreeData>, cx: &mut Context<Self>) -> Self {
        Self {
            model: TreeModel::new(source.as_ref()),
            source,
            focus: cx.focus_handle(),
            tab_stop: false,
            height: 288.,
            scroll: 0.,
            row_style: RowStyle::default(),
            geometry: RowGeometry::default(),
            split: None,
            split_origin: 0.,
            search: None,
            filter: None,
            show_advanced: true,
            expand_task: None,
            colour_picker: None,
            slider: None,
            scrub: None,
            drag: None,
            edit: None,
            error: None,
        }
    }

    pub fn focusable(mut self, enabled: bool) -> Self {
        self.tab_stop = enabled;
        self
    }

    pub fn row_style(mut self, style: RowStyle) -> Self {
        self.row_style = style;
        self.rebuild_geometry();
        self
    }

    pub fn set_row_style(&mut self, style: RowStyle, cx: &mut Context<Self>) {
        if self.row_style != style {
            self.row_style = style;
            self.rebuild_geometry();
            self.clamp_scroll();
            cx.notify();
        }
    }

    /// Override heights by stable node ID. Heights are expressed in pixels.
    pub fn row_heights(mut self, overrides: impl IntoIterator<Item = (NodeId, f32)>) -> Self {
        self.geometry.set(overrides);
        self.rebuild_geometry();
        self
    }

    pub fn set_row_heights(
        &mut self,
        overrides: impl IntoIterator<Item = (NodeId, f32)>,
        cx: &mut Context<Self>,
    ) {
        self.geometry.set(overrides);
        self.rebuild_geometry();
        self.clamp_scroll();
        cx.notify();
    }

    fn rebuild_geometry(&mut self) {
        let model = &self.model;
        let filter = &self.filter;
        let editor = self
            .edit
            .as_ref()
            .and_then(|edit| Some((edit.id, edit.height?)));
        self.geometry
            .rebuild(self.row_style.density.height(), editor, |id| {
                let rank = model.row_index(id)?;
                match filter {
                    Some(filter) => filter.ranks.binary_search(&rank).ok(),
                    None => Some(rank),
                }
            });
    }

    /// Open loaded branches in batches. Unloaded branches remain closed.
    pub fn expand_loaded(&mut self, cx: &mut Context<Self>) {
        if let Some(filter) = &mut self.filter {
            filter.pending = true;
            filter.ranks.clear();
            filter.task = None;
            self.clamp_scroll();
            cx.notify();
        }
        let executor = cx.background_executor().clone();
        self.expand_task = Some(cx.spawn(async move |view, cx| {
            let mut start = 0;
            loop {
                executor.timer(std::time::Duration::from_millis(1)).await;
                let Ok(next) = view.update(cx, |this, cx| {
                    let viewport_end = this.window().end;
                    let mut changed = false;
                    let mut visible_change = false;
                    let next = start + 32;
                    for rank in start..next {
                        let Some(row) = this.model.row(rank) else {
                            break;
                        };
                        if !this.model.expanded(row.id)
                            && let Children::Ready(children) = this.source.children(row.id)
                            && !children.is_empty()
                        {
                            this.model.set_expanded(row.id, true, this.source.as_ref());
                            changed = true;
                            visible_change |= rank < viewport_end;
                        }
                    }
                    if changed {
                        this.rebuild_geometry();
                    }
                    if visible_change {
                        cx.notify();
                    }
                    if next >= this.model.row_count() {
                        this.restart_filter(cx);
                        None
                    } else {
                        Some(next)
                    }
                }) else {
                    break;
                };
                let Some(next) = next else { break };
                start = next;
            }
        }));
    }

    pub fn collapse_all(&mut self, cx: &mut Context<Self>) {
        self.expand_task = None;
        if self.model.collapse_all(self.source.as_ref()) {
            self.rebuild_geometry();
            self.restart_filter(cx);
            self.clamp_scroll();
            cx.notify();
        }
    }

    fn filter_rows(&mut self, query: String, cx: &mut Context<Self>) {
        if query.is_empty() && self.show_advanced {
            if self.filter.take().is_some() {
                self.scroll = 0.;
                self.rebuild_geometry();
                cx.notify();
            }
            return;
        }
        self.scroll = 0.;
        let needle = query.clone();
        let executor = cx.background_executor().clone();
        let task = cx.spawn(async move |view, cx| {
            let mut matches = Vec::new();
            let mut start = 0;
            loop {
                executor.timer(std::time::Duration::from_millis(1)).await;
                let Ok((end, done)) = view.read_with(cx, |this, _| {
                    let count = this.model.row_count();
                    let end = (start + 512).min(count);
                    for rank in start..end {
                        if let Some(row) = this.model.row(rank)
                            && (this.show_advanced
                                || !this.source.property_options(row.id).advanced)
                            && (needle.is_empty()
                                || this.source.label(row.id).to_lowercase().contains(&needle))
                        {
                            matches.push(rank);
                        }
                    }
                    (end, end == count)
                }) else {
                    break;
                };
                if done {
                    let _ = view.update(cx, |this, cx| {
                        if let Some(filter) = &mut this.filter {
                            filter.ranks = matches;
                            filter.pending = false;
                            this.rebuild_geometry();
                            cx.notify();
                        }
                    });
                    break;
                }
                start = end;
            }
        });
        self.filter = Some(Filter {
            query,
            ranks: Vec::new(),
            task: Some(task),
            pending: true,
        });
        self.rebuild_geometry();
        cx.notify();
    }

    fn restart_filter(&mut self, cx: &mut Context<Self>) {
        if let Some(filter) = &self.filter {
            self.filter_rows(filter.query.clone(), cx);
        }
    }

    fn row_count(&self) -> usize {
        self.filter
            .as_ref()
            .map_or_else(|| self.model.row_count(), |filter| filter.ranks.len())
    }

    fn model_rank(&self, display: usize) -> Option<usize> {
        match &self.filter {
            Some(filter) => filter.ranks.get(display).copied(),
            None => Some(display),
        }
    }

    fn display_rank(&self, id: NodeId) -> Option<usize> {
        let rank = self.model.row_index(id)?;
        match &self.filter {
            Some(filter) => filter.ranks.binary_search(&rank).ok(),
            None => Some(rank),
        }
    }

    fn row_top(&self, rank: usize) -> f64 {
        self.geometry.top(rank, self.row_style.density.height())
    }

    fn window(&self) -> std::ops::Range<usize> {
        self.geometry.window(
            self.row_count(),
            self.scroll,
            f64::from(self.height),
            self.row_style.density.height(),
        )
    }

    pub fn replace_source(&mut self, source: Rc<dyn TreeData>, cx: &mut Context<Self>) {
        self.expand_task = None;
        self.source = source;
        self.model.refresh(self.source.as_ref());
        if self.edit.as_ref().is_some_and(|edit| {
            self.model.row_index(edit.id).is_none()
                || self
                    .source
                    .property(edit.id)
                    .is_none_or(|property| property.read_only)
        }) {
            self.edit = None;
            self.error = None;
        }
        self.rebuild_geometry();
        self.restart_filter(cx);
        self.clamp_scroll();
        cx.notify();
    }

    pub fn set_expanded(&mut self, id: NodeId, open: bool, cx: &mut Context<Self>) {
        self.expand_task = None;
        let previously_visible = self
            .model
            .row_index(id)
            .filter(|_| self.model.expanded(id) != open)
            .map(|rank| (rank < self.window().end, self.model.cursor()));
        if let Some(id) = self.model.set_expanded(id, open, self.source.as_ref()) {
            cx.emit(TreeEvent::LoadChildren(id));
        }
        if let Some((in_viewport, cursor)) = previously_visible {
            self.rebuild_geometry();
            self.restart_filter(cx);
            let scroll = self.scroll;
            self.clamp_scroll();
            if in_viewport || self.scroll != scroll || self.model.cursor() != cursor {
                cx.notify();
            }
        }
    }

    fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.clamp(
            0.,
            (self.row_top(self.row_count()) - f64::from(self.height)).max(0.),
        );
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if key == "escape" && self.edit.is_none() {
            let picker = self.colour_picker.take().is_some();
            let slider = self.slider.take().is_some();
            let scrub = self.scrub.take().is_some();
            if picker || slider || scrub {
                if self.tab_stop {
                    window.focus(&self.focus, cx);
                }
                cx.notify();
                cx.stop_propagation();
                return;
            }
        }

        if key == "tab"
            && self.edit.is_some()
            && self.tab_to_editor(event.keystroke.modifiers.shift, window, cx)
        {
            cx.stop_propagation();
            return;
        }
        if self.edit.is_some() {
            if key == "escape" {
                self.edit = None;
                self.rebuild_geometry();
                self.clamp_scroll();
                self.error = None;
                if self.tab_stop {
                    window.focus(&self.focus, cx);
                }
                cx.notify();
                cx.stop_propagation();
            }
            return;
        }
        let modifiers = event.keystroke.modifiers;
        if P && (modifiers.control || modifiers.platform)
            && let Some(id) = self
                .model
                .cursor()
                .filter(|&id| self.display_rank(id).is_some())
            && let Some(property) = self.source.property(id)
        {
            match key {
                "c" => cx
                    .write_to_clipboard(gpui_kit::ClipboardItem::new_string(property.value.text())),
                "v" if !property.read_only => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.commit_text(id, &text, cx);
                    }
                }
                _ => return,
            }
            cx.stop_propagation();
            return;
        }
        let direction = match key {
            "up" => Some(Navigation::Up),
            "down" => Some(Navigation::Down),
            "left" => Some(Navigation::Left),
            "right" => Some(Navigation::Right),
            "home" => Some(Navigation::Home),
            "end" => Some(Navigation::End),
            _ => None,
        };
        if let Some(direction) = direction {
            if self.filter.is_some() {
                let current = self.model.cursor().and_then(|id| self.display_rank(id));
                let next = match direction {
                    Navigation::Up => current.map_or(0, |i| i.saturating_sub(1)),
                    Navigation::Down => current.map_or(0, |i| i.saturating_add(1)),
                    Navigation::Home => 0,
                    Navigation::End => self.row_count().saturating_sub(1),
                    _ => return,
                }
                .min(self.row_count().saturating_sub(1));
                if let Some(row) = self.model_rank(next).and_then(|rank| self.model.row(rank)) {
                    self.model.select(row.id, false);
                }
            } else {
                if matches!(direction, Navigation::Left | Navigation::Right) {
                    self.expand_task = None;
                }
                if let Some(id) = self.model.navigate(direction, self.source.as_ref()) {
                    cx.emit(TreeEvent::LoadChildren(id));
                }
                if matches!(direction, Navigation::Left | Navigation::Right) {
                    self.rebuild_geometry();
                }
            }
        } else if key == "enter"
            && P
            && let Some(id) = self
                .model
                .cursor()
                .filter(|&id| self.display_rank(id).is_some())
            && let Some(property) = self.source.property(id).filter(|p| !p.read_only)
        {
            match property.value {
                Value::Bool(value) => self.edit_value(id, Value::Bool(!value), cx),
                _ => self.start_edit(id, window, cx),
            }
        } else if matches!(key, "space" | "enter") {
            if let Some(id) = self.model.cursor() {
                self.model.select(id, event.keystroke.modifiers.shift);
                cx.emit(TreeEvent::Selected(id));
            }
        } else if !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.platform
            && let Some(text) = &event.keystroke.key_char
            && let Some(ch) = text.chars().next()
        {
            self.model
                .type_ahead(ch, Instant::now(), self.source.as_ref());
        } else {
            return;
        }

        if let Some(index) = self.model.cursor().and_then(|id| self.display_rank(id)) {
            let top = self.row_top(index);
            let bottom = self.row_top(index + 1);
            self.scroll = self
                .scroll
                .clamp((bottom - f64::from(self.height)).max(0.).min(top), top);
        }
        self.clamp_scroll();
        cx.notify();
        cx.stop_propagation();
    }

    fn tab_to_editor(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = &self.edit else { return false };
        let id = edit.id;
        let Some(rank) = self.display_rank(id) else {
            return false;
        };
        if edit.input.focus_adjacent(reverse, window, cx) {
            return true;
        }
        let text = edit.input.value(cx);
        self.commit_text(id, &text, cx);
        if self.edit.is_some() {
            return true;
        }
        let ranks = self.window();
        let next = if reverse {
            (ranks.start..rank)
                .rev()
                .find_map(|rank| self.inline_editor_at(rank))
        } else {
            (rank + 1..ranks.end).find_map(|rank| self.inline_editor_at(rank))
        };
        if let Some(next) = next {
            self.model.select(next, false);
            self.start_edit(next, window, cx);
            true
        } else {
            false
        }
    }

    fn inline_editor_at(&self, rank: usize) -> Option<NodeId> {
        let id = self.model.row(self.model_rank(rank)?)?.id;
        let property = self.source.property(id)?;
        (!property.read_only
            && !matches!(
                property.value,
                Value::Bool(_) | Value::Enum(_) | Value::Flags(_)
            ))
        .then_some(id)
    }

    fn edit_value(&mut self, id: NodeId, value: Value, cx: &mut Context<Self>) {
        let Some(property) = self.source.property(id).filter(|p| !p.read_only) else {
            return;
        };
        let options = self.source.property_options(id);
        let valid = match &value {
            Value::Number(value) => {
                value.is_finite()
                    && options.min.is_none_or(|min| *value >= min)
                    && options.max.is_none_or(|max| *value <= max)
            }
            Value::Enum(value) => property.choices.contains(value),
            Value::Flags(values) => values.iter().all(|value| property.choices.contains(value)),
            _ => true,
        };
        let validation = if valid {
            self.source.validate(id, &value)
        } else {
            Err("Value is outside the allowed range or choices".to_owned())
        };
        match validation {
            Ok(()) => {
                let mut redraw = self.error.take().is_some();
                if self
                    .colour_picker
                    .as_ref()
                    .is_some_and(|(row, _, _)| *row == id)
                {
                    self.colour_picker = None;
                    redraw = true;
                }
                if self.slider.as_ref().is_some_and(|(row, _, _)| *row == id) {
                    self.slider = None;
                    redraw = true;
                }
                if self.edit.take().is_some() {
                    redraw = true;
                    self.rebuild_geometry();
                    self.clamp_scroll();
                }
                cx.emit(TreeEvent::Edited(id, value));
                if redraw {
                    cx.notify();
                }
                return;
            }
            Err(error) => self.error = Some((id, error)),
        }
        cx.notify();
    }

    fn commit_text(&mut self, id: NodeId, text: &str, cx: &mut Context<Self>) {
        if let Some(value) = self
            .source
            .property(id)
            .and_then(|property| property.value.parse(text))
        {
            self.edit_value(id, value, cx);
        } else {
            self.error = Some((id, "Invalid value".to_owned()));
            cx.notify();
        }
    }

    fn start_edit(&mut self, id: NodeId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(property) = self
            .source
            .property(id)
            .filter(|property| !property.read_only)
        else {
            return;
        };
        let options = self.source.property_options(id);
        let text = if options.mixed {
            String::new()
        } else {
            property.value.text()
        };
        let multiline = options
            .multiline_rows
            .filter(|_| matches!(property.value, Value::Text(_)))
            .map(|rows| rows.clamp(2, 8));
        let coordinates: &[f64] = match property.value {
            Value::Vector(values) => values,
            Value::Rect(values) => values,
            _ => &[],
        };
        let (input, focus, height, subscriptions) = if !coordinates.is_empty() {
            let values: Vec<_> = coordinates
                .iter()
                .map(|value| {
                    if options.mixed {
                        String::new()
                    } else {
                        value.to_string()
                    }
                })
                .collect();
            let mut inputs = Vec::with_capacity(values.len());
            let mut subscriptions = Vec::with_capacity(values.len());
            for value in values {
                let input = cx.new(|cx| InputState::new(window, cx));
                input.update(cx, |input, cx| input.set_value(value, window, cx));
                subscriptions.push(cx.subscribe_in(
                    &input,
                    window,
                    move |this, _, event, window, cx| {
                        if matches!(event, InputEvent::PressEnter { .. })
                            && let Some(edit) = &this.edit
                            && edit.id == id
                        {
                            let text = edit.input.value(cx);
                            this.commit_text(id, &text, cx);
                            if this.edit.is_none() && this.tab_stop {
                                window.focus(&this.focus, cx);
                            }
                        }
                    },
                ));
                inputs.push(input);
            }
            let Some(first) = inputs.first() else { return };
            let focus = first.read(cx).focus_handle(cx);
            (EditorInput::Components(inputs), focus, None, subscriptions)
        } else if let Some(rows) = multiline {
            let height = text_height(&text, rows);
            let input = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, rows));
            input.update(cx, |input, cx| input.set_value(text, window, cx));
            let subscription = cx.subscribe(&input, move |this, input, event, cx| {
                if matches!(event, InputEvent::Change) {
                    let height = text_height(&input.read(cx).value(), rows);
                    if let Some(edit) = &mut this.edit
                        && edit.id == id
                        && edit.height != Some(height)
                    {
                        edit.height = Some(height);
                        this.rebuild_geometry();
                        cx.notify();
                    }
                }
            });
            let focus = input.read(cx).focus_handle(cx);
            (
                EditorInput::Multiline(input),
                focus,
                Some(height),
                vec![subscription],
            )
        } else {
            let input = cx.new(|cx| InputState::new(window, cx));
            input.update(cx, |input, cx| input.set_value(text, window, cx));
            let subscription =
                cx.subscribe_in(&input, window, move |this, input, event, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.commit_text(id, &input.read(cx).value(), cx);
                        if this.edit.is_none() && this.tab_stop {
                            window.focus(&this.focus, cx);
                        }
                    }
                });
            let focus = input.read(cx).focus_handle(cx);
            (EditorInput::Single(input), focus, None, vec![subscription])
        };
        window.focus(&focus, cx);
        self.error = None;
        self.edit = Some(Edit {
            id,
            input,
            height,
            _subscriptions: subscriptions,
        });
        self.rebuild_geometry();
        cx.notify();
    }

    fn resize_split(&mut self, ratio: f32, save: bool, cx: &mut Context<Self>) {
        if let Some(split) = &mut self.split {
            let ratio = split.clamp(ratio);
            if split.ratio != ratio {
                split.ratio = ratio;
                cx.notify();
            }
            if save
                && let Some(key) = &split.key
                && let Some(store) = guiraffe_layout::layout_store(cx)
            {
                store.save_splitter_sizes(
                    key,
                    &guiraffe_layout::SplitterSizes::from_pixels(&[px(ratio)]),
                );
            }
        }
    }

    fn finish_split(&mut self, cx: &mut Context<Self>) {
        if let Some(split) = &mut self.split
            && split.dragging
        {
            split.dragging = false;
            let ratio = split.ratio;
            self.resize_split(ratio, true, cx);
        }
    }

    fn fit_split(&mut self, cx: &mut Context<Self>) {
        let Some(split) = &self.split else { return };
        let longest = self
            .window()
            .filter_map(|display| {
                self.model_rank(display)
                    .and_then(|rank| self.model.row(rank))
            })
            .map(|row| {
                let label = self.source.label(row.id);
                44 + row.depth.min(45) * 16 + label.chars().take(256).count() * 8
            })
            .max()
            .unwrap_or(96);
        self.resize_split(longest as f32 / split.width.get().max(1.), true, cx);
    }

    fn open_colour_picker(&mut self, id: NodeId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Property {
            value: Value::Colour(rgb),
            read_only: false,
            ..
        }) = self.source.property(id)
        else {
            return;
        };
        let rgb = *rgb;
        let state =
            cx.new(|cx| ColorPickerState::new(window, cx).default_value(gpui_kit::rgb(rgb)));
        state.update(cx, |state, cx| state.set_open(true, cx));
        let subscription = cx.subscribe(&state, move |this, _, event, cx| {
            if let ColorPickerEvent::Change(Some(colour)) = event {
                let colour = gpui_kit::Rgba::from(*colour);
                let rgb = |channel: f32| (channel.clamp(0., 1.) * 255.).round() as u32;
                this.edit_value(
                    id,
                    Value::Colour(rgb(colour.r) << 16 | rgb(colour.g) << 8 | rgb(colour.b)),
                    cx,
                );
            }
        });
        self.colour_picker = Some((id, state, subscription));
        cx.notify();
    }

    fn open_slider(&mut self, id: NodeId, cx: &mut Context<Self>) {
        let Some(Property {
            value: Value::Number(number),
            read_only: false,
            ..
        }) = self.source.property(id)
        else {
            return;
        };
        let Some((min, max, step)) = self.source.property_options(id).slider_range() else {
            return;
        };
        let number = (*number as f32).clamp(min, max);
        let state = cx.new(|_| {
            SliderState::new()
                .min(min)
                .max(max)
                .step(step)
                .default_value(number)
        });
        let subscription = cx.subscribe(&state, move |this, _, event, cx| {
            if let SliderEvent::Release(SliderValue::Single(value)) = event {
                this.edit_value(id, Value::Number(f64::from(*value)), cx);
            }
        });
        self.slider = Some((id, state, subscription));
        cx.notify();
    }

    fn dragging_property(&self) -> bool {
        self.scrub.is_some() || self.split.as_ref().is_some_and(|split| split.dragging)
    }

    fn move_property_drag(&mut self, x: f32, cx: &mut Context<Self>) {
        if let Some(scrub) = &mut self.scrub
            && let Some(value) = self
                .source
                .property_options(scrub.id)
                .scrub_value(scrub.initial, x - scrub.start)
            && scrub.preview != value
        {
            scrub.preview = value;
            cx.notify();
        }
        if let Some(split) = &self.split
            && split.dragging
        {
            self.resize_split(
                (x - self.split_origin) / split.width.get().max(1.),
                false,
                cx,
            );
        }
    }

    fn finish_property_drag(&mut self, x: Option<f32>, cx: &mut Context<Self>) {
        if let Some(scrub) = self.scrub.take() {
            if let Some(x) = x
                && let Some(value) = self
                    .source
                    .property_options(scrub.id)
                    .scrub_value(scrub.initial, x - scrub.start)
                && value != scrub.initial
            {
                self.edit_value(scrub.id, Value::Number(value), cx);
            }
            if scrub.preview != scrub.initial {
                cx.notify();
            }
        }
        self.finish_split(cx);
    }

    fn property_cell(&self, id: NodeId, row_height: f32, cx: &Context<Self>) -> Option<AnyElement> {
        let size = row_control_size(row_height);
        let property = self.source.property(id)?;
        let text = || {
            self.scrub
                .as_ref()
                .filter(|scrub| scrub.id == id)
                .map_or_else(|| property.value.text(), |scrub| scrub.preview.to_string())
        };
        let options = self.source.property_options(id);
        let editing = self.edit.as_ref().is_some_and(|edit| edit.id == id);
        let value =
            match property.value {
                _ if property.read_only => div()
                    .child(if options.mixed {
                        "—".to_owned()
                    } else {
                        property.value.text()
                    })
                    .into_any_element(),
                _ if editing => {
                    let label = self.source.label(id).to_owned();
                    match &self.edit.as_ref()?.input {
                        EditorInput::Single(input) => Input::new(input)
                            .with_size(size)
                            .aria_label(label)
                            .w_full()
                            .into_any_element(),
                        EditorInput::Components(inputs) => div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .gap(px(4.))
                            .children(inputs.iter().zip(["X", "Y", "W", "H"]).map(
                                |(input, field)| {
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .items_center()
                                        .gap(px(2.))
                                        .child(field)
                                        .child(
                                            Input::new(input)
                                                .with_size(size)
                                                .aria_label(format!("{label} {field}"))
                                                .w_full(),
                                        )
                                },
                            ))
                            .into_any_element(),
                        EditorInput::Multiline(input) => div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .child(Textarea::new(input).aria_label(label).w_full().flex_1())
                            .child(
                                Button::new(("property-text-save", id as usize))
                                    .with_size(size)
                                    .label("Save")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(Edit {
                                            input: EditorInput::Multiline(input),
                                            ..
                                        }) = &this.edit
                                        {
                                            this.commit_text(id, &input.read(cx).value(), cx);
                                        }
                                    })),
                            )
                            .into_any_element(),
                    }
                }
                _ if options.mixed => Button::new(("property-mixed", id as usize))
                    .with_size(size)
                    .tab_stop(self.tab_stop)
                    .label("—")
                    .accessibility_label(format!("{}: Mixed values", self.source.label(id)))
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.start_edit(id, window, cx)),
                    )
                    .into_any_element(),
                Value::Bool(value) => Checkbox::new(("property-bool", id as usize))
                    .with_size(size)
                    .accessibility_label(self.source.label(id).to_owned())
                    .tab_stop(self.tab_stop)
                    .checked(*value)
                    .on_change(cx.listener(move |this, value: &bool, _, cx| {
                        this.edit_value(id, Value::Bool(*value), cx)
                    }))
                    .into_any_element(),
                Value::Enum(value) => div()
                    .flex()
                    .gap(px(2.))
                    .children(property.choices.iter().map(|choice| {
                        let selected = choice == value;
                        let choice = choice.clone();
                        Button::new(choice.clone())
                            .with_size(size)
                            .tab_stop(self.tab_stop)
                            .label(choice.clone())
                            .selected(selected)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit_value(id, Value::Enum(choice.clone()), cx)
                            }))
                    }))
                    .into_any_element(),
                Value::Flags(flags) => {
                    div()
                        .flex()
                        .gap(px(4.))
                        .children(property.choices.iter().enumerate().map(
                            |(choice_index, choice)| {
                                Checkbox::new(("property-flag", choice_index))
                                    .with_size(size)
                                    .tab_stop(self.tab_stop)
                                    .label(choice.clone())
                                    .checked(flags.contains(choice))
                                    .on_change(cx.listener(move |this, checked: &bool, _, cx| {
                                        if let Some(property) = this.source.property(id)
                                            && let Value::Flags(flags) = property.value
                                            && let Some(choice) = property.choices.get(choice_index)
                                        {
                                            let mut flags = flags.clone();
                                            flags.retain(|flag| flag != choice);
                                            if *checked {
                                                flags.push(choice.clone());
                                            }
                                            this.edit_value(id, Value::Flags(flags), cx);
                                        }
                                    }))
                            },
                        ))
                        .into_any_element()
                }
                _ => {
                    if self.tab_stop {
                        Button::new(("property-value", id as usize))
                            .with_size(size)
                            .tab_stop(true)
                            .label(text())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.start_edit(id, window, cx)
                            }))
                            .into_any_element()
                    } else {
                        let text = SharedString::from(text());
                        div()
                            .id(("property-value", id as usize))
                            .test_support()
                            .role(Role::Button)
                            .aria_label(text.clone())
                            .flex_1()
                            .text_ellipsis()
                            .px(px(8.))
                            .py(px(4.))
                            .cursor_pointer()
                            .child(text)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.start_edit(id, window, cx)
                            }))
                            .into_any_element()
                    }
                }
            };
        let cell = div()
            .id(("property-cell", id as usize))
            .flex()
            .items_center()
            .gap(px(4.))
            .w(self.split.as_ref().map_or_else(
                || gpui_kit::Length::from(px(240.)),
                |split| gpui_kit::relative(1. - split.ratio).into(),
            ))
            .flex_none()
            .overflow_hidden()
            .child(value)
            .children(options.units.map(str::to_owned))
            .children(if let Value::Colour(rgb) = property.value {
                let rgb = *rgb;
                let swatch = div()
                    .flex_none()
                    .size(px(12.))
                    .rounded_sm()
                    .bg(gpui_kit::rgb(rgb));
                if let Some((_, state, _)) =
                    self.colour_picker.as_ref().filter(|(row, _, _)| *row == id)
                {
                    Some(ColorPicker::new(state).with_size(size).into_any_element())
                } else if property.read_only {
                    Some(swatch.into_any_element())
                } else {
                    Some(
                        Button::new(("property-swatch", id as usize))
                            .with_size(size)
                            .compact()
                            .tab_stop(self.tab_stop)
                            .accessibility_label("Choose colour")
                            .child(swatch)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_colour_picker(id, window, cx)
                            }))
                            .into_any_element(),
                    )
                }
            } else {
                None
            })
            .children(
                if let Value::Number(number) = property.value
                    && !property.read_only
                    && !options.mixed
                    && options.step.is_some()
                {
                    let number = *number;
                    Some(
                        div()
                            .id(("property-scrub", id as usize))
                            .test_support()
                            .aria_label("Drag to adjust value")
                            .cursor_col_resize()
                            .px(px(4.))
                            .child("↔")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(
                                    move |this, event: &gpui_kit::MouseDownEvent, _, cx| {
                                        this.scrub = Some(Scrub {
                                            id,
                                            start: f32::from(event.position.x),
                                            initial: number,
                                            preview: number,
                                        });
                                        cx.stop_propagation();
                                    },
                                ),
                            ),
                    )
                } else {
                    None
                },
            )
            .children(
                (!property.read_only
                    && matches!(property.value, Value::Number(_))
                    && options.slider_range().is_some())
                .then(|| {
                    if let Some((_, state, _)) =
                        self.slider.as_ref().filter(|(row, _, _)| *row == id)
                    {
                        return Slider::new(state)
                            .w(px(100.))
                            .accessibility_label(self.source.label(id).to_owned())
                            .into_any_element();
                    }
                    Button::new(("property-slider", id as usize))
                        .with_size(size)
                        .compact()
                        .label("Range")
                        .tab_stop(self.tab_stop)
                        .on_click(cx.listener(move |this, _, _, cx| this.open_slider(id, cx)))
                        .into_any_element()
                }),
            )
            .children(
                (!property.read_only && matches!(property.value, Value::Path(_))).then(|| {
                    Button::new(("property-path", id as usize))
                        .with_size(size)
                        .label("Browse")
                        .tab_stop(self.tab_stop)
                        .on_click(cx.listener(move |_, _, _, cx| cx.emit(TreeEvent::PickPath(id))))
                }),
            )
            .children(
                (!property.read_only && (options.mixed || property.value != property.default))
                    .then(|| {
                        let default = property.default.clone();
                        Button::new(("property-reset", id as usize))
                            .with_size(size)
                            .tab_stop(self.tab_stop)
                            .label("Reset")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit_value(id, default.clone(), cx);
                            }))
                    }),
            )
            .children(
                self.error
                    .as_ref()
                    .filter(|(row, _)| *row == id)
                    .map(|(_, error)| {
                        div()
                            .id("tree-edit-error")
                            .test_support()
                            .role(Role::Alert)
                            .aria_label(error.clone())
                            .text_ellipsis()
                            .child(error.clone())
                    }),
            );
        if self.split.is_none() {
            return Some(cell.into_any_element());
        }
        let view = cx.entity().downgrade();
        Some(
            cell.context_menu(move |menu, _, cx| {
                let Some(view) = view.upgrade() else {
                    return menu;
                };
                let this = view.read(cx);
                let Some(property) = this.source.property(id) else {
                    return menu;
                };
                let text = property.value.text();
                let mut menu =
                    menu.item(PopupMenuItem::new("Copy value").on_click(move |_, _, cx| {
                        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text.clone()));
                    }));
                if !property.read_only {
                    let value = property.default.clone();
                    menu = menu.item(PopupMenuItem::new("Reset to default").on_click(
                        move |_, _, cx| {
                            view.update(cx, |this, cx| this.edit_value(id, value.clone(), cx));
                        },
                    ));
                }
                menu
            })
            .into_any_element(),
        )
    }
}

impl<const P: bool> Render for TreeView<P> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let colours = theme.press_colors(Variant::Light, None);
        let range = self.window();
        let row_height = self.row_style.density.height();
        let offset = self.row_top(range.start) - self.scroll;
        let surfaces = theme.surfaces();
        let border = surfaces.border.to_hsla();
        let alternate = surfaces.track.to_hsla();
        let group = surfaces.raised.to_hsla();
        let sticky = self
            .row_style
            .sticky_headers
            .then(|| {
                if self.filter.is_some() {
                    return None;
                }
                let first = self.model.row(range.start)?;
                let header = if offset < 0. && self.model.expanded(first.id) {
                    first
                } else {
                    let parent = self.model.row_index(first.parent?)?;
                    self.model.row(parent)?
                };
                let next = range.clone().skip(1).find(|&rank| {
                    self.model
                        .row(rank)
                        .is_some_and(|row| row.depth <= header.depth)
                });
                let top = next.map_or(0., |rank| {
                    (self.row_top(rank) - self.scroll - row_height).min(0.)
                });
                Some((header.id, top))
            })
            .flatten();
        let hover_background = colours.hover.background.to_hsla();
        let selection_background = colours.selected.background.to_hsla();
        let rows = range.filter_map(|index| {
            let (row, siblings) = self.model.row_and_siblings(self.model_rank(index)?)?;
            let id = row.id;
            let height = (self.row_top(index + 1) - self.row_top(index)) as f32;
            let size = row_control_size(height);
            let open = self.model.expanded(id);
            let branch = self.source.children(id) != Children::Ready(&[]);
            let selected = self.model.selected(id);
            let active = self.model.cursor() == Some(id);
            let label = SharedString::from(self.source.label(id).to_owned());
            let select = cx.listener(move |this, event: &gpui_kit::ClickEvent, _, cx| {
                this.model
                    .select(id, event.modifiers().control || event.modifiers().platform);
                cx.emit(TreeEvent::Selected(id));
                cx.notify();
            });
            let mut element = div()
                .id(("tree-row", id as usize))
                .test_support()
                .role(Role::TreeItem)
                .aria_label(label.clone())
                .aria_selected(selected)
                .aria_level(row.depth + 1)
                .h(px(height))
                .w_full()
                .flex()
                .items_center()
                .gap(px(4.))
                .hover(|style| style.bg(hover_background));
            if let Some((position, count)) = siblings {
                element = element
                    .aria_position_in_set(position)
                    .aria_size_of_set(count);
            }
            if active {
                element = element.aria_active_descendant();
            }
            if branch {
                element = element.aria_expanded(open);
            }
            if self
                .filter
                .as_ref()
                .is_some_and(|filter| !filter.query.is_empty())
            {
                element = element.bg(surfaces.mark.to_hsla());
            } else if self.row_style.group_headers && branch {
                element = element
                    .bg(group)
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD);
            } else if self.row_style.alternating && index % 2 == 1 {
                element = element.bg(alternate);
            }
            if self.row_style.grid_lines {
                element = element.border_b_1().border_color(border);
            }
            if self.row_style.indent_guides && row.depth > 0 {
                element = element
                    .relative()
                    .children((0..row.depth.min(45)).map(|depth| {
                        div()
                            .absolute()
                            .left(px((depth * 16 + 13) as f32))
                            .top_0()
                            .bottom_0()
                            .w(px(1.))
                            .bg(border)
                    }));
            }
            if selected || active {
                element = element
                    .bg(selection_background)
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD);
            }
            if P && let Some(description) = self.source.property_options(id).description {
                let description = SharedString::from(description.to_owned());
                element = element.tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(description.clone())
                        .build(window, cx)
                });
            }
            if !P {
                element = element
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &gpui_kit::MouseDownEvent, _, _| {
                            this.drag = Some((id, event.position));
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, event: &gpui_kit::MouseUpEvent, _, cx| {
                            if let Some((node, start)) = this.drag.take()
                                && node != id
                                && f32::from((event.position.y - start.y).abs()) >= 4.
                                && this
                                    .model
                                    .row_index(node)
                                    .zip(this.model.row_index(id))
                                    .is_some_and(|(a, b)| {
                                        this.model
                                            .row(a)
                                            .zip(this.model.row(b))
                                            .is_some_and(|(a, b)| a.parent == b.parent)
                                    })
                            {
                                cx.emit(TreeEvent::Reorder { node, before: id });
                            }
                        }),
                    );
            }
            Some(
                element
                    .children((row.depth > 0).then(|| {
                        div()
                            .w(px((row.depth.min(45) * 16) as f32))
                            .mr(px(-4.))
                            .flex_none()
                    }))
                    .child(div().w(px(28.)).flex_none().children(branch.then(|| {
                        Button::new(("tree-expand", id as usize))
                            .with_size(size)
                            .tab_stop(false)
                            .compact()
                            .w_full()
                            .label(if open { "▾" } else { "▸" })
                            .accessibility_label(format!(
                                "{} {label}",
                                if open { "Collapse" } else { "Expand" }
                            ))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.set_expanded(id, !open, cx)),
                            )
                    })))
                    .children((!P).then(|| self.source.icon_element(id)).flatten())
                    .child(if P {
                        div()
                            .id(("tree-select", id as usize))
                            .flex_1()
                            .text_ellipsis()
                            .py(px(6.))
                            .cursor_pointer()
                            .child(label)
                            .on_click(select)
                            .into_any_element()
                    } else {
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .child(
                                Button::new(("tree-select", id as usize))
                                    .with_size(size)
                                    .tab_stop(false)
                                    .ghost()
                                    .justify_start()
                                    .accessibility_label(label.clone())
                                    .child(div().w_full().text_left().child(label))
                                    .on_click(select),
                            )
                            .into_any_element()
                    })
                    .children(
                        (!P).then(|| self.source.badge(id))
                            .flatten()
                            .map(str::to_owned),
                    )
                    .children(
                        (!P).then(|| self.source.checked(id))
                            .flatten()
                            .map(|checked| {
                                Checkbox::new(("tree-check", id as usize))
                                    .with_size(size)
                                    .accessibility_label(format!("Check {}", self.source.label(id)))
                                    .tab_stop(false)
                                    .checked(checked)
                                    .on_change(cx.listener(move |_, value: &bool, _, cx| {
                                        cx.emit(TreeEvent::Checked(id, *value))
                                    }))
                            }),
                    )
                    .children(if P {
                        self.property_cell(id, height, cx)
                    } else {
                        None
                    }),
            )
        });
        let moving = cx.weak_entity();
        let dropped = moving.clone();
        let cancelled = moving.clone();
        let mut tree = div()
            .id("virtual-tree")
            .role(Role::Tree)
            .aria_label(if P { "Properties" } else { "Tree" })
            .relative()
            .overflow_hidden()
            .h(px(self.height))
            .w_full()
            .on_mouse_move(move |event: &gpui_kit::MouseMoveEvent, _, cx| {
                if event.pressed_button == Some(MouseButton::Left)
                    && let Some(view) = moving.upgrade()
                    && view.read(cx).dragging_property()
                {
                    view.update(cx, |this, cx| {
                        this.move_property_drag(f32::from(event.position.x), cx)
                    });
                }
            })
            .on_mouse_up(
                MouseButton::Left,
                move |event: &gpui_kit::MouseUpEvent, _, cx| {
                    if let Some(view) = dropped.upgrade()
                        && view.read(cx).dragging_property()
                    {
                        view.update(cx, |this, cx| {
                            this.finish_property_drag(Some(f32::from(event.position.x)), cx)
                        });
                    }
                },
            )
            .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                if let Some(view) = cancelled.upgrade()
                    && view.read(cx).dragging_property()
                {
                    view.update(cx, |this, cx| this.finish_property_drag(None, cx));
                }
            })
            .on_scroll(cx.listener(|this, event: &ScrollEvent, _, cx| {
                let before = this.scroll;
                this.scroll -= f64::from(f32::from(event.delta.pixel_delta(px(24.)).y));
                this.clamp_scroll();
                if this.scroll != before {
                    cx.notify();
                }
                cx.stop_propagation();
            }))
            .on_key_down(
                cx.listener(|this, event: &KeyDownEvent, window, cx| this.key(event, window, cx)),
            )
            .child(
                div()
                    .absolute()
                    .w_full()
                    .top(px(offset as f32))
                    .children(rows),
            )
            .children(sticky.map(|(parent, top)| {
                div()
                    .absolute()
                    .top(px(top as f32))
                    .w_full()
                    .h(px(row_height as f32))
                    .flex()
                    .items_center()
                    .px(px(8.))
                    .bg(group)
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .text_ellipsis()
                    .child(self.source.label(parent).to_owned())
            }));
        if let Some(split) = &self.split {
            let width = split.width.clone();
            tree = tree
                .child(
                    gpui_kit::canvas(
                        move |bounds, _, _| width.set(f32::from(bounds.size.width)),
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .w_full()
                    .h_0(),
                )
                .child(
                    div()
                        .id("property-divider")
                        .test_support()
                        .role(Role::Splitter)
                        .aria_label("Resize property columns")
                        .focusable()
                        .tab_stop(self.tab_stop)
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(gpui_kit::relative(split.ratio))
                        .w(px(6.))
                        .border_l_1()
                        .border_color(border)
                        .cursor_col_resize()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &gpui_kit::MouseDownEvent, _, cx| {
                                if let Some(split) = &mut this.split {
                                    this.split_origin = f32::from(event.position.x)
                                        - split.ratio * split.width.get();
                                    split.dragging = true;
                                }
                                cx.stop_propagation();
                            }),
                        )
                        .on_click(cx.listener(|this, event: &gpui_kit::ClickEvent, _, cx| {
                            if event.click_count() == 2 {
                                this.fit_split(cx);
                            }
                        }))
                        .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                            let offset = match event.keystroke.key.as_str() {
                                "left" => -0.025,
                                "right" => 0.025,
                                "enter" => {
                                    this.fit_split(cx);
                                    cx.stop_propagation();
                                    return;
                                }
                                _ => return,
                            };
                            if let Some(split) = &this.split {
                                this.resize_split(split.ratio + offset, true, cx);
                            }
                            cx.stop_propagation();
                        })),
                );
        }
        if self.tab_stop {
            tree = tree.track_focus(&self.focus).tab_stop(true);
        }
        div()
            .w_full()
            .children(self.search.as_ref().map(|(input, _)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(input).aria_label("Filter properties").w_full()),
                    )
                    .child(
                        Checkbox::new("show-advanced-properties")
                            .label("Advanced")
                            .checked(self.show_advanced)
                            .on_change(cx.listener(|this, show: &bool, _, cx| {
                                if this.show_advanced != *show {
                                    this.expand_task = None;
                                    this.show_advanced = *show;
                                    let query = this
                                        .filter
                                        .as_ref()
                                        .map_or_else(String::new, |filter| filter.query.clone());
                                    this.filter_rows(query, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("expand-properties")
                            .label("Expand all")
                            .on_click(cx.listener(|this, _, _, cx| this.expand_loaded(cx))),
                    )
                    .child(
                        Button::new("collapse-properties")
                            .label("Collapse all")
                            .on_click(cx.listener(|this, _, _, cx| this.collapse_all(cx))),
                    )
            }))
            .children(
                self.filter
                    .as_ref()
                    .filter(|filter| filter.pending)
                    .map(|_| div().child("Filtering properties…")),
            )
            .child(focus_ring(
                div().w_full().child(tree),
                self.tab_stop && ring_visible(&self.focus, window),
                px(0.),
                colours.ring,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{
        Modifiers, ScrollDelta, ScrollSource, StyleRefinement, TestAppContext, TouchPhase, point,
    };
    use std::cell::{Cell, RefCell};

    struct Data(Vec<NodeId>, Value, Value);
    impl TreeSource for Data {
        fn roots(&self) -> &[NodeId] {
            &[0]
        }
        fn children(&self, id: NodeId) -> Children<'_> {
            Children::Ready(if id == 0 { &self.0 } else { &[] })
        }
        fn label(&self, id: NodeId) -> &str {
            if id == 0 { "Settings" } else { "Property" }
        }
    }
    impl TreeData for Data {
        fn checked(&self, id: NodeId) -> Option<bool> {
            (id > 0).then_some(false)
        }
        fn property(&self, id: NodeId) -> Option<Property<'_>> {
            (id > 0).then_some(Property {
                value: &self.1,
                default: &self.2,
                choices: &[],
                read_only: id == 2,
            })
        }
        fn property_options(&self, id: NodeId) -> PropertyOptions<'_> {
            PropertyOptions {
                step: (id == 1 && matches!(self.1, Value::Number(_))).then_some(1.),
                ..Default::default()
            }
        }
        fn validate(&self, _: NodeId, value: &Value) -> Result<(), String> {
            if matches!(value, Value::Number(n) if *n < 0.) {
                Err("Out of range".into())
            } else {
                Ok(())
            }
        }
    }

    struct Host(Entity<PropertyGrid>, Rc<Cell<usize>>);
    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.1.set(self.1.get() + 1);
            div().size_full().child(
                self.0
                    .clone()
                    .cached(StyleRefinement::default().w_full().h(px(320.))),
            )
        }
    }

    struct Container(Entity<Host>);
    impl Render for Container {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                self.0
                    .clone()
                    .cached(StyleRefinement::default().size_full()),
            )
        }
    }

    #[gpui_kit::test]
    fn hidden_and_unchanged_expansion_do_not_notify(cx: &mut TestAppContext) {
        let notifications = Rc::new(Cell::new(0));
        let tree = cx.new(|cx| {
            let count = notifications.clone();
            cx.observe_self(move |_, _| count.set(count.get() + 1))
                .detach();
            TreeList::new(
                Rc::new(Data(vec![1], Value::Number(10.), Value::Number(5.))),
                cx,
            )
        });

        tree.update(cx, |tree, cx| tree.set_expanded(1, true, cx));
        assert_eq!(notifications.get(), 0);
        tree.update(cx, |tree, cx| tree.set_expanded(0, true, cx));
        assert_eq!(notifications.get(), 1);
        tree.update(cx, |tree, cx| tree.set_expanded(0, true, cx));
        assert_eq!(notifications.get(), 1);
    }

    #[gpui_kit::test]
    fn keyboard_expand_scroll_and_validation(cx: &mut TestAppContext) {
        use gpui_kit::test::TestQueryExt;

        cx.update(|cx| {
            gpui_kit::base::init(cx);
            gpui_kit::component::init(cx);
            guiraffe_core::apply(Default::default(), cx);
        });
        let renders = Rc::new(Cell::new(0));
        let events = Rc::new(RefCell::new(Vec::new()));
        let view = cx.new(|cx| {
            PropertyGrid::new(
                Rc::new(Data(
                    (1..50).collect(),
                    Value::Number(10.),
                    Value::Number(5.),
                )),
                cx,
            )
            .focusable(true)
        });
        let (_, window) = cx.add_window_view(|window, cx| {
            let events = events.clone();
            cx.subscribe(&view, move |_, _, event: &TreeEvent, _| {
                events.borrow_mut().push(event.clone())
            })
            .detach();
            let host = cx.new(|_| Host(view.clone(), renders.clone()));
            let container = cx.new(|_| Container(host));
            gpui_kit::base::Root::new(container, window, cx)
        });
        window.run_until_parked();
        window.update(|window, cx| {
            let focus = view.read(cx).focus.clone();
            window.focus(&focus, cx);
            window.draw(cx).clear(cx);
        });
        window.run_until_parked();
        window.simulate_keystrokes("home");
        window.run_until_parked();
        let initial = renders.get();
        window.simulate_keystrokes("right down space");
        window.run_until_parked();
        window.update(|window, _| {
            let rows = window.find_all_by_role(Role::TreeItem);
            assert!(
                rows.iter()
                    .any(|row| { row.label() == Some("Settings") && row.expanded() == Some(true) })
            );
            assert!(
                rows.iter()
                    .any(|row| { row.label() == Some("Property") && row.selected() == Some(true) })
            );
        });
        window.simulate_keystrokes("end");
        window.run_until_parked();
        assert_eq!(
            view.read_with(window, |view, _| view.model.rows().len()),
            50
        );
        assert_eq!(
            view.read_with(window, |view, _| view.model.cursor()),
            Some(49)
        );
        assert!(events.borrow().contains(&TreeEvent::Selected(1)));
        // Upstream GPUI marks the ancestor path dirty for descendant updates.
        if gpui_kit::GPUI_FAST {
            assert_eq!(
                renders.get(),
                initial,
                "expand and navigation invalidated host"
            );
        }
        let before = view.read_with(window, |view, _| view.scroll);
        let position = point(px(15.), px(120.));
        window.simulate_mouse_move(position, None, Modifiers::default());
        window.run_until_parked();
        let initial = renders.get();
        window.simulate_event(ScrollEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(0.), px(100.))),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
            source: ScrollSource::Pointer,
        });
        window.run_until_parked();
        assert!(view.read_with(window, |view, _| view.scroll) < before);
        if gpui_kit::GPUI_FAST {
            assert_eq!(renders.get(), initial, "scroll invalidated host");
        }
        window.simulate_keystrokes("home down enter");
        window.run_until_parked();
        let input = view
            .read_with(window, |view, _| {
                view.edit.as_ref().and_then(|edit| match &edit.input {
                    EditorInput::Single(input) => Some(input.clone()),
                    EditorInput::Multiline(_) | EditorInput::Components(_) => None,
                })
            })
            .expect("Enter opens the number editor");
        window.update(|window, cx| input.update(cx, |input, cx| input.set_value("-1", window, cx)));
        window.simulate_keystrokes("enter");
        window.run_until_parked();
        assert!(view.read_with(window, |view, _| view.error.is_some()));
        window.update(|window, _| {
            assert_eq!(
                window.find_by_label("Out of range").role(),
                Some(Role::Alert)
            );
        });
        window.update(|window, cx| input.update(cx, |input, cx| input.set_value("5", window, cx)));
        window.simulate_keystrokes("enter");
        window.run_until_parked();
        assert!(view.read_with(window, |view, _| view.error.is_none()));
        window.simulate_keystrokes("down enter");
        window.run_until_parked();
        assert!(view.read_with(window, |view, _| view.edit.is_none()));
        assert!(
            events
                .borrow()
                .contains(&TreeEvent::Edited(1, Value::Number(5.)))
        );
        assert!(!events.borrow().iter().any(|e| matches!(
            e,
            TreeEvent::Edited(2, _) | TreeEvent::Edited(1, Value::Number(-1.))
        )));
    }
    #[gpui_kit::test]
    fn tree_pointer_selection_check_and_reorder(cx: &mut TestAppContext) {
        use gpui_kit::test::{TestQueryExt, TestWindowExt};
        cx.update(|cx| {
            gpui_kit::base::init(cx);
            gpui_kit::component::init(cx);
            guiraffe_core::apply(Default::default(), cx);
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let view = cx.new(|cx| {
            TreeList::new(
                Rc::new(Data(vec![1, 2], Value::Bool(false), Value::Bool(false))),
                cx,
            )
        });
        let (_, window) = cx.add_window_view(|window, cx| {
            let events = events.clone();
            cx.subscribe(&view, move |_, _, event: &TreeEvent, _| {
                events.borrow_mut().push(event.clone())
            })
            .detach();
            gpui_kit::base::Root::new(view.clone(), window, cx)
        });
        window.run_until_parked();
        let expand =
            window.update(|window, _| window.find_by_label("Expand Settings").bounds().center());
        window.simulate_click(expand, Modifiers::default());
        window.run_until_parked();
        assert!(view.read_with(window, |view, _| view.model.expanded(0)));

        let from = expand + point(px(75.), px(ROW as f32));
        let to = from + point(px(0.), px(ROW as f32));
        window.simulate_click(from, Modifiers::default());
        window.simulate_click(
            to,
            Modifiers {
                control: true,
                ..Default::default()
            },
        );
        window.run_until_parked();
        assert!(view.read_with(window, |view, _| view.model.selected(1)
            && view.model.selected(2)));
        window.update(|window, cx| assert!(window.focused(cx).is_none()));

        window.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
        window.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
        window.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
        window.run_until_parked();
        assert!(
            events
                .borrow()
                .contains(&TreeEvent::Reorder { node: 1, before: 2 })
        );

        let check =
            window.update(|window, _| window.find(("tree-check", 1usize)).bounds().center());
        window.simulate_click(check, Modifiers::default());
        window.run_until_parked();
        assert!(events.borrow().contains(&TreeEvent::Checked(1, true)));
    }
    #[gpui_kit::test]
    fn divider_resizes_and_restores_from_layout_store(cx: &mut TestAppContext) {
        use gpui_kit::test::TestQueryExt;
        cx.update(|cx| {
            gpui_kit::base::init(cx);
            gpui_kit::component::init(cx);
            guiraffe_layout::init(cx);
        });
        let data = Rc::new(Data(vec![1], Value::Number(10.), Value::Number(5.)));
        let view = cx.new(|cx| {
            PropertyGrid::new(data.clone(), cx)
                .focusable(true)
                .split_ratio(0.5)
                .persist_key("divider-test", cx)
        });
        let (_, window) =
            cx.add_window_view(|window, cx| gpui_kit::base::Root::new(view.clone(), window, cx));
        window.run_until_parked();
        let divider = window.update(|window, _| {
            let divider = window.find_by_label("Resize property columns");
            assert_eq!(divider.role(), Some(Role::Splitter));
            divider.bounds().center()
        });
        let end = divider + point(px(60.), px(0.));
        window.simulate_mouse_down(divider, MouseButton::Left, Modifiers::default());
        window.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        window.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        window.run_until_parked();
        let moved = view.read_with(window, |view, _| view.split.as_ref().map(|s| s.ratio));
        assert!(moved.is_some_and(|ratio| ratio > 0.5));
        let restored = cx.new(|cx| PropertyGrid::new(data, cx).persist_key("divider-test", cx));
        assert_eq!(
            restored.read_with(cx, |view, _| view.split.as_ref().map(|s| s.ratio)),
            moved
        );
    }

    #[gpui_kit::test]
    fn number_scrub_previews_until_pointer_release(cx: &mut TestAppContext) {
        use gpui_kit::test::TestWindowExt;
        cx.update(|cx| {
            gpui_kit::base::init(cx);
            gpui_kit::component::init(cx);
            guiraffe_core::apply(Default::default(), cx);
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let view = cx.new(|cx| {
            let mut grid = PropertyGrid::new(
                Rc::new(Data(vec![1], Value::Number(10.), Value::Number(10.))),
                cx,
            );
            grid.set_expanded(0, true, cx);
            grid
        });
        let (_, window) = cx.add_window_view(|window, cx| {
            let events = events.clone();
            cx.subscribe(&view, move |_, _, event: &TreeEvent, _| {
                events.borrow_mut().push(event.clone())
            })
            .detach();
            gpui_kit::base::Root::new(view.clone(), window, cx)
        });
        window.run_until_parked();
        let grip =
            window.update(|window, _| window.find(("property-scrub", 1usize)).bounds().center());
        let end = grip + point(px(12.), px(0.));
        window.simulate_mouse_down(grip, MouseButton::Left, Modifiers::default());
        window.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        window.run_until_parked();
        assert_eq!(
            view.read_with(window, |view, _| view
                .scrub
                .as_ref()
                .map(|scrub| scrub.preview)),
            Some(13.)
        );
        assert!(events.borrow().is_empty());
        window.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        window.run_until_parked();
        assert!(
            events
                .borrow()
                .contains(&TreeEvent::Edited(1, Value::Number(13.)))
        );
    }

    #[gpui_kit::test]
    fn filter_replaces_pending_work_and_preserves_expansion(cx: &mut TestAppContext) {
        let view = cx.new(|cx| {
            PropertyGrid::new(
                Rc::new(Data(
                    (1..=800).collect(),
                    Value::Number(10.),
                    Value::Number(5.),
                )),
                cx,
            )
        });
        view.update(cx, |view, cx| {
            view.set_expanded(0, true, cx);
            view.filter_rows("property".to_owned(), cx);
            view.filter_rows("settings".to_owned(), cx);
        });
        cx.run_until_parked();
        for _ in 0..3 {
            cx.background_executor
                .advance_clock(std::time::Duration::from_millis(2));
            cx.run_until_parked();
        }
        assert_eq!(view.read_with(cx, |view, _| view.row_count()), 1);
        assert_eq!(view.read_with(cx, |view, _| view.model_rank(0)), Some(0));
        view.update(cx, |view, cx| view.filter_rows(String::new(), cx));
        assert_eq!(view.read_with(cx, |view, _| view.row_count()), 801);
    }

    struct Editors {
        rows: [(Value, Value); 4],
        choices: [String; 2],
    }

    impl TreeSource for Editors {
        fn roots(&self) -> &[NodeId] {
            &[0, 1, 2, 3]
        }
        fn children(&self, _: NodeId) -> Children<'_> {
            Children::Ready(&[])
        }
        fn label(&self, id: NodeId) -> &str {
            ["Name", "Enabled", "Mode", "Accent"][id as usize]
        }
    }

    impl TreeData for Editors {
        fn property(&self, id: NodeId) -> Option<Property<'_>> {
            let (value, default) = self.rows.get(id as usize)?;
            Some(Property {
                value,
                default,
                choices: &self.choices,
                read_only: false,
            })
        }
    }

    #[gpui_kit::test]
    fn property_controls_emit_typed_edits_and_reset(cx: &mut TestAppContext) {
        use gpui_kit::test::{TestQueryExt, TestWindowExt};
        cx.update(|cx| {
            gpui_kit::base::init(cx);
            gpui_kit::component::init(cx);
            guiraffe_core::apply(Default::default(), cx);
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let source = Editors {
            rows: [
                (Value::Text("Current".into()), Value::Text("Default".into())),
                (Value::Bool(false), Value::Bool(false)),
                (Value::Enum("Auto".into()), Value::Enum("Auto".into())),
                (Value::Colour(0x123456), Value::Colour(0x123456)),
            ],
            choices: ["Auto".into(), "Manual".into()],
        };
        let view = cx.new(|cx| PropertyGrid::new(Rc::new(source), cx));
        let (_, window) = cx.add_window_view(|window, cx| {
            let events = events.clone();
            cx.subscribe(&view, move |_, _, event: &TreeEvent, _| {
                events.borrow_mut().push(event.clone())
            })
            .detach();
            gpui_kit::base::Root::new(view.clone(), window, cx)
        });
        window.run_until_parked();

        let checkbox =
            window.update(|window, _| window.find(("property-bool", 1usize)).bounds().center());
        window.simulate_click(checkbox, Modifiers::default());
        window.run_until_parked();
        assert!(
            events
                .borrow()
                .contains(&TreeEvent::Edited(1, Value::Bool(true)))
        );

        for (label, id, value) in [
            ("Manual", 2, Value::Enum("Manual".into())),
            ("Reset", 0, Value::Text("Default".into())),
        ] {
            let position = window.update(|window, _| window.find_by_label(label).bounds().center());
            window.simulate_click(position, Modifiers::default());
            window.run_until_parked();
            assert!(events.borrow().contains(&TreeEvent::Edited(id, value)));
        }

        for (label, text, id, value) in [
            ("Current", "Updated", 0, Value::Text("Updated".into())),
            ("#123456", "#abcdef", 3, Value::Colour(0xabcdef)),
        ] {
            let position = window.update(|window, _| window.find_by_label(label).bounds().center());
            window.simulate_click(position, Modifiers::default());
            window.run_until_parked();
            let input = view
                .read_with(window, |view, _| {
                    view.edit.as_ref().and_then(|edit| match &edit.input {
                        EditorInput::Single(input) => Some(input.clone()),
                        EditorInput::Multiline(_) | EditorInput::Components(_) => None,
                    })
                })
                .expect("value click opens the editor");
            window.update(|window, cx| {
                input.update(cx, |input, cx| input.set_value(text, window, cx))
            });
            window.simulate_keystrokes("enter");
            window.run_until_parked();
            assert!(events.borrow().contains(&TreeEvent::Edited(id, value)));
        }
    }
}
