/// Compact user card.
#[derive(Clone, PartialEq)]
pub struct UserCard
where
    bool: ::core::default::Default,
{
    name: SharedString,
    compact: bool,
}
impl UserCard
where
    bool: ::core::default::Default,
{
    pub fn new(name: SharedString) -> Self {
        Self {
            name,
            compact: ::core::default::Default::default(),
        }
    }
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }
    fn __guiraffe_render(name: SharedString, compact: bool) -> impl IntoElement
    where
        bool: ::core::default::Default,
    {
        ui! {
            div { { name } if compact { "Compact" } }
        }
    }
}
impl ::gpui_kit::RenderOnce for UserCard
where
    bool: ::core::default::Default,
    Self: 'static,
{
    fn render(
        self,
        _: &mut ::gpui_kit::Window,
        _: &mut ::gpui_kit::App,
    ) -> impl ::gpui_kit::IntoElement {
        Self::__guiraffe_render(self.name, self.compact)
    }
}
impl ::gpui_kit::IntoElement for UserCard
where
    bool: ::core::default::Default,
    Self: 'static,
{
    type Element = ::gpui_kit::ViewElement<Self>;
    fn into_element(self) -> Self::Element {
        ::gpui_kit::ViewElement::new(self)
    }
}
