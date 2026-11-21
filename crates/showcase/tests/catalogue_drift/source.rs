use std::{collections::BTreeSet, fs, path::Path};

use syn::{ImplItem, Item, Type, Visibility};

const TOOL_CRATES: &[&str] = &["bench", "cli", "gallery", "mocket", "showcase", "test"];

pub fn public_surface(crates: &Path) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    for dir in fs::read_dir(crates).expect("workspace crates") {
        let dir = dir.expect("crate directory entry");
        let name = dir.file_name();
        let name = name.to_str().expect("UTF-8 crate directory");
        let src = dir.path().join("src");
        if TOOL_CRATES.contains(&name) || !src.is_dir() {
            continue;
        }
        let mut structs = BTreeSet::new();
        let mut renders = BTreeSet::new();
        let mut methods = BTreeSet::new();
        for source in rust_files(&src) {
            let text = fs::read_to_string(&source).expect("source file");
            let syntax = syn::parse_file(&text)
                .unwrap_or_else(|error| panic!("{}: {error}", source.display()));
            scan(&syntax.items, &mut structs, &mut renders, &mut methods);
        }
        for ty in structs.intersection(&renders) {
            result.insert(format!("{name}::{ty}"));
            result.extend(
                methods
                    .iter()
                    .filter(|(owner, _)| owner == ty)
                    .map(|(_, method)| format!("{name}::{ty}::{method}")),
            );
        }
    }
    result
}

fn rust_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).expect("source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            files.extend(rust_files(&path));
        } else if path.extension().is_some_and(|suffix| suffix == "rs") {
            files.push(path);
        }
    }
    files
}

fn scan(
    items: &[Item],
    structs: &mut BTreeSet<String>,
    renders: &mut BTreeSet<String>,
    methods: &mut BTreeSet<(String, String)>,
) {
    for item in items {
        match item {
            Item::Struct(item) if matches!(item.vis, Visibility::Public(_)) => {
                structs.insert(item.ident.to_string());
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
                    scan(items, structs, renders, methods);
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
                    if let Some(name) = trait_path.segments.last()
                        && (name.ident == "Render" || name.ident == "RenderOnce")
                    {
                        renders.insert(owner);
                    }
                    continue;
                }
                for method in &item.items {
                    if let ImplItem::Fn(method) = method
                        && matches!(method.vis, Visibility::Public(_))
                    {
                        methods.insert((owner.clone(), method.sig.ident.to_string()));
                    }
                }
            }
            Item::Macro(item) if item.mac.path.is_ident("props") => {
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
                            methods.insert((owner.to_string(), name.to_string()));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
