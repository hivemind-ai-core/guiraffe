#[doc = concat!("State of [`", stringify!(Counter), "`].")]
#[derive(Clone, PartialEq, Debug, Default)]
pub struct CounterFields {
    pub count: i32,
}
#[doc(hidden)]
#[derive(Clone)]
pub struct __CounterProps {
    step: i32,
}
#[doc(hidden)]
#[derive(Clone, Default)]
pub struct __CounterControlled {
    count: ::core::option::Option<i32>,
    on_count_change: ::core::option::Option<
        ::std::rc::Rc<
            dyn ::core::ops::Fn(&i32, &mut ::gpui_kit::Window, &mut ::gpui_kit::App),
        >,
    >,
}
pub struct Counter {
    id: ::guiraffe_ui::__macro::SharedString,
    props: __CounterProps,
    initial: CounterFields,
    controlled: __CounterControlled,
}
#[doc = concat!(
    "The entity behind a [`", stringify!(Counter),
    "`]: owns its state, and is observed and driven over the action bus at the component's id."
)]
pub struct CounterState {
    fields: CounterFields,
    props: ::core::option::Option<__CounterProps>,
    controlled: __CounterControlled,
    registered: bool,
}
impl Counter {
    pub fn new(
        id: impl ::core::convert::Into<::guiraffe_ui::__macro::SharedString>,
        step: i32,
    ) -> Self {
        Self {
            id: id.into(),
            props: __CounterProps { step },
            initial: ::core::default::Default::default(),
            controlled: ::core::default::Default::default(),
        }
    }
    /// Controlled value: the caller owns it and receives changes through the callback.
    pub fn count(mut self, count: i32) -> Self {
        self.controlled.count = ::core::option::Option::Some(count);
        self
    }
    /// Initial value of the component-owned (uncontrolled) state.
    pub fn default_count(mut self, count: i32) -> Self {
        self.initial.count = count;
        self
    }
    pub fn on_count_change(
        mut self,
        callback: impl ::core::ops::Fn(
            &i32,
            &mut ::gpui_kit::Window,
            &mut ::gpui_kit::App,
        ) + 'static,
    ) -> Self {
        self.controlled.on_count_change = ::core::option::Option::Some(
            ::std::rc::Rc::new(callback),
        );
        self
    }
    #[allow(clippy::too_many_arguments)]
    fn __guiraffe_render(
        __guiraffe_state: ::guiraffe_ui::__macro::StateHandle<CounterState>,
        count: i32,
        step: i32,
    ) -> impl IntoElement {
        ui! {
            div { { count } }
        }
    }
}
impl CounterState {
    /// The current state.
    pub fn fields(&self) -> &CounterFields {
        &self.fields
    }
}
impl ::guiraffe_ui::__macro::Stateful for CounterState {
    type Fields = CounterFields;
    fn fields(&self) -> &CounterFields {
        &self.fields
    }
    fn commit(
        &mut self,
        next: CounterFields,
        window: &mut ::gpui_kit::Window,
        cx: &mut ::gpui_kit::Context<Self>,
    ) {
        let mut changed = false;
        if next.count != self.fields.count {
            if self.controlled.count.is_some() {
                if let ::core::option::Option::Some(callback) = self
                    .controlled
                    .on_count_change
                    .clone()
                {
                    callback(&next.count, window, cx);
                }
            } else {
                self.fields.count = next.count;
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    }
}
impl ::gpui_kit::Render for CounterState {
    fn render(
        &mut self,
        _: &mut ::gpui_kit::Window,
        cx: &mut ::gpui_kit::Context<Self>,
    ) -> impl ::gpui_kit::IntoElement {
        let ::core::option::Option::Some(props) = ::core::clone::Clone::clone(
            &self.props,
        ) else {
            return ::gpui_kit::IntoElement::into_any_element(::gpui_kit::Empty);
        };
        let CounterFields { count } = ::core::clone::Clone::clone(&self.fields);
        let __CounterProps { step } = props;
        ::gpui_kit::IntoElement::into_any_element(
            Counter::__guiraffe_render(
                ::guiraffe_ui::__macro::StateHandle::new(cx.weak_entity()),
                count,
                step,
            ),
        )
    }
}
impl ::guiraffe_ui::__macro::Observe for CounterState {
    type Snapshot = ::guiraffe_ui::__macro::ControlSnapshot;
    fn snapshot(&self, _: &::gpui_kit::App) -> ::guiraffe_ui::__macro::ControlSnapshot {
        ::guiraffe_ui::__macro::ControlSnapshot {
            value: ::core::option::Option::Some(
                ::guiraffe_ui::__macro::StateValue::to_value(&self.fields.count),
            ),
            ..::core::default::Default::default()
        }
    }
}
impl ::guiraffe_ui::__macro::Drive for CounterState {
    type Action = ::guiraffe_ui::__macro::Value;
    type Output = ();
    fn decode(
        action: &::guiraffe_ui::__macro::Action,
    ) -> ::core::result::Result<
        ::guiraffe_ui::__macro::Value,
        ::guiraffe_ui::__macro::ActionError,
    > {
        if action.name == ::guiraffe_ui::__macro::names::SET_VALUE {
            ::core::result::Result::Ok(::core::clone::Clone::clone(&action.payload))
        } else {
            ::core::result::Result::Err(::guiraffe_ui::__macro::ActionError::Unsupported)
        }
    }
    fn apply(
        &mut self,
        action: ::guiraffe_ui::__macro::Value,
        window: &mut ::gpui_kit::Window,
        cx: &mut ::gpui_kit::Context<Self>,
    ) -> ::core::result::Result<(), ::guiraffe_ui::__macro::ActionError> {
        let mut next = ::core::clone::Clone::clone(&self.fields);
        next.count = <i32 as ::guiraffe_ui::__macro::StateValue>::from_value(&action)
            .ok_or(
                ::guiraffe_ui::__macro::ActionError::Invalid("value has the wrong type"),
            )?;
        ::guiraffe_ui::__macro::Stateful::commit(self, next, window, cx);
        ::core::result::Result::Ok(())
    }
}
impl ::gpui_kit::RenderOnce for Counter {
    fn render(
        self,
        window: &mut ::gpui_kit::Window,
        cx: &mut ::gpui_kit::App,
    ) -> impl ::gpui_kit::IntoElement {
        let Self { id, props, initial, controlled } = self;
        let entity = window
            .use_keyed_state(
                ::gpui_kit::ElementId::Name(::core::clone::Clone::clone(&id)),
                cx,
                move |_, _| CounterState {
                    fields: initial,
                    props: ::core::option::Option::None,
                    controlled: ::core::default::Default::default(),
                    registered: false,
                },
            );
        let register = entity
            .update(
                cx,
                |state, _| {
                    if let ::core::option::Option::Some(value) = &controlled.count {
                        if *value != state.fields.count {
                            state.fields.count = ::core::clone::Clone::clone(value);
                        }
                    }
                    state.props = ::core::option::Option::Some(props);
                    state.controlled = controlled;
                    !::core::mem::replace(&mut state.registered, true)
                },
            );
        if register {
            ::guiraffe_ui::__macro::ActionBus::register(
                &id,
                &entity,
                "Counter",
                ::guiraffe_ui::__macro::Capabilities::SET_VALUE,
                window,
                cx,
            );
        }
        ::gpui_kit::AnyView::from(entity)
    }
}
impl ::gpui_kit::IntoElement for Counter {
    type Element = ::gpui_kit::ViewElement<Self>;
    fn into_element(self) -> Self::Element {
        ::gpui_kit::ViewElement::new(self)
    }
}
