//! The frame an application is built in.

use gpui_kit::{
    AnyElement, App, ParentElement as _, Pixels, RenderOnce, StyleRefinement, Styled as _, Window,
    div, px,
};

use crate::macros::impl_into_element;
use crate::tokens::Size;

/// The parts an application shell is made of.
///
/// A shell has a header across the top, a navbar down the left, an aside down
/// the right, a footer across the bottom and the main content in the middle.
/// Every part is optional; the parts that are there keep their place even when
/// the others are missing.
///
/// ```
/// use guiraffe_layout::{AppShell, Size};
///
/// let shell = AppShell::new()
///     .header("header")
///     .navbar("navigation")
///     .main("content")
///     .padding(Size::Md);
/// ```
pub struct AppShell {
    header: Option<AnyElement>,
    navbar: Option<AnyElement>,
    aside: Option<AnyElement>,
    footer: Option<AnyElement>,
    main: Option<AnyElement>,
    navbar_width: Pixels,
    aside_width: Pixels,
    header_height: Pixels,
    footer_height: Pixels,
    padding: f32,
    navbar_collapsed: bool,
    with_border: bool,
    style: StyleRefinement,
}

impl Default for AppShell {
    fn default() -> Self {
        Self::new()
    }
}

impl AppShell {
    /// A shell with no parts in it.
    pub fn new() -> Self {
        Self {
            header: None,
            navbar: None,
            aside: None,
            footer: None,
            main: None,
            navbar_width: px(240.),
            aside_width: px(240.),
            header_height: px(60.),
            footer_height: px(40.),
            padding: 0.,
            navbar_collapsed: false,
            with_border: false,
            style: StyleRefinement::default(),
        }
    }

    /// The bar across the top.
    pub fn header(mut self, header: impl gpui_kit::IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    /// The bar down the left.
    pub fn navbar(mut self, navbar: impl gpui_kit::IntoElement) -> Self {
        self.navbar = Some(navbar.into_any_element());
        self
    }

    /// The bar down the right.
    pub fn aside(mut self, aside: impl gpui_kit::IntoElement) -> Self {
        self.aside = Some(aside.into_any_element());
        self
    }

    /// The bar across the bottom.
    pub fn footer(mut self, footer: impl gpui_kit::IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// The content in the middle.
    pub fn main(mut self, main: impl gpui_kit::IntoElement) -> Self {
        self.main = Some(main.into_any_element());
        self
    }

    /// How wide the navbar is.
    pub fn navbar_width(mut self, width: impl Into<Pixels>) -> Self {
        self.navbar_width = width.into();
        self
    }

    /// How wide the aside is.
    pub fn aside_width(mut self, width: impl Into<Pixels>) -> Self {
        self.aside_width = width.into();
        self
    }

    /// How tall the header is.
    pub fn header_height(mut self, height: impl Into<Pixels>) -> Self {
        self.header_height = height.into();
        self
    }

    /// How tall the footer is.
    pub fn footer_height(mut self, height: impl Into<Pixels>) -> Self {
        self.footer_height = height.into();
        self
    }

    /// The space around the main content.
    pub fn padding(mut self, padding: Size) -> Self {
        self.padding = padding.spacing();
        self
    }

    /// The space around the main content, in pixels.
    pub fn padding_px(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }

    /// Whether the navbar is hidden.
    pub fn navbar_collapsed(mut self, collapsed: bool) -> Self {
        self.navbar_collapsed = collapsed;
        self
    }

    /// Whether the shell is drawn with a hairline around it.
    pub fn with_border(mut self, with_border: bool) -> Self {
        self.with_border = with_border;
        self
    }
}

impl gpui_kit::Styled for AppShell {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for AppShell {
    fn render(self, _: &mut Window, cx: &mut App) -> impl gpui_kit::IntoElement {
        let border = crate::theme::hairline(cx);

        let mut shell = div().flex().flex_col().size_full();
        if self.with_border {
            shell = shell.border_1().border_color(border);
        }

        if let Some(header) = self.header {
            let mut bar = div()
                .flex_none()
                .w_full()
                .h(self.header_height)
                .flex()
                .items_center();
            if self.with_border {
                bar = bar.border_b_1().border_color(border);
            }
            shell = shell.child(bar.child(header));
        }

        let mut body = div().flex().flex_row().flex_1().min_h_0();
        if let Some(navbar) = self.navbar
            && !self.navbar_collapsed
        {
            let mut bar = div().flex_none().w(self.navbar_width).h_full();
            if self.with_border {
                bar = bar.border_r_1().border_color(border);
            }
            body = body.child(bar.child(navbar));
        }
        if let Some(main) = self.main {
            let mut content = div().flex_1().min_w_0().h_full();
            if self.padding > 0. {
                content = content.p(px(self.padding));
            }
            body = body.child(content.child(main));
        }
        if let Some(aside) = self.aside {
            let mut bar = div().flex_none().w(self.aside_width).h_full();
            if self.with_border {
                bar = bar.border_l_1().border_color(border);
            }
            body = body.child(bar.child(aside));
        }
        shell = shell.child(body);

        if let Some(footer) = self.footer {
            let mut bar = div()
                .flex_none()
                .w_full()
                .h(self.footer_height)
                .flex()
                .items_center();
            if self.with_border {
                bar = bar.border_t_1().border_color(border);
            }
            shell = shell.child(bar.child(footer));
        }

        gpui_kit::base::StyledExt::refine_style(shell, &self.style)
    }
}

impl_into_element!(AppShell);

#[cfg(test)]
mod tests {
    use super::AppShell;
    use gpui_kit::px;

    #[test]
    fn a_new_shell_has_no_parts() {
        let shell = AppShell::new();
        assert!(shell.header.is_none());
        assert!(shell.navbar.is_none());
        assert!(shell.aside.is_none());
        assert!(shell.footer.is_none());
        assert!(shell.main.is_none());
        assert!(!shell.navbar_collapsed);
        assert!(!shell.with_border);
        assert_eq!(shell.padding, 0.);
    }

    #[test]
    fn the_bars_have_default_sizes() {
        let shell = AppShell::new();
        assert_eq!(shell.navbar_width, px(240.));
        assert_eq!(shell.aside_width, px(240.));
        assert_eq!(shell.header_height, px(60.));
        assert_eq!(shell.footer_height, px(40.));
    }

    #[test]
    fn the_sizes_can_be_changed() {
        let shell = AppShell::new()
            .navbar_width(px(300.))
            .aside_width(px(200.))
            .header_height(px(48.))
            .footer_height(px(32.));
        assert_eq!(shell.navbar_width, px(300.));
        assert_eq!(shell.aside_width, px(200.));
        assert_eq!(shell.header_height, px(48.));
        assert_eq!(shell.footer_height, px(32.));
    }
}
