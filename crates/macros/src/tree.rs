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
    syn::custom_keyword!(via);
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
    /// `prop <= lens` or `prop <=> lens [via on_x]`.
    Bind {
        prop: Ident,
        lens: LensPath,
        handler: Option<Ident>,
    },
    /// `on_x => Event(args)` dispatches into the scope's store;
    /// `on_x => field op= value` mutates a stateful component's state.
    Handle(Ident, Handler),
}

enum Handler {
    Dispatch(Expr),
    Mutate(Expr),
}

/// `field.field.by_key(k).field`: a path resolved against the scope's store or a row item.
#[derive(Clone)]
struct LensPath {
    root: Ident,
    steps: Vec<Step>,
}

#[derive(Clone)]
enum Step {
    Field(Ident),
    ByKey(Ident, Expr),
}

/// Options after `for row in <= lens`.
struct ListOptions {
    mounted: bool,
    row_height: Option<Expr>,
}

enum Node {
    Element(Element),
    Value(TokenStream),
    View(Expr),
    Cached(Span, Expr, Vec<Node>),
    If(Expr, Vec<Node>, Option<Vec<Node>>),
    Match(Expr, Vec<(Pat, Option<Expr>, Node)>),
    For(Box<Pat>, Expr, Option<Box<Expr>>, Vec<Node>),
    /// `store(expr) { … }`
    Store(Expr, Vec<Node>),
    /// `{<= lens}`
    BoundText(LensPath),
    /// `if <= lens { … } else { … }`
    BoundIf(LensPath, Vec<Node>, Option<Vec<Node>>),
    /// `for row in <= lens [mounted] [row_height: h] { … }`, or `for (key, row) in …`
    BoundFor(Option<Ident>, Ident, LensPath, ListOptions, Vec<Node>),
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

fn is_compound(op: &syn::BinOp) -> bool {
    use syn::BinOp::*;
    matches!(
        op,
        AddAssign(_)
            | SubAssign(_)
            | MulAssign(_)
            | DivAssign(_)
            | RemAssign(_)
            | BitXorAssign(_)
            | BitAndAssign(_)
            | BitOrAssign(_)
            | ShlAssign(_)
            | ShrAssign(_)
    )
}

/// The change callback a two-way binding pairs with (conventions §3).
fn change_handler(prop: &Ident) -> Ident {
    let name = prop.to_string();
    match name.as_str() {
        "value" | "checked" | "selected" => Ident::new("on_change", prop.span()),
        _ => Ident::new(&format!("on_{name}_change"), prop.span()),
    }
}

impl Parse for LensPath {
    fn parse(input: ParseStream) -> Result<Self> {
        let root: Ident = input.parse().map_err(|error| {
            Error::new(
                error.span(),
                "expected a lens: a store field such as `font_size`",
            )
        })?;
        let mut steps = Vec::new();
        while input.peek(Token![.]) && !input.peek2(Token![.]) {
            // `.method(…)` other than `by_key` ends the lens: it is the next builder call.
            let fork = input.fork();
            fork.parse::<Token![.]>()?;
            let next: Ident = fork.parse()?;
            if fork.peek(token::Paren) && next != "by_key" {
                break;
            }
            input.parse::<Token![.]>()?;
            let name: Ident = input.parse()?;
            if input.peek(token::Paren) {
                let args;
                parenthesized!(args in input);
                steps.push(Step::ByKey(name, args.parse()?));
            } else {
                steps.push(Step::Field(name));
            }
        }
        Ok(Self { root, steps })
    }
}

fn bind_operator(input: ParseStream) -> Result<Option<bool>> {
    if input.peek(Token![<]) && input.peek2(Token![-]) {
        return Err(input.error("bindings use `<=` (one-way) or `<=>` (two-way), not `<-`"));
    }
    if !input.peek(Token![<=]) {
        return Ok(None);
    }
    input.parse::<Token![<=]>()?;
    if input.peek(Token![>]) {
        input.parse::<Token![>]>()?;
        return Ok(Some(true));
    }
    Ok(Some(false))
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
            if fork.peek(Token![<=]) || (fork.peek(Token![<]) && fork.peek2(Token![-])) {
                input.parse::<Ident>()?;
                let two_way = bind_operator(input)?.unwrap_or(false);
                let lens: LensPath = input.parse()?;
                let handler = if input.peek(kw::via) {
                    input.parse::<kw::via>()?;
                    if !two_way {
                        return Err(input
                            .error("`via` names the change callback of a two-way `<=>` binding"));
                    }
                    Some(input.parse::<Ident>()?)
                } else if two_way {
                    Some(change_handler(&name))
                } else {
                    None
                };
                return Ok(Self::Bind {
                    prop: name,
                    lens,
                    handler,
                });
            }
            if fork.peek(Token![=>]) {
                input.parse::<Ident>()?;
                input.parse::<Token![=>]>()?;
                let value = Expr::parse_without_eager_brace(input)?;
                let mutate = matches!(&value, Expr::Assign(_))
                    || matches!(&value, Expr::Binary(binary) if is_compound(&binary.op));
                return Ok(Self::Handle(
                    name,
                    if mutate {
                        Handler::Mutate(value)
                    } else {
                        Handler::Dispatch(value)
                    },
                ));
            }
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
        if input.peek(Token![if]) && input.peek2(Token![<=]) {
            input.parse::<Token![if]>()?;
            input.parse::<Token![<=]>()?;
            let lens: LensPath = input.parse()?;
            let yes = block(input)?;
            let no = if input.peek(Token![else]) {
                input.parse::<Token![else]>()?;
                Some(block(input)?)
            } else {
                None
            };
            return Ok(Self::BoundIf(lens, yes, no));
        }
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
            if input.peek(Token![<=]) {
                input.parse::<Token![<=]>()?;
                let names = |pattern: &Pat| match pattern {
                    Pat::Ident(syn::PatIdent { ident, .. }) => Some((None, ident.clone())),
                    Pat::Tuple(tuple) => match tuple.elems.iter().collect::<Vec<_>>().as_slice() {
                        [Pat::Ident(key), Pat::Ident(item)] => {
                            Some((Some(key.ident.clone()), item.ident.clone()))
                        }
                        _ => None,
                    },
                    _ => None,
                };
                let Some((key, item)) = names(&pattern) else {
                    return Err(Error::new_spanned(
                        pattern,
                        "a store-bound list binds `row` or `(key, row)`: `for row in <= rows { … }`",
                    ));
                };
                let lens: LensPath = input.parse()?;
                let mut options = ListOptions {
                    mounted: false,
                    row_height: None,
                };
                while !input.peek(token::Brace) {
                    separators(input)?;
                    if input.peek(kw::key) {
                        return Err(input.error(
                            "store-bound lists are keyed by their KeyedList keys; remove `key`",
                        ));
                    }
                    let option: Ident = input.parse()?;
                    if option == "mounted" {
                        options.mounted = true;
                    } else if option == "row_height" {
                        input.parse::<Token![:]>()?;
                        options.row_height = Some(Expr::parse_without_eager_brace(input)?);
                    } else {
                        return Err(Error::new_spanned(
                            option,
                            "expected `mounted` or `row_height: <pixels>`",
                        ));
                    }
                }
                let body = block(input)?;
                return Ok(Self::BoundFor(key, item, lens, options, body));
            }
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
            let fork = input.fork();
            let content;
            braced!(content in fork);
            if content.peek(Token![<=]) {
                let content;
                braced!(content in input);
                content.parse::<Token![<=]>()?;
                let lens = content.parse()?;
                if !content.is_empty() {
                    return Err(content.error("expected the end of the bound read"));
                }
                return Ok(Self::BoundText(lens));
            }
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
        if path.is_ident("store") && input.peek(token::Paren) {
            let args;
            parenthesized!(args in input);
            let store = args.parse()?;
            if !args.is_empty() {
                return Err(args.error("expected exactly one store expression"));
            }
            return Ok(Self::Store(store, block(input)?));
        }
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

/// A protocol control entity (`SliderControl`, `CheckboxControl`, …) with one binding lowers
/// to `bound_control`: built once, host writes through its non-emitting setter, edits from its
/// typed event. Everything else in the body configures its builder.
fn control_binding(
    path: &Path,
    args: &Punctuated<Expr, Token![,]>,
    items: &[Item],
) -> Option<TokenStream> {
    let name = path.segments.last()?.ident.to_string();
    if !name.ends_with("Control") {
        return None;
    }
    let binds: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            Item::Bind {
                lens,
                handler,
                prop,
            } => Some((prop, lens, handler.is_some())),
            _ => None,
        })
        .collect();
    let [(prop, lens, two_way)] = binds.as_slice() else {
        if binds.len() > 1 {
            return Some(
                Error::new_spanned(path, "a control entity binds one value").into_compile_error(),
            );
        }
        return None;
    };
    if !args.is_empty() {
        return Some(
            Error::new_spanned(args, "a bound control entity takes its id from `.path(…)`")
                .into_compile_error(),
        );
    }
    let mut configure = Vec::new();
    for item in items {
        match item {
            Item::Builder(call) => configure.push(call.clone()),
            Item::Bind { .. } => {}
            Item::Child(_) | Item::Handle(..) => {
                return Some(
                    Error::new_spanned(
                        path,
                        "a bound control entity takes builder props only; edits arrive through the binding",
                    )
                    .into_compile_error(),
                );
            }
        }
    }
    let _ = prop;
    let builder = Ident::new("__guiraffe_builder", Span::mixed_site());
    let store = store_ident();
    let id = site_id();
    let lens = lens.expand();
    Some(
        quote_spanned!(path.span()=> ::guiraffe_ui::__macro::bound_control::<#path, _, _>(
            #id,
            #store,
            #lens,
            #two_way,
            move |#builder| #builder #(#configure)*,
        )),
    )
}

/// `count -= step` becomes `__guiraffe_fields.count -= step`.
fn mutation(change: &Expr) -> TokenStream {
    let fields = Ident::new("__guiraffe_fields", Span::mixed_site());
    let target = |left: &Expr| match left {
        Expr::Path(path) if path.path.get_ident().is_some() => {
            let name = path.path.get_ident();
            quote!(#fields.#name)
        }
        other => quote!(#other),
    };
    match change {
        Expr::Assign(assign) => {
            let left = target(&assign.left);
            let right = &assign.right;
            quote!(#left = #right)
        }
        Expr::Binary(binary) => {
            let left = target(&binary.left);
            let (op, right) = (&binary.op, &binary.right);
            quote!(#left #op #right)
        }
        other => quote!(#other),
    }
}

/// Expansion state: whether a `store(…)` scope encloses the node, which names are row items
/// of enclosing bound lists, and a counter that gives each bound site its own element id.
#[derive(Default)]
struct Scope {
    stores: usize,
    items: Vec<Ident>,
    sites: u64,
}

thread_local! {
    static SCOPE: std::cell::RefCell<Scope> = std::cell::RefCell::default();
}

fn next_site() -> u64 {
    SCOPE.with(|scope| {
        let mut scope = scope.borrow_mut();
        scope.sites += 1;
        scope.sites
    })
}

fn is_item(name: &Ident) -> bool {
    SCOPE.with(|scope| scope.borrow().items.iter().any(|item| item == name))
}

fn store_ident() -> Ident {
    Ident::new("__guiraffe_store", Span::mixed_site())
}

fn item_ident(name: &Ident) -> Ident {
    Ident::new(&format!("__guiraffe_item_{name}"), Span::mixed_site())
}

/// The element id of the next bound site: the `ui!` call site plus a per-invocation counter.
fn site_id() -> TokenStream {
    let site = next_site() as usize;
    quote!(::gpui_kit::ElementId::from((
        ::core::concat!(::core::file!(), ":", ::core::line!(), ":", ::core::column!()),
        #site as usize,
    )))
}

impl LensPath {
    fn expand(&self) -> TokenStream {
        let root = &self.root;
        let mut lens = if is_item(root) {
            let item = item_ident(root);
            quote!(#item)
        } else {
            let store = store_ident();
            quote_spanned!(root.span()=> ::guiraffe_ui::__macro::root(#store).#root())
        };
        for step in &self.steps {
            lens = match step {
                Step::Field(name) => {
                    quote_spanned!(name.span()=> ::guiraffe_ui::__macro::Lens::path(#lens).#name())
                }
                Step::ByKey(name, key) => {
                    quote_spanned!(name.span()=> ::guiraffe_ui::__macro::ListLens::by_key(#lens, #key))
                }
            };
        }
        lens
    }
}

/// Wrap `body` (which may refer to `__guiraffe_store` and `__guiraffe_bound`) in a bound view
/// over `lenses`. `values` are the identifiers the body uses for each bound value.
fn bound_site(lenses: &[TokenStream], values: &[Ident], body: TokenStream) -> TokenStream {
    let store = store_ident();
    let bound = Ident::new("__guiraffe_bound", Span::mixed_site());
    let id = site_id();
    let indices = (0..lenses.len()).map(syn::Index::from);
    let somes = values
        .iter()
        .map(|v| quote!(::core::option::Option::Some(#v)));
    let gets = indices.map(|i| quote!(#bound.#i.get()));
    quote!(::guiraffe_ui::__macro::bound_at(
        #id,
        #store,
        (#(#lenses,)*),
        move |#bound, #store, _, _| {
            let (#(#somes,)*) = (#(#gets,)*) else {
                return ::gpui_kit::IntoElement::into_any_element(::gpui_kit::Empty);
            };
            ::gpui_kit::IntoElement::into_any_element(#body)
        },
    ))
}

fn fragment(nodes: &[Node]) -> TokenStream {
    direct(nodes).map_or_else(
        || {
            let children = collect(nodes);
            quote!(::gpui_kit::ParentElement::children(::gpui_kit::div(), #children))
        },
        Node::expand,
    )
}

impl Root {
    pub(crate) fn expand(self) -> TokenStream {
        SCOPE.with(|scope| *scope.borrow_mut() = Scope::default());
        let tokens = match self.check() {
            Err(error) => error.into_compile_error(),
            Ok(()) => match direct(&self.0) {
                Some(node) => node.expand(),
                None => collect(&self.0),
            },
        };
        SCOPE.with(|scope| *scope.borrow_mut() = Scope::default());
        tokens
    }

    fn check(&self) -> Result<()> {
        self.0.iter().try_for_each(|node| node.check(false))
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
    /// Reject bindings outside a store scope and handlers outside their context.
    fn check(&self, store: bool) -> Result<()> {
        let lens = |lens: &LensPath, items: &[Ident], store: bool| {
            if store || items.iter().any(|item| *item == lens.root) {
                Ok(())
            } else {
                Err(Error::new(
                    lens.root.span(),
                    "a store binding needs an enclosing `store(…) { … }` scope",
                ))
            }
        };
        self.check_in(store, &mut Vec::new(), &lens)
    }

    fn check_in(
        &self,
        store: bool,
        items: &mut Vec<Ident>,
        lens: &dyn Fn(&LensPath, &[Ident], bool) -> Result<()>,
    ) -> Result<()> {
        let all = |nodes: &[Node], items: &mut Vec<Ident>| {
            nodes
                .iter()
                .try_for_each(|node| node.check_in(store, items, lens))
        };
        match self {
            Self::Element(element) => element.items.iter().try_for_each(|item| match item {
                Item::Bind { lens: path, .. } => lens(path, items, store),
                Item::Handle(name, Handler::Dispatch(_)) if !store => Err(Error::new_spanned(
                    name,
                    "`=> Event` needs an enclosing `store(…) { … }` scope",
                )),
                Item::Child(child) => child.check_in(store, items, lens),
                _ => Ok(()),
            }),
            Self::Store(_, nodes) => nodes
                .iter()
                .try_for_each(|node| node.check_in(true, items, lens)),
            Self::BoundText(path) => lens(path, items, store),
            Self::BoundIf(path, yes, no) => {
                lens(path, items, store)?;
                all(yes, items)?;
                no.as_deref().map_or(Ok(()), |no| all(no, items))
            }
            Self::BoundFor(_, item, path, _, nodes) => {
                lens(path, items, store)?;
                items.push(item.clone());
                let result = all(nodes, items);
                items.pop();
                result
            }
            Self::If(_, yes, no) => {
                all(yes, items)?;
                no.as_deref().map_or(Ok(()), |no| all(no, items))
            }
            Self::Match(_, arms) => arms
                .iter()
                .try_for_each(|(_, _, node)| node.check_in(store, items, lens)),
            Self::For(_, _, key, nodes) => {
                if key.is_none()
                    && let Some(span) = nodes.iter().find_map(Node::bound_span)
                {
                    return Err(Error::new(
                        span,
                        "a store binding in `for` requires `key <expr>`",
                    ));
                }
                all(nodes, items)
            }
            Self::Cached(_, _, nodes) => all(nodes, items),
            Self::Value(_) | Self::View(_) => Ok(()),
        }
    }

    /// The span of a bound site in this subtree, outside nested bound lists.
    fn bound_span(&self) -> Option<Span> {
        match self {
            Self::Element(element) => element.items.iter().find_map(|item| match item {
                Item::Bind { prop, .. } => Some(prop.span()),
                Item::Child(child) => child.bound_span(),
                _ => None,
            }),
            Self::BoundText(lens) | Self::BoundIf(lens, ..) | Self::BoundFor(_, _, lens, ..) => {
                Some(lens.root.span())
            }
            Self::If(_, yes, no) => yes
                .iter()
                .chain(no.iter().flatten())
                .find_map(Self::bound_span),
            Self::Match(_, arms) => arms.iter().find_map(|(_, _, node)| node.bound_span()),
            Self::Store(_, nodes) | Self::For(_, _, _, nodes) | Self::Cached(_, _, nodes) => {
                nodes.iter().find_map(Self::bound_span)
            }
            Self::Value(_) | Self::View(_) => None,
        }
    }

    fn cache_span(&self) -> Option<Span> {
        match self {
            Self::Cached(span, ..) => Some(*span),
            Self::Element(element) => element.items.iter().find_map(|item| match item {
                Item::Child(child) => child.cache_span(),
                _ => None,
            }),
            Self::If(_, yes, no) => yes
                .iter()
                .chain(no.iter().flatten())
                .find_map(Self::cache_span),
            Self::Match(_, arms) => arms.iter().find_map(|(_, _, node)| node.cache_span()),
            Self::For(_, _, _, nodes) | Self::Store(_, nodes) => {
                nodes.iter().find_map(Self::cache_span)
            }
            Self::BoundIf(_, yes, no) => yes
                .iter()
                .chain(no.iter().flatten())
                .find_map(Self::cache_span),
            Self::BoundFor(..) | Self::BoundText(_) | Self::Value(_) | Self::View(_) => None,
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
                    _ => false,
                });
                if explicit || (known && !element.args.is_empty()) {
                    return Some(&element.path);
                }
                element.items.iter().find_map(|item| match item {
                    Item::Child(child) => child.interactive(),
                    _ => None,
                })
            }
            Self::If(_, yes, no) => yes
                .iter()
                .chain(no.iter().flatten())
                .find_map(Self::interactive),
            Self::Cached(..) => None,
            Self::Match(_, arms) => arms.iter().find_map(|(_, _, node)| node.interactive()),
            Self::Store(_, nodes) => nodes.iter().find_map(Self::interactive),
            Self::For(..)
            | Self::Value(_)
            | Self::View(_)
            | Self::BoundText(_)
            | Self::BoundIf(..)
            | Self::BoundFor(..) => None,
        }
    }

    fn spreads(&self) -> bool {
        match self {
            Self::If(_, _, None) | Self::For(..) => true,
            Self::If(_, yes, Some(no)) => direct(yes).is_none() || direct(no).is_none(),
            Self::Match(_, arms) => arms.iter().any(|(_, _, node)| node.spreads()),
            Self::Store(_, nodes) => direct(nodes).is_none_or(Node::spreads),
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
                if let Some(tokens) = control_binding(&path, args, items) {
                    return tokens;
                }
                let bound = Ident::new("__guiraffe_bound", Span::mixed_site());
                let mut lenses = Vec::new();
                let mut values = Vec::new();
                let built = items.iter().fold(base, |result, item| match item {
                    Item::Builder(call) => quote!(#result #call),
                    Item::Child(node) => node.apply(result),
                    Item::Bind { prop, lens, handler } => {
                        let index = syn::Index::from(lenses.len());
                        let value = Ident::new(&format!("__guiraffe_value_{}", lenses.len()), Span::mixed_site());
                        lenses.push(lens.expand());
                        values.push(value.clone());
                        let read = quote_spanned!(prop.span()=> #result.#prop(::core::clone::Clone::clone(#value)));
                        match handler {
                            None => read,
                            Some(handler) => {
                                let store = store_ident();
                                quote_spanned!(handler.span()=> #read.#handler({
                                    let __guiraffe_weak = #store.downgrade();
                                    let __guiraffe_lens = #bound.#index.lens();
                                    move |__guiraffe_new, _, __guiraffe_cx| {
                                        if let ::core::option::Option::Some(__guiraffe_store) = __guiraffe_weak.upgrade() {
                                            __guiraffe_store.set_from_control(
                                                __guiraffe_lens,
                                                ::core::clone::Clone::clone(__guiraffe_new),
                                                __guiraffe_cx,
                                            );
                                        }
                                    }
                                }))
                            }
                        }
                    }
                    Item::Handle(name, Handler::Dispatch(event)) => {
                        let store = store_ident();
                        quote_spanned!(name.span()=> #result.#name({
                            let __guiraffe_weak = #store.downgrade();
                            move |_, _, __guiraffe_cx| {
                                if let ::core::option::Option::Some(__guiraffe_store) = __guiraffe_weak.upgrade() {
                                    __guiraffe_store.dispatch_from_control(#event, __guiraffe_cx);
                                }
                            }
                        }))
                    }
                    Item::Handle(name, Handler::Mutate(change)) => {
                        let state = Ident::new("__guiraffe_state", Span::call_site());
                        let change = mutation(change);
                        let fields = Ident::new("__guiraffe_fields", Span::mixed_site());
                        quote_spanned!(name.span()=> #result.#name({
                            let __guiraffe_handle = ::core::clone::Clone::clone(&#state);
                            move |_, __guiraffe_window, __guiraffe_cx| {
                                __guiraffe_handle.mutate(__guiraffe_window, __guiraffe_cx, |#fields| {
                                    #change;
                                });
                            }
                        }))
                    }
                });
                if lenses.is_empty() {
                    built
                } else {
                    bound_site(&lenses, &values, built)
                }
            }
            Self::Store(store, nodes) => {
                let name = store_ident();
                SCOPE.with(|scope| scope.borrow_mut().stores += 1);
                let body = match direct(nodes) {
                    Some(node) => node.expand(),
                    None => collect(nodes),
                };
                SCOPE.with(|scope| scope.borrow_mut().stores -= 1);
                quote!({
                    let #name = &(#store);
                    #body
                })
            }
            Self::BoundText(lens) => {
                let value = Ident::new("__guiraffe_value_0", Span::mixed_site());
                bound_site(
                    &[lens.expand()],
                    std::slice::from_ref(&value),
                    quote!(::guiraffe_ui::__macro::text(#value)),
                )
            }
            Self::BoundIf(lens, yes, no) => {
                let value = Ident::new("__guiraffe_value_0", Span::mixed_site());
                let yes = erase(fragment(yes));
                let no = no.as_deref().map_or_else(
                    || erase(quote!(::gpui_kit::Empty)),
                    |no| erase(fragment(no)),
                );
                bound_site(
                    &[lens.expand()],
                    std::slice::from_ref(&value),
                    quote!(if *#value { #yes } else { #no }),
                )
            }
            Self::BoundFor(key, item, lens, options, nodes) => {
                let store = store_ident();
                let lens = lens.expand();
                let item_lens = item_ident(item);
                SCOPE.with(|scope| scope.borrow_mut().items.push(item.clone()));
                let body = erase(fragment(nodes));
                let body = match key {
                    Some(key) => quote!({
                        let #key = *#item_lens.item_key();
                        #body
                    }),
                    None => body,
                };
                SCOPE.with(|scope| scope.borrow_mut().items.pop());
                let id = site_id();
                let mounted = options.mounted.then(|| quote!(.mounted()));
                let height = options
                    .row_height
                    .as_ref()
                    .map(|height| quote!(.row_height(#height)));
                quote!(::guiraffe_ui::__macro::bound_list(
                    #store,
                    #lens,
                    move |#item_lens, #item, #store, _, _| #body,
                ).id(#id) #mounted #height)
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
