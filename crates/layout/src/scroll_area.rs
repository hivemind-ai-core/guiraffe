//! A scrollable region.

use crate::gpui::IntoElement;

use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, ParentElement as _, RenderOnce,
    ScrollHandle, StatefulInteractiveElement as _, StyleRefinement, Styled as _, Window, div,
};

/// A region whose content scrolls when it does not fit.
///
/// A scroll area scrolls down by default; [`ScrollArea::both`] scrolls in both
/// directions.
///
/// The id has to stay the same across renders and has to be unique among the
/// element's siblings: it is what the scroll position is remembered by.
///
/// ```
/// use guiraffe_layout::ScrollArea;
///
/// let area = ScrollArea::new("list").child("a long list");
/// ```
#[derive(IntoElement)]
pub struct ScrollArea {
    id: ElementId,
    scroll_x: bool,
    scroll_y: bool,
    children: Vec<AnyElement>,
    style: StyleRefinement,
    handle: Option<ScrollHandle>,
}

impl ScrollArea {
    /// A scroll area that scrolls down.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            scroll_x: false,
            scroll_y: true,
            children: Vec::new(),
            style: StyleRefinement::default(),
            handle: None,
        }
    }

    /// A scroll area that scrolls in both directions.
    pub fn both(id: impl Into<ElementId>) -> Self {
        Self::new(id).scroll_x(true).scroll_y(true)
    }

    /// Whether the area scrolls sideways.
    pub fn scroll_x(mut self, scroll_x: bool) -> Self {
        self.scroll_x = scroll_x;
        self
    }

    /// Whether the area scrolls down.
    pub fn scroll_y(mut self, scroll_y: bool) -> Self {
        self.scroll_y = scroll_y;
        self
    }

    /// Drive or observe the scroll position from outside, e.g. to jump to a
    /// section: each direct child is an item of the handle, so
    /// `handle.scroll_to_item(i)` brings child `i` into view.
    pub fn track_scroll(mut self, handle: &ScrollHandle) -> Self {
        self.handle = Some(handle.clone());
        self
    }

    /// Adds a child.
    pub fn child(mut self, child: impl gpui_kit::IntoElement) -> Self {
        self.children.push(child.into_any_element());
        self
    }

    /// Adds several children.
    pub fn children(
        mut self,
        children: impl IntoIterator<Item = impl gpui_kit::IntoElement>,
    ) -> Self {
        self.children
            .extend(children.into_iter().map(|child| child.into_any_element()));
        self
    }
}

impl gpui_kit::Styled for ScrollArea {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ScrollArea {
    fn render(self, _: &mut Window, _: &mut App) -> impl gpui_kit::IntoElement {
        let mut element = div().id(self.id).size_full().min_w_0().min_h_0();
        if self.scroll_x {
            element = element.overflow_x_scroll();
        }
        if self.scroll_y {
            element = element.overflow_y_scroll();
        }
        if let Some(handle) = &self.handle {
            element = element.track_scroll(handle);
        }
        element = gpui_kit::base::StyledExt::refine_style(element, &self.style);
        for child in self.children {
            element = element.child(child);
        }
        element
    }
}

#[cfg(test)]
mod tests {
    use super::ScrollArea;

    #[test]
    fn a_new_scroll_area_scrolls_down_only() {
        let area = ScrollArea::new("list");
        assert!(area.scroll_y);
        assert!(!area.scroll_x);
    }

    #[test]
    fn both_directions_can_be_asked_for() {
        let area = ScrollArea::both("list");
        assert!(area.scroll_y);
        assert!(area.scroll_x);
    }

    #[test]
    fn each_direction_can_be_turned_off() {
        let area = ScrollArea::new("list").scroll_y(false);
        assert!(!area.scroll_y);
        assert!(!area.scroll_x);
    }
}
