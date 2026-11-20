//! Compile declarative GPUI trees into ordinary Rust builder calls.
//!
//! See `guiraffe_ui::ui` for usage examples.

use proc_macro::TokenStream;
use syn::parse_macro_input;

mod component;
mod tree;

/// Build an element tree with Rust expressions and builder methods.
#[proc_macro]
pub fn ui(input: TokenStream) -> TokenStream {
    parse_macro_input!(input as tree::Root).expand().into()
}

/// Generate a stateless GPUI component from a function.
#[proc_macro_attribute]
pub fn component(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as syn::parse::Nothing);
    let _ = args;
    let function = parse_macro_input!(input as syn::ItemFn);
    component::expand(function)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
