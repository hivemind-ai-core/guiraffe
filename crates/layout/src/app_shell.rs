//! The frame an application is built in.

use crate::gpui::IntoElement;
use crate::responsive::Breakpoint;
use crate::tokens::Size;

use gpui_kit::base::ElementExt as _;
use gpui_kit::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, EventEmitter,
    InteractiveElement as _, ParentElement as _, Pixels, RenderOnce,
    StatefulInteractiveElement as _, StyleRefinement, Styled as _, Window, div, px,
};

/// The event an [`AppShell`] emits.
///
/// The shell tells what it did, so an application can react to it without
/// watching the state.
///
/// ```
/// use guiraffe_layout::AppShellEvent;
///
/// # fn read(event: AppShellEvent) {
/// match event {
///     AppShellEvent::NavbarCollapsed { collapsed } => {
///         assert!(!collapsed);
///     }
///     _ => {}
/// }
/// # }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub enum AppShellEvent {
    /// The navbar was collapsed or opened.
    NavbarCollapsed {
        /// Whether the navbar is collapsed now.
        collapsed: bool,
    },
    /// The aside was collapsed or opened.
    AsideCollapsed {
        /// Whether the aside is collapsed now.
        collapsed: bool,
    },
    /// The navbar was given another width.
    NavbarResized {
        /// The width of the navbar, in pixels.
        width: f32,
    },
    /// The aside was given another width.
    AsideResized {
        /// The width of the aside, in pixels.
        width: f32,
    },
}

/// The state an [`AppShell`] keeps its layout in.
///
/// The state is an entity, so it outlives a frame: the application owns it,
/// collapses the navbar through it, and the shell reads the layout from it.
///
/// The state emits [`AppShellEvent`] when the shell collapses a bar or gives
/// it another width.
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
    /// The band the navbar collapses below.
    collapse_below: Option<Breakpoint>,
    /// The band the aside collapses below.
    collapse_aside_below: Option<Breakpoint>,
    /// Whether the shell collapsed the navbar itself.
    ///
    /// The shell only collapses a bar when the width crosses the band it was
    /// given, so a toggle the user made is not undone by every resize.
    navbar_auto_collapsed: bool,
    /// Whether the shell collapsed the aside itself.
    aside_auto_collapsed: bool,
    /// The drag of the navbar that is being resized.
    navbar_drag: Option<BarDrag>,
    /// The drag of the aside that is being resized.
    aside_drag: Option<BarDrag>,
    /// The width the shell was last laid out in.
    width: f32,
}

/// A bar that is being dragged to another width.
#[derive(Clone, Copy, Debug)]
struct BarDrag {
    /// Where the pointer was when the drag began.
    start: f32,
    /// How wide the bar was when the drag began.
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
            collapse_aside_below: None,
            navbar_auto_collapsed: false,
            aside_auto_collapsed: false,
            navbar_drag: None,
            aside_drag: None,
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
        self.set_navbar_collapsed(!self.navbar_collapsed, cx);
    }

    /// Collapses or opens the navbar.
    pub fn set_navbar_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        if self.navbar_collapsed != collapsed {
            self.navbar_collapsed = collapsed;
            cx.notify();
            cx.emit(AppShellEvent::NavbarCollapsed { collapsed });
        }
    }

    /// Collapses the navbar.
    pub fn collapse_navbar(&mut self, cx: &mut Context<Self>) {
        self.set_navbar_collapsed(true, cx);
    }

    /// Opens the navbar.
    pub fn expand_navbar(&mut self, cx: &mut Context<Self>) {
        self.set_navbar_collapsed(false, cx);
    }

    /// Collapses or opens the aside.
    pub fn toggle_aside(&mut self, cx: &mut Context<Self>) {
        self.set_aside_collapsed(!self.aside_collapsed, cx);
    }

    /// Collapses or opens the aside.
    pub fn set_aside_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        if self.aside_collapsed != collapsed {
            self.aside_collapsed = collapsed;
            cx.notify();
            cx.emit(AppShellEvent::AsideCollapsed { collapsed });
        }
    }

    /// Sets how wide the navbar is.
    pub fn set_navbar_width(&mut self, width: impl Into<Pixels>, cx: &mut Context<Self>) {
        let width = width.into();
        if self.navbar_width != width {
            self.navbar_width = width;
            cx.notify();
            cx.emit(AppShellEvent::NavbarResized {
                width: width.as_f32(),
            });
        }
    }

    /// Sets how wide the aside is.
    pub fn set_aside_width(&mut self, width: impl Into<Pixels>, cx: &mut Context<Self>) {
        let width = width.into();
        if self.aside_width != width {
            self.aside_width = width;
            cx.notify();
            cx.emit(AppShellEvent::AsideResized {
                width: width.as_f32(),
            });
        }
    }

    /// The band the navbar collapses below.
    pub fn collapse_below(&self) -> Option<Breakpoint> {
        self.collapse_below
    }

    /// The band the aside collapses below.
    pub fn collapse_aside_below(&self) -> Option<Breakpoint> {
        self.collapse_aside_below
    }

    /// Remembers the width the shell is laid out in.
    ///
    /// A bar collapses when the width crosses into a band below the one the
    /// shell was given: a width that stays in the same band leaves the bars
    /// alone, so a toggle the user made is not undone by every resize.
    fn measure(&mut self, width: f32, cx: &mut Context<Self>) {
        if (width - self.width).abs() < f32::EPSILON {
            return;
        }
        self.width = width;

        let collapse = self
            .collapse_below
            .is_some_and(|band| width < band.min_width());
        if collapse != self.navbar_auto_collapsed {
            self.navbar_auto_collapsed = collapse;
            self.set_navbar_collapsed(collapse, cx);
        }

        let collapse = self
            .collapse_aside_below
            .is_some_and(|band| width < band.min_width());
        if collapse != self.aside_auto_collapsed {
            self.aside_auto_collapsed = collapse;
            self.set_aside_collapsed(collapse, cx);
        }
    }

    /// Remembers that the navbar is being dragged to another width.
    fn begin_navbar_drag(&mut self, start: f32) {
        self.navbar_drag = Some(BarDrag {
            start,
            width: self.navbar_width.as_f32(),
        });
    }

    /// Remembers that the aside is being dragged to another width.
    fn begin_aside_drag(&mut self, start: f32) {
        self.aside_drag = Some(BarDrag {
            start,
            width: self.aside_width.as_f32(),
        });
    }

    /// The width the navbar has while it is dragged.
    fn dragged_navbar_width(&self, position: f32) -> Option<f32> {
        let drag = self.navbar_drag?;
        Some((drag.width + position - drag.start).max(min_bar_width()))
    }

    /// The width the aside has while it is dragged.
    fn dragged_aside_width(&self, position: f32) -> Option<f32> {
        let drag = self.aside_drag?;
        Some((drag.width - position + drag.start).max(min_bar_width()))
    }
}

impl EventEmitter<AppShellEvent> for AppShellState {}

/// The width the side bars take until the application says otherwise: a
/// quarter of the medium container.
fn default_bar_width() -> f32 {
    Size::Md.container_width() / 4.
}

/// The narrowest a bar may become.
fn min_bar_width() -> f32 {
    crate::theme::panel_min_size()
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
/// The main content is a view, and the shell keeps it: the content is not
/// rendered again when the shell collapses a bar.
///
/// ```
/// use guiraffe_layout::{AppShell, AppShellState, Size};
///
/// # fn build(state: &gpui_kit::Entity<AppShellState>) {
/// let shell = AppShell::new(state)
///     .header("header")
///     .navbar("navigation")
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
    main: Option<AnyView>,
    header_height: Pixels,
    footer_height: Pixels,
    padding: f32,
    with_border: bool,
    collapse_below: Option<Breakpoint>,
    collapse_aside_below: Option<Breakpoint>,
    resizable_navbar: bool,
    resizable_aside: bool,
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
            collapse_below: None,
            collapse_aside_below: None,
            resizable_navbar: false,
            resizable_aside: false,
            style: StyleRefinement::default(),
        }
    }

    /// The header of the shell.
    pub fn header(mut self, header: impl gpui_kit::IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    /// The navbar of the shell.
    pub fn navbar(mut self, navbar: impl gpui_kit::IntoElement) -> Self {
        self.navbar = Some(navbar.into_any_element());
        self
    }

    /// The main content of the shell.
    ///
    /// The content is a view, and the shell keeps it: the content is not
    /// rendered again when the shell collapses a bar or gives it another
    /// width.
    pub fn main(mut self, main: impl Into<AnyView>) -> Self {
        self.main = Some(main.into());
        self
    }

    /// The aside of the shell.
    pub fn aside(mut self, aside: impl gpui_kit::IntoElement) -> Self {
        self.aside = Some(aside.into_any_element());
        self
    }

    /// The footer of the shell.
    pub fn footer(mut self, footer: impl gpui_kit::IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
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

    /// The room around the parts of the shell.
    pub fn padding(mut self, padding: impl Into<Size>) -> Self {
        self.padding = padding.into().spacing();
        self
    }

    /// A line around the shell.
    pub fn border(mut self) -> Self {
        self.with_border = true;
        self
    }

    /// The band the navbar collapses below.
    ///
    /// The navbar collapses when the shell is narrower than the band, and it
    /// opens again when the shell is wider. A width that stays in the same
    /// band leaves the navbar alone.
    pub fn collapse_below(mut self, band: Breakpoint) -> Self {
        self.collapse_below = Some(band);
        self
    }

    /// The band the aside collapses below.
    ///
    /// The aside collapses when the shell is narrower than the band, and it
    /// opens again when the shell is wider. A width that stays in the same
    /// band leaves the aside alone.
    pub fn collapse_aside_below(mut self, band: Breakpoint) -> Self {
        self.collapse_aside_below = Some(band);
        self
    }

    /// A handle the user can drag to give the navbar another width.
    pub fn resizable_navbar(mut self) -> Self {
        self.resizable_navbar = true;
        self
    }

    /// A handle the user can drag to give the aside another width.
    pub fn resizable_aside(mut self) -> Self {
        self.resizable_aside = true;
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
        let state = self.state.clone();

        // The bands belong to the state, so the state can tell whether the
        // width crossed them.
        let collapse_below = self.collapse_below;
        let collapse_aside_below = self.collapse_aside_below;
        state.update(cx, |state, _| {
            state.collapse_below = collapse_below;
            state.collapse_aside_below = collapse_aside_below;
        });

        let AppShell {
            header,
            navbar,
            main,
            aside,
            footer,
            header_height,
            footer_height,
            padding,
            with_border,
            resizable_navbar,
            resizable_aside,
            style,
            ..
        } = self;

        let mut shell = div().id("app-shell").flex().flex_col().size_full();
        if with_border {
            shell = shell.border_1().border_color(border);
        }

        if let Some(header) = header {
            let mut bar = div()
                .flex_none()
                .w_full()
                .h(header_height)
                .flex()
                .items_center();
            if with_border {
                bar = bar.border_b_1().border_color(border);
            }
            shell = shell.child(bar.child(header));
        }

        let mut body = div().flex().flex_row().flex_1().min_h_0();
        if let Some(navbar) = navbar {
            body = body.child(bar(
                Bar::Navbar,
                navbar,
                resizable_navbar,
                with_border,
                &state,
                cx,
            ));
        }
        if let Some(main) = main {
            // The main content is kept: the shell collapsing a bar does not
            // render the content again.
            let mut main_style = StyleRefinement::default();
            main_style = main_style.flex_1().min_w_0().h_full();
            if padding > 0. {
                main_style = main_style.p(px(padding));
            }
            body = body.child(main.cached(main_style));
        }
        if let Some(aside) = aside {
            body = body.child(bar(
                Bar::Aside,
                aside,
                resizable_aside,
                with_border,
                &state,
                cx,
            ));
        }
        shell = shell.child(body);

        if let Some(footer) = footer {
            let mut bar = div()
                .flex_none()
                .w_full()
                .h(footer_height)
                .flex()
                .items_center();
            if with_border {
                bar = bar.border_t_1().border_color(border);
            }
            shell = shell.child(bar.child(footer));
        }

        // The shell measures itself, so it knows how wide it is.
        let measured = state.clone();
        let shell = shell.on_prepaint(move |bounds, _, cx| {
            measured.update(cx, |state, cx| {
                state.measure(bounds.size.width.as_f32(), cx)
            });
        });

        gpui_kit::base::StyledExt::refine_style(shell, &style)
    }
}

/// Which bar of the shell a piece is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bar {
    Navbar,
    Aside,
}

/// A bar of the shell: the navbar or the aside.
fn bar(
    which: Bar,
    content: AnyElement,
    resizable: bool,
    with_border: bool,
    state: &Entity<AppShellState>,
    cx: &App,
) -> impl gpui_kit::IntoElement {
    let (width, collapsed) = {
        let state = state.read(cx);
        match which {
            Bar::Navbar => (state.navbar_width, state.navbar_collapsed),
            Bar::Aside => (state.aside_width, state.aside_collapsed),
        }
    };

    // A collapsed bar keeps its place: it stays in the tree, so what is
    // inside it keeps its state.
    let mut element = div()
        .flex()
        .flex_row()
        .flex_none()
        .w(if collapsed { px(0.) } else { width })
        .h_full()
        .overflow_hidden();
    if with_border {
        let border = crate::theme::hairline(cx);
        element = match which {
            Bar::Navbar => element.border_r_1().border_color(border),
            Bar::Aside => element.border_l_1().border_color(border),
        };
    }
    element = element.child(div().flex_1().min_w_0().child(content));

    if resizable {
        element = element.child(resize_handle(which, state.clone()));
    }
    element
}

/// The drag that resizes a bar.
///
/// The value of the drag is a marker: the bar follows the pointer, and the
/// drag itself shows nothing.
struct ResizeDrag;

impl gpui_kit::Render for ResizeDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui_kit::IntoElement {
        div()
    }
}

/// The handle the user drags to give a bar another width.
fn resize_handle(bar: Bar, state: Entity<AppShellState>) -> impl gpui_kit::IntoElement {
    let pressed = state.clone();
    let moved = state.clone();
    let released = state.clone();
    div()
        .id(match bar {
            Bar::Navbar => "app-shell-navbar-handle",
            Bar::Aside => "app-shell-aside-handle",
        })
        .w(px(4.))
        .h_full()
        .flex_shrink_0()
        .cursor_col_resize()
        .on_mouse_down(gpui_kit::MouseButton::Left, move |event, _, cx| {
            pressed.update(cx, |state, _| match bar {
                Bar::Navbar => state.begin_navbar_drag(event.position.x.as_f32()),
                Bar::Aside => state.begin_aside_drag(event.position.x.as_f32()),
            });
        })
        // The drag follows the pointer, even when the pointer is not over
        // the handle any more.
        .on_drag(ResizeDrag, |_, _, _, cx| cx.new(|_| ResizeDrag))
        .on_drag_move(move |event: &gpui_kit::DragMoveEvent<ResizeDrag>, _, cx| {
            moved.update(cx, |state, cx| {
                let position = event.event.position.x.as_f32();
                let width = match bar {
                    Bar::Navbar => state.dragged_navbar_width(position),
                    Bar::Aside => state.dragged_aside_width(position),
                };
                if let Some(width) = width {
                    match bar {
                        Bar::Navbar => state.set_navbar_width(px(width), cx),
                        Bar::Aside => state.set_aside_width(px(width), cx),
                    }
                }
            });
        })
        .on_mouse_up(gpui_kit::MouseButton::Left, move |_, _, cx| {
            released.update(cx, |state, _| match bar {
                Bar::Navbar => state.navbar_drag = None,
                Bar::Aside => state.aside_drag = None,
            });
        })
}

#[cfg(test)]
mod tests {
    use super::{AppShellState, default_bar_width};
    use crate::responsive::Breakpoint;
    use gpui_kit::px;

    #[test]
    fn the_bars_start_at_their_default_width() {
        let cx = gpui_kit::TestAppContext::single();
        let state = cx.update(AppShellState::new);
        state.read_with(&cx, |state, _| {
            assert_eq!(state.navbar_width(), px(default_bar_width()));
            assert_eq!(state.aside_width(), px(default_bar_width()));
            assert!(!state.navbar_collapsed());
            assert!(!state.aside_collapsed());
        });
    }

    #[test]
    fn a_shell_has_no_band_to_collapse_below_until_it_is_given_one() {
        let cx = gpui_kit::TestAppContext::single();
        let state = cx.update(AppShellState::new);
        state.read_with(&cx, |state, _| {
            assert_eq!(state.collapse_below(), None);
            assert_eq!(state.collapse_aside_below(), None);
        });
    }

    #[test]
    fn a_band_names_the_width_the_bars_collapse_below() {
        assert_eq!(Breakpoint::Xs.min_width(), 576.);
        assert_eq!(Breakpoint::Sm.min_width(), 768.);
    }
}
