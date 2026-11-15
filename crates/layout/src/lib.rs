//! Layout primitives for GUIraffe.
//!
//! The widgets here answer "where does this go" and nothing else: they place
//! the children they are given, and they hand every styling decision the
//! caller makes straight to GPUI. The API follows Mantine, so a design that
//! reads `Stack`, `Group`, `Grid` and `Container` reads the same here.
//!
//! ```
//! use guiraffe_layout::{Align, Col, Grid, Group, Justify, Size, Stack};
//!
//! let page = Stack::new()
//!     .gap(Size::Md)
//!     .align(Align::Stretch)
//!     .child(Group::new().justify(Justify::Between).child("title"))
//!     .child(
//!         Grid::new()
//!             .child(Col::span(6).child("left"))
//!             .child(Col::span(6).child("right")),
//!     );
//! ```
//!
//! # Tokens
//!
//! Sizes come from [`Size`], the scale `guiraffe-core` owns: the same tokens
//! the theme and the palette use, so a gap here means what a gap means
//! everywhere else in GUIraffe.
//!
//! # Shapes
//!
//! The widgets come in the two shapes the house style asks for. A component is
//! stateless and cheap: [`Stack`], [`Group`], [`Flex`], [`Grid`], [`Container`],
//! [`Center`], [`AspectRatio`], [`Space`], [`AppShell`] and the rest. A control
//! keeps state that outlives a frame: [`Splitter`] takes a
//! [`SplitterState`] the application owns, so the pane sizes can be stored
//! and restored.
//!
//! # Responsiveness
//!
//! [`Breakpoint`] and [`Responsive`] decide which value a widget uses at a
//! given width. The rules are pure, so they can be tested without a window.
//! A widget reads the width of the window by default; a caller that knows the
//! width of its own container can say so with [`ContainerWidth`].

/// GPUI under the name its derive macros expect.
///
/// The `IntoElement` derive that ships with GPUI expands to paths that start
/// at `gpui::`, so the crate has to answer to that name. `gpui-kit` re-exports
/// GPUI in full, which is what this alias points at.
pub(crate) use gpui_kit as gpui;

mod align;
mod app_shell;
mod aspect_ratio;
mod center;
mod container;
mod divider;
mod flex;
mod grid;
mod group;
mod responsive;
mod scroll_area;
mod simple_grid;
mod space;
mod splitter;
mod stack;
mod style;
mod theme;
mod tokens;
mod width;

/// Install what the layout widgets need before they are shown.
///
/// The widgets draw their rules and their dimmed text from the core theme, so
/// this installs that theme. It is idempotent: calling it twice changes
/// nothing.
pub fn init(cx: &mut gpui_kit::App) {
    guiraffe_core::init(cx);
}

pub use align::{Align, Justify};
pub use app_shell::{AppShell, AppShellState};
pub use aspect_ratio::AspectRatio;
pub use center::Center;
pub use container::Container;
pub use divider::{Divider, DividerOrientation, LabelPosition};
pub use flex::{Flex, FlexDirection};
pub use grid::{Col, DEFAULT_COLUMNS, Grid, GridPlacement, Span};
pub use group::Group;
pub use responsive::{Breakpoint, Responsive};
pub use scroll_area::ScrollArea;
pub use simple_grid::{SimpleGrid, columns_for_min_width};
pub use space::Space;
pub use splitter::{
    Splitter, SplitterAxis, SplitterEvent, SplitterSizes, SplitterState, SplitterStore,
    splitter_store,
};
pub use stack::Stack;
pub use tokens::Size;
pub use width::{ContainerWidth, ContainerWidthState};
