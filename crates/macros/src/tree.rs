use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{
    Error, Expr, Ident, LitStr, Pat, Path, PathArguments, Result, Token, braced, parenthesized,
    punctuated::Punctuated, token,
};

mod kw {
    syn::custom_keyword!(key);
}

pub(crate) struct Root(Vec<Node>);

struct Element {
    path: Path,
    constructor: bool,
    args: Punctuated<Expr, Token![,]>,
    items: Vec<Item>,
}

enum Item {
    Builder(TokenStream),
    Child(Box<Node>),
}

enum Node {
    Element(Element),
    Value(TokenStream),
    View(Expr),
    Cached(Span, Expr, Vec<Node>),
    If(Expr, Vec<Node>, Option<Vec<Node>>),
    Match(Expr, Vec<(Pat, Option<Expr>, Node)>),
    For(Box<Pat>, Expr, Option<Box<Expr>>, Vec<Node>),
}

impl Parse for Root {
    fn parse(input: ParseStream) -> Result<Self> {
        nodes(input).map(Self)
    }
}

fn separators(input: ParseStream) -> Result<()> {
    while input.peek(Token![,]) || input.peek(Token![;]) {
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        } else {
            input.parse::<Token![;]>()?;
        }
    }
    Ok(())
}

fn nodes(input: ParseStream) -> Result<Vec<Node>> {
    let mut nodes = Vec::new();
    separators(input)?;
    while !input.is_empty() {
        nodes.push(input.parse()?);
        separators(input)?;
    }
    Ok(nodes)
}

fn block(input: ParseStream) -> Result<Vec<Node>> {
    let content;
    braced!(content in input);
    nodes(&content)
}

fn reserved(input: ParseStream) -> Result<()> {
    if input.peek(kw::key) {
        let token = input.parse::<kw::key>()?;
        return Err(Error::new_spanned(
            token,
            "`key` is only allowed after a `for` iterator",
        ));
    }
    if input.peek(Token![else]) {
        let token = input.parse::<Token![else]>()?;
        return Err(Error::new_spanned(token, "`else` needs a preceding `if`"));
    }
    Ok(())
}

fn upper(ident: &Ident) -> bool {
    ident
        .to_string()
        .trim_start_matches("r#")
        .starts_with(char::is_uppercase)
}

impl Parse for Item {
    fn parse(input: ParseStream) -> Result<Self> {
        reserved(input)?;
        if input.peek(Token![.]) {
            let dot = input.parse::<Token![.]>()?;
            let method = input.parse::<Ident>()?;
            let generics = if input.peek(Token![::]) {
                Some(input.parse::<syn::AngleBracketedGenericArguments>()?)
            } else {
                None
            };
            let args;
            let parens = parenthesized!(args in input);
            let args: TokenStream = args.parse()?;
            let mut call = quote!(#dot #method #generics);
            parens.surround(&mut call, |tokens| tokens.extend(args));
            return Ok(Self::Builder(call));
        }
        if input.peek(Ident) && !input.peek2(Token![::]) {
            let fork = input.fork();
            let name = fork.parse::<Ident>()?;
            if fork.peek(Token![:]) {
                input.parse::<Ident>()?;
                input.parse::<Token![:]>()?;
                let value = Expr::parse_without_eager_brace(input).map_err(|error| {
                    Error::new(
                        error.span(),
                        format!("expected a Rust expression for prop `{name}`: {error}"),
                    )
                })?;
                return Ok(Self::Builder(quote_spanned!(name.span()=> .#name(#value))));
            }
            if !upper(&name) && !fork.peek(token::Paren) && !fork.peek(token::Brace) {
                input.parse::<Ident>()?;
                return Ok(Self::Builder(quote_spanned!(name.span()=> .#name())));
            }
        }
        input.parse().map(|node| Self::Child(Box::new(node)))
    }
}

impl Parse for Node {
    fn parse(input: ParseStream) -> Result<Self> {
        reserved(input)?;
        if input.peek(Token![if]) {
            input.parse::<Token![if]>()?;
            let condition = Expr::parse_without_eager_brace(input)?;
            let yes = block(input)?;
            let no = if input.peek(Token![else]) {
                input.parse::<Token![else]>()?;
                Some(if input.peek(Token![if]) {
                    vec![input.parse()?]
                } else {
                    block(input)?
                })
            } else {
                None
            };
            return Ok(Self::If(condition, yes, no));
        }
        if input.peek(Token![match]) {
            input.parse::<Token![match]>()?;
            let value = Expr::parse_without_eager_brace(input)?;
            let content;
            braced!(content in input);
            let mut arms = Vec::new();
            while !content.is_empty() {
                let pattern = Pat::parse_multi_with_leading_vert(&content)?;
                let guard = if content.peek(Token![if]) {
                    content.parse::<Token![if]>()?;
                    Some(content.parse()?)
                } else {
                    None
                };
                content.parse::<Token![=>]>()?;
                arms.push((pattern, guard, content.parse()?));
                separators(&content)?;
            }
            return Ok(Self::Match(value, arms));
        }
        if input.peek(Token![for]) {
            input.parse::<Token![for]>()?;
            let pattern = Pat::parse_single(input)?;
            input.parse::<Token![in]>()?;
            let iter = Expr::parse_without_eager_brace(input)?;
            let key = if input.peek(kw::key) {
                input.parse::<kw::key>()?;
                Some(Box::new(Expr::parse_without_eager_brace(input)?))
            } else {
                None
            };
            let body = block(input)?;
            if key.is_none() {
                if let Some(span) = body.iter().find_map(Node::cache_span) {
                    return Err(Error::new(span, "`cached` in `for` requires `key <expr>`"));
                }
                if let Some(path) = body.iter().find_map(Node::interactive) {
                    return Err(Error::new_spanned(
                        path,
                        "an interactive element with an id in `for` requires `key <expr>`",
                    ));
                }
            }
            return Ok(Self::For(Box::new(pattern), iter, key, body));
        }
        if input.peek(LitStr) {
            return Ok(Self::Value(input.parse::<LitStr>()?.into_token_stream()));
        }
        if input.peek(token::Brace) {
            let value = input.parse::<syn::ExprBlock>()?;
            // A single expression needs no Rust block and retains its source span.
            if let [syn::Stmt::Expr(expression, None)] = value.block.stmts.as_slice() {
                return Ok(Self::Value(expression.to_token_stream()));
            }
            return Ok(Self::Value(value.into_token_stream()));
        }
        if !(input.peek(Ident)
            || input.peek(Token![::])
            || input.peek(Token![crate])
            || input.peek(Token![super])
            || input.peek(Token![self])
            || input.peek(Token![Self]))
        {
            return Err(input.error(
                "unknown UI item; expected an element, text, { expression }, if, match, or for",
            ));
        }
        let path: Path = input.parse()?;
        if path.is_ident("view") || path.is_ident("cached") {
            let args;
            parenthesized!(args in input);
            let value = args.parse()?;
            if !args.is_empty() {
                return Err(args.error("expected exactly one expression"));
            }
            return if path.is_ident("view") {
                Ok(Self::View(value))
            } else {
                Ok(Self::Cached(path.span(), value, block(input)?))
            };
        }
        let constructor = path
            .segments
            .last()
            .is_some_and(|segment| upper(&segment.ident));
        if !constructor && !input.peek(token::Paren) && !input.peek(token::Brace) {
            return Err(Error::new_spanned(
                path,
                "expected constructor arguments or an element body",
            ));
        }
        let args = if input.peek(token::Paren) {
            let args;
            parenthesized!(args in input);
            args.parse_terminated(Expr::parse, Token![,])?
        } else {
            Punctuated::new()
        };
        let mut items = Vec::new();
        if input.peek(token::Brace) {
            let content;
            braced!(content in input);
            separators(&content)?;
            while !content.is_empty() {
                items.push(content.parse()?);
                separators(&content)?;
            }
        }
        Ok(Self::Element(Element {
            path,
            constructor,
            args,
            items,
        }))
    }
}

impl Root {
    pub(crate) fn expand(self) -> TokenStream {
        if let Some(node) = direct(&self.0) {
            return node.expand();
        }
        collect(&self.0)
    }
}

fn erase(tokens: TokenStream) -> TokenStream {
    quote!(::gpui_kit::IntoElement::into_any_element(#tokens))
}

fn collect(nodes: &[Node]) -> TokenStream {
    let children = Ident::new("__guiraffe_children", proc_macro2::Span::mixed_site());
    let operations = nodes.iter().map(|node| {
        let tokens = node.expand();
        if node.spreads() {
            quote!(#children.extend((#tokens).into_iter().map(::gpui_kit::IntoElement::into_any_element));)
        } else {
            let erased = erase(tokens);
            quote!(#children.push(#erased);)
        }
    });
    quote!({
        let mut #children: ::std::vec::Vec<::gpui_kit::AnyElement> = ::std::vec::Vec::new();
        #(#operations)*
        #children
    })
}

fn direct(nodes: &[Node]) -> Option<&Node> {
    match nodes {
        [node] if !node.spreads() => Some(node),
        _ => None,
    }
}

fn head(node: &Node) -> Option<String> {
    match node {
        Node::Element(element) => Some(element.path.to_token_stream().to_string()),
        Node::Value(tokens) if syn::parse2::<LitStr>(tokens.clone()).is_ok() => Some("\"".into()),
        Node::If(_, yes, Some(no)) => {
            let yes = head(direct(yes)?)?;
            (head(direct(no)?)? == yes).then_some(yes)
        }
        _ => None,
    }
}

fn same_heads<'a>(nodes: impl IntoIterator<Item = &'a Node>) -> bool {
    let mut heads = nodes.into_iter().map(head);
    heads
        .next()
        .flatten()
        .is_some_and(|first| heads.all(|head| head.as_ref() == Some(&first)))
}

impl Node {
    fn cache_span(&self) -> Option<Span> {
        match self {
            Self::Cached(span, ..) => Some(*span),
            Self::Element(element) => element.items.iter().find_map(|item| match item {
                Item::Child(child) => child.cache_span(),
                Item::Builder(_) => None,
            }),
            Self::If(_, yes, no) => yes
                .iter()
                .chain(no.iter().flatten())
                .find_map(Self::cache_span),
            Self::Match(_, arms) => arms.iter().find_map(|(_, _, node)| node.cache_span()),
            Self::For(_, _, _, nodes) => nodes.iter().find_map(Self::cache_span),
            Self::Value(_) | Self::View(_) => None,
        }
    }

    fn interactive(&self) -> Option<&Path> {
        match self {
            Self::Element(element) => {
                // Constructor ids of known interactive builders and explicit .id()/id:.
                let known = element.path.segments.last().is_some_and(|segment| {
                    matches!(
                        segment.ident.to_string().as_str(),
                        "Button"
                            | "Checkbox"
                            | "Switch"
                            | "Radio"
                            | "Tabs"
                            | "Slider"
                            | "ActionIcon"
                            | "UnstyledButton"
                            | "Chip"
                            | "MenuItem"
                    )
                });
                let explicit = element.items.iter().any(|item| match item {
                    Item::Builder(call) => matches!(call.clone().into_iter().nth(1), Some(proc_macro2::TokenTree::Ident(name)) if name == "id"),
                    Item::Child(_) => false,
                });
                if explicit || (known && !element.args.is_empty()) {
                    return Some(&element.path);
                }
                element.items.iter().find_map(|item| match item {
                    Item::Child(child) => child.interactive(),
                    Item::Builder(_) => None,
                })
            }
            Self::If(_, yes, no) => yes
                .iter()
                .chain(no.iter().flatten())
                .find_map(Self::interactive),
            Self::Cached(..) => None,
            Self::Match(_, arms) => arms.iter().find_map(|(_, _, node)| node.interactive()),
            Self::For(..) | Self::Value(_) | Self::View(_) => None,
        }
    }

    fn spreads(&self) -> bool {
        match self {
            Self::If(_, _, None) | Self::For(..) => true,
            Self::If(_, yes, Some(no)) => direct(yes).is_none() || direct(no).is_none(),
            Self::Match(_, arms) => arms.iter().any(|(_, _, node)| node.spreads()),
            _ => false,
        }
    }

    fn apply(&self, receiver: TokenStream) -> TokenStream {
        let child = self.expand();
        if self.spreads() {
            quote!(#receiver.children(#child))
        } else {
            quote!(#receiver.child(#child))
        }
    }

    fn expand(&self) -> TokenStream {
        match self {
            Self::Element(Element {
                path,
                constructor,
                args,
                items,
            }) => {
                // Convert path type arguments to turbofish syntax in expressions.
                let mut path = path.clone();
                for segment in &mut path.segments {
                    if let PathArguments::AngleBracketed(arguments) = &mut segment.arguments {
                        arguments.colon2_token.get_or_insert_with(Default::default);
                    }
                }
                let base = if *constructor {
                    quote_spanned!(path.span()=> #path::new(#args))
                } else {
                    quote!(#path(#args))
                };
                items.iter().fold(base, |result, item| match item {
                    Item::Builder(call) => quote!(#result #call),
                    Item::Child(node) => node.apply(result),
                })
            }
            Self::Value(value) => value.clone(),
            Self::View(value) => quote!(#value),
            Self::Cached(_, style, nodes) => {
                let body = direct(nodes).map_or_else(
                    || {
                        let children = collect(nodes);
                        quote!(::gpui_kit::ParentElement::children(::gpui_kit::div(), #children))
                    },
                    Self::expand,
                );
                quote_spanned!(style.span()=> ::guiraffe_ui::__macro::cached(#style, move || #body))
            }
            Self::If(condition, yes, no) => {
                if let Some(no) = no {
                    let (yes, no) = match (direct(yes), direct(no)) {
                        (Some(yes), Some(no)) if same_heads([yes, no]) => {
                            (yes.expand(), no.expand())
                        }
                        (Some(yes), Some(no)) => (erase(yes.expand()), erase(no.expand())),
                        _ => (collect(yes), collect(no)),
                    };
                    quote!(if #condition { #yes } else { #no })
                } else if let Some(node) = direct(yes) {
                    let child = node.expand();
                    quote!(if #condition { ::core::option::Option::Some(#child) } else { ::core::option::Option::None })
                } else {
                    let children = collect(yes);
                    quote!(if #condition { #children } else { ::std::vec::Vec::new() })
                }
            }
            Self::Match(value, arms) => {
                let homogeneous = same_heads(arms.iter().map(|(_, _, node)| node));
                let spreads = self.spreads();
                let arms = arms.iter().map(|(pattern, guard, node)| {
                    let guard = guard.as_ref().map(|guard| quote!(if #guard));
                    let body = if spreads {
                        collect(std::slice::from_ref(node))
                    } else if homogeneous {
                        node.expand()
                    } else {
                        erase(node.expand())
                    };
                    quote!(#pattern #guard => #body,)
                });
                quote!(match #value { #(#arms)* })
            }
            Self::For(pattern, iter, key, nodes) => {
                if let Some(key) = key {
                    let body = direct(nodes).map_or_else(|| {
                        let children = collect(nodes);
                        quote!(::gpui_kit::ParentElement::children(::gpui_kit::div(), #children))
                    }, Self::expand);
                    let id = Ident::new("__guiraffe_key", proc_macro2::Span::mixed_site());
                    quote!((#iter).into_iter().map(|#pattern| {
                        let #id = #key;
                        ::guiraffe_ui::__macro::keyed(#id, #body)
                    }))
                } else if let Some(node) = direct(nodes) {
                    let body = node.expand();
                    quote!((#iter).into_iter().map(|#pattern| #body))
                } else {
                    let body = collect(nodes);
                    quote!((#iter).into_iter().flat_map(|#pattern| #body))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builder_items_and_children_in_source_order() -> Result<()> {
        let Root(nodes) =
            syn::parse_quote!(div { flex "literal" gap: pixels; {value} .method::<u8>(1) });
        let [Node::Element(element)] = nodes.as_slice() else {
            panic!("expected one element");
        };
        assert!(matches!(
            element.items.as_slice(),
            [
                Item::Builder(_),
                Item::Child(_),
                Item::Builder(_),
                Item::Child(_),
                Item::Builder(_),
            ]
        ));
        Ok(())
    }

    #[test]
    fn grammar_expansion() -> Result<()> {
        let root: Root = syn::parse_quote! {
            layouts::Stack {
                gap: Size::Md
                Title("Settings") { order: 2 }
                div() {
                    flex gap_2 p: px(8.);
                    .when(dense, |d| d.p(px(2.)))
                    .test::<u8>(1)
                    "raw div" {built}
                }
                if let Some(error) = &error { Text(error.clone()) }
                if dense { Text("Dense") } else if roomy { Text("Roomy") } else { Text("Normal") }
                match tab { Tab::A => Text("A"), Tab::B => div { "B" } }
                for row in &rows key row.id { RowView(row) }
                for line in lines { {line} }
                view(chart.clone())
                cached(style) { HeavyTable(data.clone()) }
            }
        };
        let expansion = root.expand();
        let rust: syn::File = syn::parse2(quote!(fn snapshot() { let _ = #expansion; }))?;
        let actual = prettyplease::unparse(&rust);
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/goldens/grammar.rs");
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
