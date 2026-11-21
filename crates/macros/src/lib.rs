//! Compile declarative GPUI trees into ordinary Rust builder calls.
//!
//! See `guiraffe_ui::ui` for usage examples.

use proc_macro::TokenStream;
use syn::parse_macro_input;

mod component;
mod tree;

/// Build an element tree with Rust expressions and builder methods.
///
/// The expansion uses ordinary Rust calls. Any builder visible to the calling
/// module works without adding a catalogue entry for the Rust macro.
/// Use `name: value` for a single argument, a bare method name for zero
/// arguments, or `.name(arg1, arg2)` for explicit calls. Parenthesize an
/// inline struct literal in `name: value` or pass it in an explicit call.
///
/// ```
/// use guiraffe_macros::ui;
///
/// struct Label(&'static str);
///
/// impl Label {
///     fn new(text: &'static str) -> Self {
///         Self(text)
///     }
///
///     fn text(self, text: &'static str) -> Self {
///         Self(text)
///     }
/// }
///
/// let label = ui! { Label("Initial") { text: "Updated" } };
/// assert_eq!(label.0, "Updated");
/// ```
///
/// ```
/// use guiraffe_macros::ui;
///
/// struct Bounds { min: u8, max: u8 }
/// struct Meter(u8, u8);
/// impl Meter {
///     fn new() -> Self { Self(0, 0) }
///     fn range(self, bounds: Bounds, adjustment: u8) -> Self {
///         Self(bounds.min, bounds.max + adjustment)
///     }
/// }
/// let meter = ui! { Meter { .range(Bounds { min: 1, max: 5 }, 2) } };
/// assert_eq!((meter.0, meter.1), (1, 7));
/// ```
#[proc_macro]
pub fn ui(input: TokenStream) -> TokenStream {
    parse_macro_input!(input as tree::Root).expand().into()
}

/// Generate a stateless GPUI component from a function.
#[proc_macro_attribute]
pub fn component(args: TokenStream, input: TokenStream) -> TokenStream {
    parse_macro_input!(args as syn::parse::Nothing);
    let function = parse_macro_input!(input as syn::ItemFn);
    component::expand(function)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
