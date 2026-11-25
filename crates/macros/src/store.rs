//! `store!` and `#[derive(Lens)]`: state structs with typed field lenses.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    Attribute, Error, Expr, Fields, Ident, ItemStruct, Pat, Result, Token, Type, Visibility,
    braced, parenthesized, punctuated::Punctuated,
};

mod kw {
    syn::custom_keyword!(events);
}

struct StoreField {
    name: Ident,
    ty: Type,
    default: Expr,
    check: Option<Expr>,
    persist: bool,
    no_undo: bool,
    attrs: Vec<Attribute>,
}

struct EventArg {
    name: Ident,
    ty: Type,
}

struct EventDef {
    name: Ident,
    args: Vec<EventArg>,
    state: Pat,
    fx: Option<Pat>,
    body: Expr,
}

pub(crate) struct StoreDef {
    attrs: Vec<Attribute>,
    vis: Visibility,
    name: Ident,
    fields: Vec<StoreField>,
    events: Vec<EventDef>,
}

impl Parse for StoreField {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut attrs = input.call(Attribute::parse_outer)?;
        let name: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty: Type = input.parse()?;
        input.parse::<Token![=]>().map_err(|error| {
            Error::new(
                error.span(),
                format!("store field `{name}` needs a default: `{name}: Type = value`"),
            )
        })?;
        let default = Expr::parse_without_eager_brace(input)?;
        let check = if input.peek(Token![where]) {
            input.parse::<Token![where]>()?;
            Some(Expr::parse_without_eager_brace(input)?)
        } else {
            None
        };
        attrs.extend(input.call(Attribute::parse_outer)?);
        let (mut persist, mut no_undo) = (false, false);
        let mut kept = Vec::new();
        for attr in attrs {
            if attr.path().is_ident("persist") {
                persist = true;
            } else if attr.path().is_ident("no_undo") {
                no_undo = true;
            } else {
                kept.push(attr);
            }
        }
        Ok(Self {
            name,
            ty,
            default,
            check,
            persist,
            no_undo,
            attrs: kept,
        })
    }
}

impl Parse for EventDef {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;
        let mut args = Vec::new();
        if input.peek(syn::token::Paren) {
            let content;
            parenthesized!(content in input);
            while !content.is_empty() {
                let name: Ident = content.parse()?;
                content.parse::<Token![:]>()?;
                let ty: Type = content.parse()?;
                args.push(EventArg { name, ty });
                if content.is_empty() {
                    break;
                }
                content.parse::<Token![,]>()?;
            }
        }
        input.parse::<Token![=>]>()?;
        let closure: syn::ExprClosure = input.parse().map_err(|error| {
            Error::new(
                error.span(),
                "an event handler is a closure: `|s| …` or `|s, fx| …`",
            )
        })?;
        let mut inputs = closure.inputs.into_iter();
        let state = inputs
            .next()
            .ok_or_else(|| Error::new(name.span(), "the handler takes the state: `|s| …`"))?;
        let fx = inputs.next();
        if let Some(extra) = inputs.next() {
            return Err(Error::new_spanned(extra, "expected `|s|` or `|s, fx|`"));
        }
        Ok(Self {
            name,
            args,
            state,
            fx,
            body: *closure.body,
        })
    }
}

impl Parse for StoreDef {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis: Visibility = input.parse()?;
        let name: Ident = input.parse()?;
        let content;
        braced!(content in input);
        let fields = Punctuated::<StoreField, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();
        let mut events = Vec::new();
        if input.peek(kw::events) {
            input.parse::<kw::events>()?;
            let content;
            braced!(content in input);
            events = Punctuated::<EventDef, Token![,]>::parse_terminated(&content)?
                .into_iter()
                .collect();
        }
        if !input.is_empty() {
            return Err(input.error("expected `events { … }` or the end of the store"));
        }
        Ok(Self {
            attrs,
            vis,
            name,
            fields,
            events,
        })
    }
}

fn pascal(name: &Ident) -> Ident {
    let mut out = String::new();
    let mut upper = true;
    for c in name.to_string().trim_start_matches("r#").chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    Ident::new(&out, name.span())
}

/// Lens consts, the path struct and `Lensed` for a struct's named fields.
fn lenses(vis: &Visibility, name: &Ident, fields: &[(&Ident, &Type, TokenStream)]) -> TokenStream {
    let path = format_ident!("{}Path", name);
    let consts = fields.iter().map(|(field, ty, modifiers)| {
        let label = field.to_string();
        quote! {
            #[allow(non_upper_case_globals)]
            #[doc = concat!("Lens onto `", stringify!(#field), "`.")]
            #vis const #field: ::guiraffe_state::Field<#name, #ty> =
                ::guiraffe_state::Field::<#name, #ty>::new(#label, |s| &s.#field, |s| &mut s.#field) #modifiers;
        }
    });
    let methods = fields.iter().map(|(field, ty, _)| {
        quote! {
            #vis fn #field(self) -> ::guiraffe_state::Compose<L, ::guiraffe_state::Field<#name, #ty>> {
                ::guiraffe_state::Lens::then(self.0, #name::#field)
            }
        }
    });
    quote! {
        impl #name {
            #(#consts)*
        }

        #[doc(hidden)]
        #[derive(Clone, Copy)]
        #vis struct #path<L>(L);

        impl<L: ::guiraffe_state::Lens<Target = #name>> #path<L> {
            #(#methods)*
            /// The lens built so far.
            #vis fn lens(self) -> L {
                self.0
            }
        }

        impl ::guiraffe_state::Lensed for #name {
            type Path<L: ::guiraffe_state::Lens<Target = Self>> = #path<L>;
            fn wrap<L: ::guiraffe_state::Lens<Target = Self>>(lens: L) -> #path<L> {
                #path(lens)
            }
        }
    }
}

impl StoreDef {
    pub(crate) fn expand(self) -> TokenStream {
        let Self {
            attrs,
            vis,
            name,
            fields,
            events,
        } = self;
        let declared = fields.iter().map(|f| {
            let (field, ty, attrs) = (&f.name, &f.ty, &f.attrs);
            quote!(#(#attrs)* pub #field: #ty)
        });
        let defaults = fields.iter().map(|f| {
            let (field, default) = (&f.name, &f.default);
            quote!(#field: #default)
        });
        let modifiers: Vec<_> = fields
            .iter()
            .map(|f| {
                let ty = &f.ty;
                let check = f.check.as_ref().map(|check| match check {
                    Expr::Range(range) => quote! {
                        .with_check(|v: #ty| ::core::option::Option::Some(
                            ::guiraffe_state::lens::clamp(v, #range)))
                    },
                    predicate => quote! {
                        .with_check(|v: #ty| {
                            let check: fn(&#ty) -> bool = #predicate;
                            if check(&v) { ::core::option::Option::Some(v) }
                            else { ::core::option::Option::None }
                        })
                    },
                });
                let no_undo = f.no_undo.then(|| quote!(.no_undo()));
                let persist = f.persist.then(|| quote!(.persist()));
                quote!(#check #no_undo #persist)
            })
            .collect();
        let lens_fields: Vec<_> = fields
            .iter()
            .zip(modifiers)
            .map(|(f, m)| (&f.name, &f.ty, m))
            .collect();
        let lenses = lenses(&vis, &name, &lens_fields);
        let setters = fields.iter().map(|f| {
            let (field, ty) = (&f.name, &f.ty);
            let event = format_ident!("Set{}", pascal(field));
            quote! {
                #[doc = concat!("Set `", stringify!(#field), "` (validated, keyed to the field).")]
                #[derive(Debug, Clone)]
                #vis struct #event(pub #ty);

                impl ::guiraffe_state::Event<#name> for #event {
                    fn handle(self, fx: &mut ::guiraffe_state::Fx<'_, #name>) {
                        fx.set_at(#name::#field, self.0);
                    }
                    fn undoable(&self) -> bool {
                        ::guiraffe_state::Lens::undoable(&#name::#field)
                    }
                }
            }
        });
        let events = events.iter().map(|event| expand_event(&vis, &name, event));
        let store_fn = quote! {
            impl #name {
                /// A store holding the default state.
                #vis fn store(cx: &mut ::gpui_kit::App) -> ::guiraffe_state::Store<#name> {
                    ::guiraffe_state::Store::new(<#name as ::core::default::Default>::default(), cx)
                }
            }
        };
        quote! {
            #(#attrs)*
            #[derive(Clone, PartialEq, Debug)]
            #vis struct #name {
                #(#declared,)*
            }

            impl ::core::default::Default for #name {
                fn default() -> Self {
                    Self { #(#defaults,)* }
                }
            }

            #store_fn
            #lenses
            #(#setters)*
            #(#events)*
        }
    }
}

fn expand_event(vis: &Visibility, store: &Ident, event: &EventDef) -> TokenStream {
    let EventDef {
        name,
        args,
        state,
        fx,
        body,
    } = event;
    let types = args.iter().map(|arg| &arg.ty);
    let names: Vec<_> = args.iter().map(|arg| &arg.name).collect();
    let declaration = if args.is_empty() {
        quote!(#vis struct #name;)
    } else {
        quote!(#vis struct #name(#(pub #types),*);)
    };
    let destructure = if args.is_empty() {
        quote!()
    } else {
        quote!(let #name(#(#names),*) = self;)
    };
    let fx_param = Ident::new("__guiraffe_fx", Span::mixed_site());
    let handle = match fx {
        None => quote! {
            #destructure
            #fx_param.update(move |#state: &mut #store| { let _ = #body; });
        },
        Some(fx) => {
            let next = Ident::new("__guiraffe_next", Span::mixed_site());
            quote! {
                #destructure
                let mut #next = ::core::clone::Clone::clone(#fx_param.state());
                {
                    let #state: &mut #store = &mut #next;
                    let #fx: &mut ::guiraffe_state::Fx<'_, #store> = &mut *#fx_param;
                    let _ = #body;
                }
                #fx_param.set(#next);
            }
        }
    };
    quote! {
        #[derive(Debug)]
        #declaration

        impl ::guiraffe_state::Event<#store> for #name {
            #[allow(unused_variables)]
            fn handle(self, #fx_param: &mut ::guiraffe_state::Fx<'_, #store>) {
                #handle
            }
        }
    }
}

/// `#[derive(Lens)]` for nested state structs.
pub(crate) fn derive_lens(item: ItemStruct) -> Result<TokenStream> {
    let Fields::Named(named) = &item.fields else {
        return Err(Error::new_spanned(
            &item.ident,
            "#[derive(Lens)] needs a struct with named fields",
        ));
    };
    if !item.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &item.generics,
            "#[derive(Lens)] does not support generic structs yet",
        ));
    }
    let fields: Vec<_> = named
        .named
        .iter()
        .filter_map(|f| f.ident.as_ref().map(|name| (name, &f.ty, quote!())))
        .collect();
    Ok(lenses(&item.vis, &item.ident, &fields))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_expansion() -> Result<()> {
        let store: StoreDef = syn::parse_quote! {
            pub Settings {
                theme: Theme = Theme::System,
                font_size: f32 = 14.0 where 8.0..=48.0,
                recent: Vec<PathBuf> = vec![] #[persist],
                draft: Draft = Draft::default() #[no_undo],
            }
            events {
                ResetFonts => |s| s.font_size = 14.0,
                Open(path: PathBuf) => |s, fx| { s.recent.insert(0, path.clone()); fx.effect(OpenFile(path)) },
            }
        };
        let actual = prettyplease::unparse(&syn::parse2(store.expand())?);
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/goldens/store.rs");
        if std::env::var_os("GUIRAFFE_UPDATE_MACRO_GOLDENS").is_some() {
            std::fs::write(path, &actual).map_err(|e| Error::new(Span::call_site(), e))?;
        }
        let expected =
            std::fs::read_to_string(path).map_err(|e| Error::new(Span::call_site(), e))?;
        assert_eq!(actual, expected);
        Ok(())
    }
}
