use proc_macro2::TokenStream;
use quote::quote;
use syn::{Error, FnArg, Ident, ItemFn, Pat, Result, ReturnType, Token, Type, Visibility};

pub(crate) fn expand(function: ItemFn, stateful: bool) -> Result<TokenStream> {
    if stateful {
        return expand_stateful(function);
    }
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

struct Param {
    name: Ident,
    ty: Type,
    default: bool,
}

fn params(function: &ItemFn) -> Result<(Vec<Param>, Vec<Param>)> {
    let mut state = Vec::new();
    let mut props = Vec::new();
    for arg in &function.sig.inputs {
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
        let mut is_state = false;
        let mut default = false;
        for attr in &arg.attrs {
            if attr.path().is_ident("state") {
                is_state = true;
            } else if attr.path().is_ident("prop") {
                let option = attr.parse_args::<Ident>()?;
                if option != "default" {
                    return Err(Error::new_spanned(option, "expected `default`"));
                }
                default = true;
            } else {
                return Err(Error::new_spanned(
                    attr,
                    "expected #[state] or #[prop(default)]",
                ));
            }
        }
        let param = Param {
            name: pat.ident.clone(),
            ty: (*arg.ty).clone(),
            default,
        };
        if is_state {
            state.push(param);
        } else {
            props.push(param);
        }
    }
    Ok((state, props))
}

fn expand_stateful(function: ItemFn) -> Result<TokenStream> {
    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = function.clone();
    if !sig.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &sig.generics,
            "stateful components do not support generics yet",
        ));
    }
    let ReturnType::Type(_, output) = &sig.output else {
        return Err(Error::new_spanned(
            sig.ident,
            "a component must return an element",
        ));
    };
    let (state, props) = params(&function)?;
    if state.is_empty() {
        return Err(Error::new_spanned(
            &sig.ident,
            "a stateful component needs at least one #[state] parameter",
        ));
    }
    let name = &sig.ident;
    let kind = name.to_string();
    let fields = quote::format_ident!("{}Fields", name);
    let state_ty = quote::format_ident!("{}State", name);
    let props_ty = quote::format_ident!("__{}Props", name);
    let controls = quote::format_ident!("__{}Controlled", name);
    let handle = Ident::new("__guiraffe_state", proc_macro2::Span::call_site());
    let m = quote!(::guiraffe_ui::__macro);

    let state_names: Vec<_> = state.iter().map(|p| &p.name).collect();
    let state_types: Vec<_> = state.iter().map(|p| &p.ty).collect();
    let prop_names: Vec<_> = props.iter().map(|p| &p.name).collect();
    let prop_types: Vec<_> = props.iter().map(|p| &p.ty).collect();
    let callbacks: Vec<_> = state_names
        .iter()
        .map(|n| quote::format_ident!("on_{}_change", n))
        .collect();
    let required: Vec<_> = props.iter().filter(|p| !p.default).collect();
    let required_args = required.iter().map(|p| {
        let (n, t) = (&p.name, &p.ty);
        quote!(#n: #t)
    });
    let initializers = props.iter().map(|p| {
        let n = &p.name;
        if p.default {
            quote!(#n: ::core::default::Default::default())
        } else {
            quote!(#n)
        }
    });
    let prop_builders = props.iter().filter(|p| p.default).map(|p| {
        let (n, t) = (&p.name, &p.ty);
        quote! {
            #vis fn #n(mut self, #n: #t) -> Self {
                self.props.#n = #n;
                self
            }
        }
    });
    let render_params = state.iter().chain(&props).map(|p| {
        let (n, t) = (&p.name, &p.ty);
        quote!(#n: #t)
    });
    let single = state.len() == 1;
    let snapshot = if single {
        let n = state_names[0];
        quote!(#m::StateValue::to_value(&self.fields.#n))
    } else {
        let entries = state_names.iter().map(|n| {
            let label = n.to_string();
            quote!((#m::SharedString::new_static(#label), #m::StateValue::to_value(&self.fields.#n)))
        });
        quote!(#m::record([#(#entries),*]))
    };
    let decode = if single {
        let (n, t) = (state_names[0], state_types[0]);
        quote! {
            next.#n = <#t as #m::StateValue>::from_value(&action)
                .ok_or(#m::ActionError::Invalid("value has the wrong type"))?;
        }
    } else {
        let fields = state.iter().map(|p| {
            let (n, t) = (&p.name, &p.ty);
            let label = n.to_string();
            quote! {
                if let ::core::option::Option::Some(value) = #m::field(&action, #label) {
                    next.#n = <#t as #m::StateValue>::from_value(value)
                        .ok_or(#m::ActionError::Invalid(concat!("`", #label, "` has the wrong type")))?;
                }
            }
        });
        quote!(#(#fields)*)
    };
    let commit = state.iter().zip(&callbacks).map(|(p, callback)| {
        let n = &p.name;
        quote! {
            if next.#n != self.fields.#n {
                if self.controlled.#n.is_some() {
                    if let ::core::option::Option::Some(callback) = self.controlled.#callback.clone() {
                        callback(&next.#n, window, cx);
                    }
                } else {
                    self.fields.#n = next.#n;
                    changed = true;
                }
            }
        }
    });
    let mirror = state_names.iter().map(|n| {
        quote! {
            if let ::core::option::Option::Some(value) = &controlled.#n {
                if *value != state.fields.#n {
                    state.fields.#n = ::core::clone::Clone::clone(value);
                }
            }
        }
    });
    let controlled_builders = state.iter().zip(&callbacks).map(|(p, callback)| {
        let (n, t) = (&p.name, &p.ty);
        let default = quote::format_ident!("default_{}", n);
        quote! {
            /// Controlled value: the caller owns it and receives changes through the callback.
            #vis fn #n(mut self, #n: #t) -> Self {
                self.controlled.#n = ::core::option::Option::Some(#n);
                self
            }
            /// Initial value of the component-owned (uncontrolled) state.
            #vis fn #default(mut self, #n: #t) -> Self {
                self.initial.#n = #n;
                self
            }
            #vis fn #callback(
                mut self,
                callback: impl ::core::ops::Fn(&#t, &mut ::gpui_kit::Window, &mut ::gpui_kit::App) + 'static,
            ) -> Self {
                self.controlled.#callback = ::core::option::Option::Some(::std::rc::Rc::new(callback));
                self
            }
        }
    });
    Ok(quote! {
        #[doc = concat!("State of [`", stringify!(#name), "`].")]
        #[derive(Clone, PartialEq, Debug, Default)]
        #vis struct #fields {
            #(pub #state_names: #state_types,)*
        }

        #[doc(hidden)]
        #[derive(Clone)]
        #vis struct #props_ty {
            #(#prop_names: #prop_types,)*
        }

        #[doc(hidden)]
        #[derive(Clone, Default)]
        #vis struct #controls {
            #(#state_names: ::core::option::Option<#state_types>,)*
            #(#callbacks: ::core::option::Option<::std::rc::Rc<dyn ::core::ops::Fn(&#state_types, &mut ::gpui_kit::Window, &mut ::gpui_kit::App)>>,)*
        }

        #(#attrs)*
        #vis struct #name {
            id: #m::SharedString,
            props: #props_ty,
            initial: #fields,
            controlled: #controls,
        }

        #[doc = concat!("The entity behind a [`", stringify!(#name), "`]: owns its state, and is observed and driven over the action bus at the component's id.")]
        #vis struct #state_ty {
            fields: #fields,
            props: ::core::option::Option<#props_ty>,
            controlled: #controls,
            registered: bool,
        }

        impl #name {
            #vis fn new(id: impl ::core::convert::Into<#m::SharedString>, #(#required_args),*) -> Self {
                Self {
                    id: id.into(),
                    props: #props_ty { #(#initializers,)* },
                    initial: ::core::default::Default::default(),
                    controlled: ::core::default::Default::default(),
                }
            }
            #(#prop_builders)*
            #(#controlled_builders)*

            #[allow(clippy::too_many_arguments)]
            fn __guiraffe_render(#handle: #m::StateHandle<#state_ty>, #(#render_params),*) -> #output #block
        }

        impl #state_ty {
            /// The current state.
            #vis fn fields(&self) -> &#fields {
                &self.fields
            }
        }

        impl #m::Stateful for #state_ty {
            type Fields = #fields;
            fn fields(&self) -> &#fields {
                &self.fields
            }
            fn commit(
                &mut self,
                next: #fields,
                window: &mut ::gpui_kit::Window,
                cx: &mut ::gpui_kit::Context<Self>,
            ) {
                let mut changed = false;
                #(#commit)*
                if changed {
                    cx.notify();
                }
            }
        }

        impl ::gpui_kit::Render for #state_ty {
            fn render(
                &mut self,
                _: &mut ::gpui_kit::Window,
                cx: &mut ::gpui_kit::Context<Self>,
            ) -> impl ::gpui_kit::IntoElement {
                let ::core::option::Option::Some(props) = ::core::clone::Clone::clone(&self.props) else {
                    return ::gpui_kit::IntoElement::into_any_element(::gpui_kit::Empty);
                };
                let #fields { #(#state_names,)* } = ::core::clone::Clone::clone(&self.fields);
                let #props_ty { #(#prop_names,)* } = props;
                ::gpui_kit::IntoElement::into_any_element(
                    #name::__guiraffe_render(#m::StateHandle::new(cx.weak_entity()), #(#state_names,)* #(#prop_names),*)
                )
            }
        }

        impl #m::Observe for #state_ty {
            type Snapshot = #m::ControlSnapshot;
            fn snapshot(&self, _: &::gpui_kit::App) -> #m::ControlSnapshot {
                #m::ControlSnapshot {
                    value: ::core::option::Option::Some(#snapshot),
                    ..::core::default::Default::default()
                }
            }
        }

        impl #m::Drive for #state_ty {
            type Action = #m::Value;
            type Output = ();
            fn decode(action: &#m::Action) -> ::core::result::Result<#m::Value, #m::ActionError> {
                if action.name == #m::names::SET_VALUE {
                    ::core::result::Result::Ok(::core::clone::Clone::clone(&action.payload))
                } else {
                    ::core::result::Result::Err(#m::ActionError::Unsupported)
                }
            }
            fn apply(
                &mut self,
                action: #m::Value,
                window: &mut ::gpui_kit::Window,
                cx: &mut ::gpui_kit::Context<Self>,
            ) -> ::core::result::Result<(), #m::ActionError> {
                let mut next = ::core::clone::Clone::clone(&self.fields);
                #decode
                #m::Stateful::commit(self, next, window, cx);
                ::core::result::Result::Ok(())
            }
        }

        impl ::gpui_kit::RenderOnce for #name {
            fn render(self, window: &mut ::gpui_kit::Window, cx: &mut ::gpui_kit::App) -> impl ::gpui_kit::IntoElement {
                let Self { id, props, initial, controlled } = self;
                let entity = window.use_keyed_state(
                    ::gpui_kit::ElementId::Name(::core::clone::Clone::clone(&id)),
                    cx,
                    move |_, _| #state_ty {
                        fields: initial,
                        props: ::core::option::Option::None,
                        controlled: ::core::default::Default::default(),
                        registered: false,
                    },
                );
                let register = entity.update(cx, |state, _| {
                    #(#mirror)*
                    state.props = ::core::option::Option::Some(props);
                    state.controlled = controlled;
                    !::core::mem::replace(&mut state.registered, true)
                });
                if register {
                    #m::ActionBus::register(&id, &entity, #kind, #m::Capabilities::SET_VALUE, window, cx);
                }
                ::gpui_kit::AnyView::from(entity)
            }
        }

        impl ::gpui_kit::IntoElement for #name {
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
        let actual = prettyplease::unparse(&syn::parse2(expand(function, false)?)?);
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/goldens/component.rs");
        if std::env::var_os("GUIRAFFE_UPDATE_MACRO_GOLDENS").is_some() {
            std::fs::write(path, &actual)
                .map_err(|error| Error::new(proc_macro2::Span::call_site(), error))?;
        }
        let expected = std::fs::read_to_string(path)
            .map_err(|error| Error::new(proc_macro2::Span::call_site(), error))?;
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn stateful_component_expansion() -> Result<()> {
        let function = syn::parse_quote! {
            pub fn Counter(#[state] count: i32, step: i32) -> impl IntoElement {
                ui! { div { {count} } }
            }
        };
        let actual = prettyplease::unparse(&syn::parse2(expand(function, true)?)?);
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/goldens/stateful.rs");
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
