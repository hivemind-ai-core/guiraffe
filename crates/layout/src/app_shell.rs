//! The frame an application is built in.

use crate::gpui::IntoElement;
use crate::tokens::Size;

use gpui_kit::base::ElementExt as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, ParentElement as _,
    Pixels, RenderOnce, StyleRefinement, Styled as _, Window, div, px,
};

/// The state an [`AppShell`] keeps its layout in.
///
/// The state is an entity, so it outlives a frame: the application owns it,
/// collapses the navbar through it, and the shell reads the layout from it.
///
/// ```
/// use guiraffe_layout::AppShellState;
///
/// # fn build(state: &gpui_kit::Entity<AppShellState>) {
/// let shell = guiraffe_layout::AppShell::new(state);
/// # }
/// ```
#[derive(Clone, Default)]
pub struct AppShellState {
    navbar_collapsed: bool,
    aside_collapsed: bool,
    navbar_width: Pixels,
    aside_width: Pixels,
    /// The width below which the navbar collapses.
    collapse_below: Option<f32>,
    /// The width the shell was last laid out in.
    width: f32,
}

impl AppShellState {
    /// A state with the bars at their default widths.
    pub fn new(cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self {
            navbar_collapsed: false,
            aside_collapsed: false,
            navbar_width: px(default_bar_width()),
            aside_width: px(default_bar_width()),
            collapse_below: None,
            width: 0.,
        })
    }

    /// Whether the navbar is collapsed.
    pub fn navbar_collapsed(&self) -> bool {
        self.navbar_collapsed
    }

    /// Whether the aside is collapsed.
    pub fn aside_collapsed(&self) -> bool {
        self.aside_collapsed
    }

    /// How wide the navbar is.
    pub fn navbar_width(&self) -> Pixels {
        self.navbar_width
    }

    /// How wide the aside is.
    pub fn aside_width(&self) -> Pixels {
        self.aside_width
    }

    /// Collapses or opens the navbar.
    pub fn toggle_navbar(&mut self, cx: &mut Context<Self>) {
        self.navbar_collapsed = !self.navbar_collapsed;
        cx.notify();
    }

    /// Collapses the navbar.
    pub fn collapse_navbar(&mut self, cx: &mut Context<Self>) {
        if !self.navbar_collapsed {
            self.navbar_collapsed = true;
            cx.notify();
        }
    }

    /// Opens the navbar.
    pub fn expand_navbar(&mut self, cx: &mut Context<Self>) {
        if self.navbar_collapsed {
            self.navbar_collapsed = false;
            cx.notify();
        }
    }

    /// Collapses or opens the aside.
    pub fn toggle_aside(&mut self, cx: &mut Context<Self>) {
        self.aside_collapsed = !self.aside_collapsed;
        cx.notify();
    }

    /// Sets how wide the navbar is.
    pub fn set_navbar_width(&mut self, width: impl Into<Pixels>, cx: &mut Context<Self>) {
        let width = width.into();
        if self.navbar_width != width {
            self.navbar_width = width;
            cx.notify();
        }
    }

    /// Sets how wide the aside is.
    pub fn set_aside_width(&mut self, width: impl Into<Pixels>, cx: &mut Context<Self>) {
        let width = width.into();
        if self.aside_width != width {
            self.aside_width = width;
            cx.notify();
        }
    }

    /// The width below which the navbar collapses.
    pub fn collapse_below(&self) -> Option<f32> {
        self.collapse_below
    }

    /// Collapses the navbar on a window that is narrower than `width`.
    pub fn collapse_below_width(&mut self, width: f32, cx: &mut Context<Self>) {
        if self.collapse_below != Some(width) {
            self.collapse_below = Some(width);
            cx.notify();
        }
    }

    /// Remembers the width the shell is laid out in.
    ///
    /// The navbar collapses when the width crosses the band the state was
    /// given.
    fn measure(&mut self, width: f32, cx: &mut Context<Self>) {
        if (width - self.width).abs() < f32::EPSILON {
            return;
        }
        self.width = width;
        let Some(below) = self.collapse_below else {
            return;
        };
        let collapsed = width < below;
        if collapsed != self.navbar_collapsed {
            self.navbar_collapsed = collapsed;
            cx.notify();
        }
    }
}

/// The width the side bars take until the application says otherwise: a
/// quarter of the medium container.
fn default_bar_width() -> f32 {
    Size::Md.container_width() / 4.
}

/// The parts an application shell is made of.
///
/// A shell has a header across the top, a navbar down the left, an aside down
/// the right, a footer across the bottom and the main content in the middle.
/// Every part is optional; the parts that are there keep their place even when
/// the others are missing.
///
/// The shell is a control: the layout lives in an [`AppShellState`] the
/// application owns, so the navbar can be collapsed and opened again without
/// the shell losing its place.
///
/// ```
/// use guiraffe_layout::{AppShell, AppShellState, Size};
///
/// # fn build(state: &gpui_kit::Entity<AppShellState>) {
/// let shell = AppShell::new(state)
///     .header("header")
///     .navbar("navigation")
///     .main("content")
///     .padding(Size::Md);
/// # }
/// ```
#[derive(IntoElement)]
pub struct AppShell {
    state: Entity<AppShellState>,
    header: Option<AnyElement>,
    navbar: Option<AnyElement>,
    aside: Option<AnyElement>,
    footer: Option<AnyElement>,
    main: Option<AnyElement>,
    header_height: Pixels,
    footer_height: Pixels,
    padding: f32,
    with_border: bool,
    style: StyleRefinement,
}

impl AppShell {
    /// A shell whose layout lives in `state`.
    pub fn new(state: &Entity<AppShellState>) -> Self {
        Self {
            state: state.clone(),
            header: None,
            navbar: None,
            aside: None,
            footer: None,
            main: None,
            header_height: px(Size::Lg.control_height()),
            footer_height: px(Size::Sm.control_height()),
            padding: 0.,
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
    fn render(self, _window: &mut Window, cx: &mut App) -> impl gpui_kit::IntoElement {
        let border = crate::theme::hairline(cx);
        let layout = self.state.read(cx).clone();
        let navbar_collapsed = layout.navbar_collapsed;
        let aside_collapsed = layout.aside_collapsed;
        let navbar_width = layout.navbar_width;
        let aside_width = layout.aside_width;

        let mut shell = div().id("app-shell").flex().flex_col().size_full();
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
        if let Some(navbar) = self.navbar {
            // A collapsed navbar keeps its place: it stays in the tree, so
            // what is inside it keeps its state.
            let mut bar = div()
                .flex_none()
                .w(if navbar_collapsed {
                    px(0.)
                } else {
                    navbar_width
                })
                .h_full()
                .overflow_hidden();
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
            let mut bar = div()
                .flex_none()
                .w(if aside_collapsed { px(0.) } else { aside_width })
                .h_full()
                .overflow_hidden();
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

        // The shell measures itself, so the navbar can collapse when the
        // window gets too narrow for it.
        let state = self.state.clone();
        let shell = shell.on_prepaint(move |bounds, _, cx| {
            state.update(cx, |state, cx| {
                state.measure(bounds.size.width.as_f32(), cx)
            });
        });

        gpui_kit::base::StyledExt::refine_style(shell, &self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::{AppShell, AppShellState, default_bar_width};
    use crate::tokens::Size;
    use gpui_kit::px;

    #[test]
    fn a_new_shell_has_no_parts() {
        let cx = gpui_kit::TestAppContext::single();
        let state = cx.update(AppShellState::new);
        let shell = AppShell::new(&state);
        assert!(shell.header.is_none());
        assert!(shell.navbar.is_none());
        assert!(shell.aside.is_none());
        assert!(shell.footer.is_none());
        assert!(shell.main.is_none());
        assert!(!shell.with_border);
        assert_eq!(shell.padding, 0.);
    }

    #[test]
    fn the_bars_have_default_sizes() {
        let cx = gpui_kit::TestAppContext::single();
        let state = cx.update(AppShellState::new);
        let shell = AppShell::new(&state);
        assert_eq!(shell.header_height, px(Size::Lg.control_height()));
        assert_eq!(shell.footer_height, px(Size::Sm.control_height()));
        assert_eq!(
            default_bar_width(),
            Size::Md.container_width() / 4.,
            "the bars take their width from the scale"
        );
    }

    #[test]
    fn the_sizes_can_be_changed() {
        let cx = gpui_kit::TestAppContext::single();
        let state = cx.update(AppShellState::new);
        let shell = AppShell::new(&state)
            .header_height(px(48.))
            .footer_height(px(32.))
            .padding(Size::Md);
        assert_eq!(shell.header_height, px(48.));
        assert_eq!(shell.footer_height, px(32.));
        assert_eq!(shell.padding, Size::Md.spacing());
    }
}
