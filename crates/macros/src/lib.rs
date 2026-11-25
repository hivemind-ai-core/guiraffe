//! Compile declarative GPUI trees into ordinary Rust builder calls.
//!
//! See `guiraffe_ui::ui` for usage examples.

use proc_macro::TokenStream;
use syn::parse_macro_input;

mod component;
mod store;
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

/// Generate a GPUI component from a function.
///
/// `#[component]` makes a stateless component. `#[component(stateful)]` makes a control:
/// `#[state]` parameters live in an entity keyed by the component's id (its first
/// constructor argument), registered on the action bus at that id with `Observe`/`Drive`
/// (`set_value`), and each state field can be controlled by the caller (`.count(v)` with
/// `.on_count_change(..)`, which `ui!`'s `count <=> lens` generates). Provisional (0.3).
#[proc_macro_attribute]
pub fn component(args: TokenStream, input: TokenStream) -> TokenStream {
    let stateful = if args.is_empty() {
        false
    } else {
        let flag = parse_macro_input!(args as syn::Ident);
        if flag != "stateful" {
            return syn::Error::new_spanned(flag, "expected `stateful`")
                .into_compile_error()
                .into();
        }
        true
    };
    let function = parse_macro_input!(input as syn::ItemFn);
    component::expand(function, stateful)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Declare an application state struct with typed field lenses, per-field `Set` events with
/// validation, and listed events (provisional, 0.3). See `guiraffe_state` and
/// docs/guides/stores.md.
#[proc_macro]
pub fn store(input: TokenStream) -> TokenStream {
    parse_macro_input!(input as store::StoreDef).expand().into()
}

/// Field lenses for a nested state struct, so `ui!` paths and `Lens::then` reach its fields.
#[proc_macro_derive(Lens)]
pub fn derive_lens(input: TokenStream) -> TokenStream {
    let item = parse_macro_input!(input as syn::ItemStruct);
    store::derive_lens(item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
