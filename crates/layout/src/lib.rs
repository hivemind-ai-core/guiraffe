//! Layout primitives for GUIraffe.
//!
//! The widgets here answer "where does this go" and nothing else: they place
//! the children they are given, and they hand every styling decision the
//! caller makes straight to GPUI. The API follows Mantine, so a design that
//! reads `Stack`, `Group`, `Grid` and `Container` reads the same here.
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
mod flex;
mod group;
mod macros;
mod responsive;
mod stack;
mod style;
mod tokens;
mod width;

pub use align::{Align, Justify};
pub use flex::{Flex, FlexDirection};
pub use group::Group;
pub use responsive::{Breakpoint, Responsive};
pub use stack::Stack;
pub use tokens::Size;
pub use width::ContainerWidth;
