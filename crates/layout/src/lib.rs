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
//! Sizes come from [`Size`]. `guiraffe-core` owns the canonical token tables;
//! until it lands, this crate carries its own copy in `tokens`, so the widgets
//! have a scale of their own. Swapping to `guiraffe_core::Size` later touches
//! that module and the re-export below.
//!
//! # Responsiveness
//!
//! [`Breakpoint`] and [`Responsive`] decide which value a widget uses at a
//! given width. The rules are pure, so they can be tested without a window.
//! A widget reads the width of the window by default; a caller that knows the
//! width of its own container can say so with [`ContainerWidth`].

mod align;
mod app_shell;
mod aspect_ratio;
mod center;
mod container;
mod divider;
mod flex;
mod grid;
mod group;
mod macros;
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

pub use align::{Align, Justify};
pub use app_shell::AppShell;
pub use aspect_ratio::AspectRatio;
pub use center::Center;
pub use container::{Container, ContainerSize};
pub use divider::{Divider, DividerOrientation, LabelPosition};
pub use flex::{Flex, FlexDirection};
pub use grid::{Col, Grid, DEFAULT_COLUMNS};
pub use group::Group;
pub use responsive::{Breakpoint, Responsive};
pub use scroll_area::ScrollArea;
pub use simple_grid::{SimpleGrid, columns_for_min_width};
pub use space::Space;
pub use splitter::{Splitter, SplitterAxis, SplitterSizes};
pub use stack::Stack;
pub use tokens::Size;
pub use width::ContainerWidth;
