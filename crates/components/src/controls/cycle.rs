//! A toolbar button for cycling through a short ordered list.
use super::*;
use gpui_kit::ClickEvent;

/// A stable wire value, its visible label, and its accessible description.
#[derive(Clone)]
pub struct CycleOption {
    value: SharedString,
    label: SharedString,
    description: SharedString,
}

impl CycleOption {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        Self {
            value: value.into(),
            description: label.clone(),
            label,
        }
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = description.into();
        self
    }
}

#[derive(Default)]
pub struct CycleBuilder {
    options: Vec<CycleOption>,
    initial: Option<SharedString>,
    path: Option<SharedString>,
    name: Option<SharedString>,
    tooltip: Option<SharedString>,
    size: ControlSize,
    variant: ButtonVariant,
    disabled: bool,
}

impl CycleBuilder {
    /// Preserve caller order. For duplicate wire values the first option wins.
    pub fn options(mut self, options: impl IntoIterator<Item = CycleOption>) -> Self {
        self.options.clear();
        for option in options {
            if !self.options.iter().any(|old| old.value == option.value) {
                self.options.push(option);
            }
        }
        self
    }
    /// Start at this value, or the first option when the value is absent.
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.initial = Some(value.into());
        self
    }
    pub fn path(mut self, path: impl Into<SharedString>) -> Self {
        self.path = Some(path.into());
        self
    }
    pub fn element_id(mut self, id: impl Into<SharedString>) -> Self {
        self.name = Some(id.into());
        self
    }
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
    pub fn small(mut self) -> Self {
        self.size = ControlSize::Small;
        self
    }
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn build(self, window: &mut Window, cx: &mut App) -> Entity<CycleControl> {
        self.build_with(Some(window), cx)
    }
    pub fn build_later(self, cx: &mut App) -> Entity<CycleControl> {
        self.build_with(None, cx)
    }
    fn build_with(self, window: Option<&mut Window>, cx: &mut App) -> Entity<CycleControl> {
        let index = self
            .options
            .iter()
            .position(|o| Some(&o.value) == self.initial.as_ref())
            .unwrap_or(0);
        build(
            "cycle",
            Capabilities::NEXT | Capabilities::PREVIOUS | Capabilities::SET_VALUE,
            self.path,
            self.name,
            window,
            cx,
            |identity, _, _| CycleControl {
                identity,
                options: self.options,
                index,
                tooltip: self.tooltip,
                size: self.size,
                variant: self.variant,
                disabled: self.disabled,
            },
        )
    }
}

/// Click or activate with Enter/Space for the next option. Right-click for the previous one.
/// An empty list is disabled. A singleton can be activated but never changes.
pub struct CycleControl {
    identity: Identity,
    options: Vec<CycleOption>,
    index: usize,
    tooltip: Option<SharedString>,
    size: ControlSize,
    variant: ButtonVariant,
    disabled: bool,
}

identified!(CycleControl);

impl CycleControl {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> CycleBuilder {
        CycleBuilder::default()
    }

    pub fn value(&self) -> Option<&SharedString> {
        self.options.get(self.index).map(|o| &o.value)
    }

    /// Host write. Unknown values return an error and leave the selection intact.
    pub fn set_value(&mut self, value: &str, cx: &mut Context<Self>) -> Result<(), ActionError> {
        let index = resolve(
            &self.options,
            self.index,
            &CycleAction::Set(value.to_owned().into()),
        )?;
        if self.index != index {
            self.index = index;
            cx.notify();
        }
        Ok(())
    }
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if self.disabled != disabled {
            self.disabled = disabled;
            cx.notify();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CycleEvent {
    Changed(SharedString, Source),
}

pub enum CycleAction {
    Next,
    Previous,
    Set(SharedString),
}

fn resolve(
    options: &[CycleOption],
    index: usize,
    action: &CycleAction,
) -> Result<usize, ActionError> {
    if options.is_empty() {
        return Err(ActionError::Disabled);
    }
    match action {
        CycleAction::Next => Ok((index + 1) % options.len()),
        CycleAction::Previous => Ok(index.checked_sub(1).unwrap_or(options.len() - 1)),
        CycleAction::Set(value) => options
            .iter()
            .position(|o| o.value == *value)
            .ok_or(ActionError::Invalid("unknown cycle value")),
    }
}

impl EventEmitter<CycleEvent> for CycleControl {}
impl Observe for CycleControl {
    type Snapshot = ControlSnapshot;
    fn snapshot(&self, _: &App) -> ControlSnapshot {
        ControlSnapshot {
            value: self.value().cloned().map(Value::Str),
            label: self
                .options
                .get(self.index)
                .map(|o| o.label.as_ref().into()),
            disabled: Some(self.disabled || self.options.is_empty()),
            ..Default::default()
        }
    }
}

impl Drive for CycleControl {
    type Action = CycleAction;
    type Output = Outcome;
    fn decode(action: &Action) -> Result<CycleAction, ActionError> {
        match (action.name.as_ref(), &action.payload) {
            (names::NEXT, Value::Null) => Ok(CycleAction::Next),
            (names::PREVIOUS, Value::Null) => Ok(CycleAction::Previous),
            (names::SET_VALUE, Value::Str(value)) => Ok(CycleAction::Set(value.clone())),
            (names::NEXT | names::PREVIOUS, _) => {
                Err(ActionError::Invalid("cycle step expects null"))
            }
            (names::SET_VALUE, _) => Err(ActionError::Invalid("set_value expects a string")),
            _ => Err(ActionError::Unsupported),
        }
    }
    fn apply(
        &mut self,
        action: CycleAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Outcome, ActionError> {
        enabled(self.disabled)?;
        let next = resolve(&self.options, self.index, &action)?;
        let value = self.options[next].value.clone();
        if self.index != next {
            self.index = next;
            cx.emit(CycleEvent::Changed(value.clone(), self.identity.source(cx)));
            cx.notify();
        }
        Ok(Outcome(Value::Str(value)))
    }
}

impl Render for CycleControl {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        mount(self, window, cx);
        let mut button = Button::new(self.element_id())
            .with_size(self.size.kit())
            .with_variant(self.variant)
            .disabled(self.disabled || self.options.is_empty());
        if let Some(option) = self.options.get(self.index) {
            button = button
                .label(option.label.clone())
                .aria_description(option.description.clone());
        }
        if let Some(tooltip) = &self.tooltip {
            let text = self.options.get(self.index).map_or_else(
                || tooltip.clone(),
                |option| format!("{} — {}", option.description, tooltip).into(),
            );
            button = button.tooltip(text);
        }
        let this = cx.entity().downgrade();
        if !self.disabled && !self.options.is_empty() {
            let previous = this.clone();
            button
                .interactivity()
                .on_aux_click(move |event, window, cx| {
                    if event.is_right_click() {
                        cx.stop_propagation();
                        route(
                            &previous,
                            action(names::PREVIOUS, Value::Null),
                            Source::Pointer,
                            window,
                            cx,
                        )
                        .ok();
                    }
                });
        }
        button.on_click(move |event, window, cx| {
            let source = if matches!(event, ClickEvent::Keyboard(_)) {
                Source::Keyboard
            } else {
                Source::Pointer
            };
            route(&this, action(names::NEXT, Value::Null), source, window, cx).ok();
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn steps_wrap_and_unknown_values_are_rejected() {
        let options = ["4", "8", "16"].map(|v| CycleOption::new(v, v));
        assert_eq!(resolve(&options, 2, &CycleAction::Next), Ok(0));
        assert_eq!(resolve(&options, 0, &CycleAction::Previous), Ok(2));
        assert_eq!(resolve(&options[..1], 0, &CycleAction::Previous), Ok(0));
        assert_eq!(
            resolve(&[], 0, &CycleAction::Next),
            Err(ActionError::Disabled)
        );
        assert!(resolve(&options, 1, &CycleAction::Set("32".into())).is_err());
    }
}
