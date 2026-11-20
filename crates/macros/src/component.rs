use proc_macro2::TokenStream;
use quote::quote;
use syn::{Error, FnArg, Ident, ItemFn, Pat, Result, ReturnType, Token, Type, Visibility};

pub(crate) fn expand(function: ItemFn) -> Result<TokenStream> {
    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = function;
    if sig.constness.is_some()
        || sig.asyncness.is_some()
        || sig.unsafety.is_some()
        || sig.abi.is_some()
        || sig.variadic.is_some()
    {
        return Err(Error::new_spanned(
            sig,
            "expected an ordinary Rust function",
        ));
    }
    let ReturnType::Type(_, output) = &sig.output else {
        return Err(Error::new_spanned(
            sig.ident,
            "a component must return an element",
        ));
    };
    let name = &sig.ident;
    let mut fields = Vec::new();
    let mut types = Vec::new();
    let mut required = Vec::new();
    let mut initializers = Vec::new();
    let mut builders = Vec::new();
    let mut generics = sig.generics.clone();
    for arg in &sig.inputs {
        let FnArg::Typed(arg) = arg else {
            return Err(Error::new_spanned(
                arg,
                "component props must be named parameters",
            ));
        };
        let Pat::Ident(pat) = arg.pat.as_ref() else {
            return Err(Error::new_spanned(
                &arg.pat,
                "component props must be named parameters",
            ));
        };
        if pat.by_ref.is_some() || pat.subpat.is_some() {
            return Err(Error::new_spanned(
                pat,
                "component props must bind by value",
            ));
        }
        let field = &pat.ident;
        let ty = &arg.ty;
        fields.push(field);
        types.push(ty);
        let mut default = false;
        for attr in &arg.attrs {
            if !attr.path().is_ident("prop") || default {
                return Err(Error::new_spanned(
                    attr,
                    "expected one #[prop(default)] attribute",
                ));
            }
            let option = attr.parse_args::<Ident>()?;
            if option != "default" {
                return Err(Error::new_spanned(option, "expected `default`"));
            }
            default = true;
        }
        if default {
            generics
                .make_where_clause()
                .predicates
                .push(syn::parse_quote!(#ty: ::core::default::Default));
            initializers.push(quote!(#field: ::core::default::Default::default()));
            builders.push(quote! {
                #vis fn #field(mut self, #field: #ty) -> Self {
                    self.#field = #field;
                    self
                }
            });
        } else {
            required.push(quote!(#field: #ty));
            initializers.push(quote!(#field));
        }
    }
    for ty in &types {
        if matches!(ty.as_ref(), Type::ImplTrait(_)) {
            return Err(Error::new_spanned(
                ty,
                "use a named generic parameter for a component prop",
            ));
        }
    }
    let render_params = &sig
        .inputs
        .iter()
        .map(|arg| {
            let mut arg = arg.clone();
            if let FnArg::Typed(arg) = &mut arg {
                arg.attrs.clear();
            }
            arg
        })
        .collect::<syn::punctuated::Punctuated<_, Token![,]>>();
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let mut render_generics = generics.clone();
    render_generics
        .make_where_clause()
        .predicates
        .push(syn::parse_quote!(Self: 'static));
    let (_, _, render_where) = render_generics.split_for_impl();
    let private = Visibility::Inherited;
    Ok(quote! {
        #(#attrs)*
        #[derive(Clone, PartialEq)]
        #vis struct #name #generics #where_clause {
            #(#fields: #types,)*
        }

        impl #impl_generics #name #type_generics #where_clause {
            #vis fn new(#(#required),*) -> Self {
                Self { #(#initializers,)* }
            }
            #(#builders)*
            #private fn __guiraffe_render(#render_params) -> #output #where_clause #block
        }

        impl #impl_generics ::gpui_kit::RenderOnce for #name #type_generics #render_where {
            fn render(self, _: &mut ::gpui_kit::Window, _: &mut ::gpui_kit::App) -> impl ::gpui_kit::IntoElement {
                Self::__guiraffe_render(#(self.#fields),*)
            }
        }

        impl #impl_generics ::gpui_kit::IntoElement for #name #type_generics #render_where {
            type Element = ::gpui_kit::ViewElement<Self>;
            fn into_element(self) -> Self::Element {
                ::gpui_kit::ViewElement::new(self)
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_expansion() -> Result<()> {
        let function = syn::parse_quote! {
            /// Compact user card.
            pub fn UserCard(name: SharedString, #[prop(default)] compact: bool) -> impl IntoElement {
                ui! { div { {name} if compact { "Compact" } } }
            }
        };
        let actual = prettyplease::unparse(&syn::parse2(expand(function)?)?);
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/goldens/component.rs");
        if std::env::var_os("GUIRAFFE_UPDATE_MACRO_GOLDENS").is_some() {
            std::fs::write(path, &actual)
                .map_err(|error| Error::new(proc_macro2::Span::call_site(), error))?;
        }
        let expected = std::fs::read_to_string(path)
            .map_err(|error| Error::new(proc_macro2::Span::call_site(), error))?;
        assert_eq!(actual, expected);
        Ok(())
    }
}
