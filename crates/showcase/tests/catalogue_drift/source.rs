use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use syn::{
    GenericArgument, ImplItem, Item, PathArguments, ReturnType, TraitItem, Type, TypeParamBound,
    UseTree, Visibility,
};

const TOOL_CRATES: &[&str] = &["bench", "cli", "gallery", "mocket", "showcase", "test"];

pub fn public_surface(crates: &Path) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    let kit_components = crates.join("../vendor/gpui-component/src");
    let mut kit_modules = BTreeMap::new();
    for dir in fs::read_dir(crates).expect("workspace crates") {
        let dir = dir.expect("crate directory entry");
        let name = dir.file_name();
        let name = name.to_str().expect("UTF-8 crate directory");
        let src = dir.path().join("src");
        if TOOL_CRATES.contains(&name) || !src.is_dir() {
            continue;
        }
        let mut inventory = Inventory::default();
        inventory.scan_files(&src);
        result.extend(inventory.surface(name));
        for (module, source, exported) in &inventory.kit_imports {
            let kit = kit_modules.entry(module.clone()).or_insert_with(|| {
                let mut kit = Inventory::default();
                let path = kit_components.join(module);
                if path.is_dir() {
                    kit.scan_files(&path);
                } else {
                    let file = path.with_extension("rs");
                    if file.is_file() {
                        kit.scan_file(&file);
                    }
                }
                kit
            });
            if kit.renders.contains(source)
                || matches!(
                    source.as_str(),
                    "ContextMenu" | "ContextMenuExt" | "DropdownMenu"
                )
            {
                result.insert(format!("{name}::{exported}"));
                result.extend(
                    kit.methods
                        .iter()
                        .filter(|(owner, _)| owner == source)
                        .map(|(_, method)| format!("{name}::{exported}::{method}")),
                );
            }
        }
    }
    result
}

fn rust_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).expect("source directory") {
        let path = entry.expect("source entry").path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| matches!(name, "tests" | "tests.rs") || name.ends_with("_tests.rs"))
        {
            continue;
        }
        if path.is_dir() {
            files.extend(rust_files(&path));
        } else if path.extension().is_some_and(|suffix| suffix == "rs") {
            files.push(path);
        }
    }
    files
}

#[derive(Default)]
struct Inventory {
    public_types: BTreeSet<String>,
    renders: BTreeSet<String>,
    methods: BTreeSet<(String, String)>,
    aliases: Vec<(String, Type)>,
    fields: BTreeSet<(String, String)>,
    factories: Vec<(String, Type)>,
    event_types: BTreeMap<String, Vec<String>>,
    emitters: BTreeSet<(String, String)>,
    kit_imports: BTreeSet<(PathBuf, String, String)>,
}

impl Inventory {
    fn scan_files(&mut self, dir: &Path) {
        for source in rust_files(dir) {
            self.scan_file(&source);
        }
    }

    fn scan_file(&mut self, source: &Path) {
        let text = fs::read_to_string(source).expect("source file");
        let syntax =
            syn::parse_file(&text).unwrap_or_else(|error| panic!("{}: {error}", source.display()));
        self.scan(&syntax.items);
    }

    fn surface(&self, krate: &str) -> BTreeSet<String> {
        let render_types: BTreeSet<_> = self
            .public_types
            .intersection(&self.renders)
            .cloned()
            .chain(
                self.public_types
                    .iter()
                    .filter(|name| {
                        matches!(
                            (krate, name.as_str()),
                            ("layout", "AppShellState") | ("overlays", "Drawer" | "Modal")
                        )
                    })
                    .cloned(),
            )
            .collect();
        let mut result = BTreeSet::new();
        for ty in &render_types {
            result.insert(format!("{krate}::{ty}"));
            result.extend(
                self.methods
                    .iter()
                    .filter(|(owner, _)| owner == ty)
                    .map(|(_, method)| format!("{krate}::{ty}::{method}")),
            );
        }
        for (owner, field) in &self.fields {
            result.insert(format!("{krate}::{owner}::field {field}"));
        }
        for (name, ty) in &self.aliases {
            if returns_widget(ty, &render_types) {
                result.insert(format!("{krate}::{name}"));
            }
        }
        for (name, ty) in &self.factories {
            if returns_widget(ty, &render_types) {
                result.insert(format!("{krate}::fn {name}"));
            }
        }
        for (owner, event) in &self.emitters {
            if !self.public_types.contains(owner) {
                continue;
            }
            if let Some(variants) = self.event_types.get(event) {
                result.extend(
                    variants
                        .iter()
                        .map(|variant| format!("{krate}::{owner}::event {variant}")),
                );
            } else if self.public_types.contains(event) {
                result.insert(format!("{krate}::{owner}::event {event}"));
            }
        }
        result
    }

    fn scan(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Enum(item) if matches!(item.vis, Visibility::Public(_)) => {
                    self.public_types.insert(item.ident.to_string());
                    self.event_types.insert(
                        item.ident.to_string(),
                        item.variants
                            .iter()
                            .map(|variant| variant.ident.to_string())
                            .collect(),
                    );
                }
                Item::Struct(item) if matches!(item.vis, Visibility::Public(_)) => {
                    let name = item.ident.to_string();
                    if name.ends_with("Options")
                        || name.ends_with("Props")
                        || matches!(name.as_str(), "ChartSpec" | "RowStyle")
                    {
                        self.fields.extend(item.fields.iter().filter_map(|field| {
                            if !matches!(field.vis, Visibility::Public(_)) {
                                return None;
                            }
                            Some((name.clone(), field.ident.as_ref()?.to_string()))
                        }));
                    }
                    self.public_types.insert(name);
                }
                Item::Mod(item)
                    if !item.attrs.iter().any(|attr| {
                        attr.path().is_ident("cfg")
                            && attr
                                .parse_args::<syn::Ident>()
                                .is_ok_and(|cfg| cfg == "test")
                    }) =>
                {
                    if let Some((_, items)) = &item.content {
                        self.scan(items);
                    }
                }
                Item::Use(item) if matches!(item.vis, Visibility::Public(_)) => {
                    if let UseTree::Path(kit) = &item.tree
                        && kit.ident == "gpui_kit"
                        && let UseTree::Path(component) = kit.tree.as_ref()
                        && component.ident == "component"
                    {
                        collect_imports(&component.tree, Path::new(""), &mut self.kit_imports);
                    }
                }
                Item::Trait(item) if matches!(item.vis, Visibility::Public(_)) => {
                    let owner = item.ident.to_string();
                    self.methods.extend(item.items.iter().filter_map(|item| {
                        let TraitItem::Fn(method) = item else {
                            return None;
                        };
                        Some((owner.clone(), method.sig.ident.to_string()))
                    }));
                }
                Item::Type(item) if matches!(item.vis, Visibility::Public(_)) => {
                    self.aliases
                        .push((item.ident.to_string(), item.ty.as_ref().clone()));
                }
                Item::Fn(item) if matches!(item.vis, Visibility::Public(_)) => {
                    if let ReturnType::Type(_, ty) = &item.sig.output {
                        self.factories
                            .push((item.sig.ident.to_string(), ty.as_ref().clone()));
                    }
                }
                Item::Impl(item) => {
                    let Type::Path(path) = item.self_ty.as_ref() else {
                        continue;
                    };
                    let Some(owner) = path.path.segments.last() else {
                        continue;
                    };
                    let owner = owner.ident.to_string();
                    if let Some((_, trait_path, _)) = &item.trait_ {
                        if let Some(name) = trait_path.segments.last() {
                            if name.ident == "Render" || name.ident == "RenderOnce" {
                                self.renders.insert(owner);
                            } else if name.ident == "EventEmitter"
                                && let PathArguments::AngleBracketed(args) = &name.arguments
                                && let Some(GenericArgument::Type(Type::Path(path))) =
                                    args.args.first()
                                && let Some(event) = path.path.segments.last()
                            {
                                self.emitters.insert((owner, event.ident.to_string()));
                            }
                        }
                        continue;
                    }
                    for method in &item.items {
                        if let ImplItem::Fn(method) = method
                            && matches!(method.vis, Visibility::Public(_))
                        {
                            self.methods
                                .insert((owner.clone(), method.sig.ident.to_string()));
                        }
                    }
                }
                Item::Macro(item)
                    if item
                        .mac
                        .path
                        .segments
                        .last()
                        .is_some_and(|name| name.ident == "props") =>
                {
                    // GUIraffe's props! DSL is Type { setter: ValueType => field, ... }.
                    let mut tokens = item.mac.tokens.clone().into_iter();
                    if let (
                        Some(proc_macro2::TokenTree::Ident(owner)),
                        Some(proc_macro2::TokenTree::Group(props)),
                    ) = (tokens.next(), tokens.next())
                    {
                        let tokens: Vec<_> = props.stream().into_iter().collect();
                        for pair in tokens.windows(2) {
                            if let [
                                proc_macro2::TokenTree::Ident(name),
                                proc_macro2::TokenTree::Punct(colon),
                            ] = pair
                                && colon.as_char() == ':'
                                && colon.spacing() == proc_macro2::Spacing::Alone
                            {
                                self.methods.insert((owner.to_string(), name.to_string()));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn collect_imports(
    tree: &UseTree,
    module: &Path,
    imports: &mut BTreeSet<(PathBuf, String, String)>,
) {
    match tree {
        UseTree::Path(path) => {
            collect_imports(&path.tree, &module.join(path.ident.to_string()), imports)
        }
        UseTree::Group(group) => {
            for child in &group.items {
                collect_imports(child, module, imports);
            }
        }
        UseTree::Name(name) => {
            let name = name.ident.to_string();
            imports.insert((module.to_owned(), name.clone(), name));
        }
        UseTree::Rename(rename) => {
            imports.insert((
                module.to_owned(),
                rename.ident.to_string(),
                rename.rename.to_string(),
            ));
        }
        UseTree::Glob(_) => {}
    }
}

fn returns_widget(ty: &Type, render_types: &BTreeSet<String>) -> bool {
    match ty {
        Type::Path(path) => {
            let Some(segment) = path.path.segments.last() else {
                return false;
            };
            let name = segment.ident.to_string();
            if render_types.contains(&name)
                || matches!(
                    name.as_str(),
                    "AnyElement" | "Button" | "DatePicker" | "DockArea" | "TimeField"
                )
            {
                return true;
            }
            matches!(name.as_str(), "Entity" | "Option" | "Result")
                && matches!(&segment.arguments, PathArguments::AngleBracketed(args)
                    if args.args.first().is_some_and(|argument| matches!(argument,
                        GenericArgument::Type(ty) if returns_widget(ty, render_types))))
        }
        Type::Tuple(tuple) => tuple
            .elems
            .iter()
            .any(|ty| returns_widget(ty, render_types)),
        Type::ImplTrait(implementation) => implementation.bounds.iter().any(|bound| {
            matches!(bound, TypeParamBound::Trait(bound)
                if bound.path.segments.last().is_some_and(|name| matches!(
                    name.ident.to_string().as_str(), "IntoElement" | "Render" | "RenderOnce")))
        }),
        _ => false,
    }
}

#[test]
fn tracks_new_rust_types_aliases_factories_and_builder_methods() {
    let file = syn::parse_file(
        r#"
        pub struct ButtonView;
        impl Render for ButtonView {}
        impl ButtonView {
            pub fn new_prop(self) -> Self { self }
        }
        props!(ButtonView { macro_prop: Size => size });
        crate::props!(ButtonView { qualified_prop: bool => flag });
        pub enum ButtonEvent { Clicked, Hovered(bool) }
        impl EventEmitter<ButtonEvent> for ButtonView {}
        pub enum StatusView { Fresh, Stale }
        impl RenderOnce for StatusView {}
        pub type Alias = ButtonView;
        pub struct ViewOptions { pub newer: bool, internal: bool }
        pub fn factory() -> (Entity<DockArea>, Entity<ButtonView>) { todo!() }
        #[cfg(test)]
        mod tests {
            pub struct PrivateTest;
            impl Render for PrivateTest {}
        }
    "#,
    )
    .expect("source fixture");
    let mut inventory = Inventory::default();
    inventory.scan(&file.items);
    assert_eq!(
        inventory.surface("fixture"),
        BTreeSet::from([
            "fixture::Alias".into(),
            "fixture::ButtonView".into(),
            "fixture::ButtonView::event Clicked".into(),
            "fixture::ButtonView::event Hovered".into(),
            "fixture::ButtonView::macro_prop".into(),
            "fixture::ButtonView::new_prop".into(),
            "fixture::ButtonView::qualified_prop".into(),
            "fixture::StatusView".into(),
            "fixture::fn factory".into(),
            "fixture::ViewOptions::field newer".into(),
        ])
    );
}
