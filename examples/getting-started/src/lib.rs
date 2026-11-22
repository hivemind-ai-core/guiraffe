//! The examples live in `examples/`; `docs/getting-started.md` walks through them.
//!
//! The test below keeps the guide honest: every `rust` code block in the two newcomer docs
//! must appear, line for line, in one of the example files.

#[cfg(test)]
mod tests {
    use std::path::Path;

    const EXAMPLES: [&str; 6] = [
        include_str!("../examples/01_hello.rs"),
        include_str!("../examples/02_counter.rs"),
        include_str!("../examples/03_form.rs"),
        include_str!("../examples/04_datagrid.rs"),
        include_str!("../examples/05_theme.rs"),
        include_str!("../examples/06_router_store.rs"),
    ];

    /// The ```rust blocks of a Markdown document.
    fn rust_blocks(markdown: &str) -> Vec<Vec<&str>> {
        let mut blocks = Vec::new();
        let mut current: Option<Vec<&str>> = None;
        for line in markdown.lines() {
            match (&mut current, line.trim()) {
                (None, "```rust") => current = Some(Vec::new()),
                (Some(_), "```") => blocks.extend(current.take()),
                (Some(block), _) => block.push(line),
                (None, _) => {}
            }
        }
        blocks
    }

    /// Whether `block` occurs as consecutive lines of `file`, ignoring indentation.
    fn contains(file: &str, block: &[&str]) -> bool {
        let lines: Vec<&str> = file.lines().map(str::trim).collect();
        let block: Vec<&str> = block.iter().map(|l| l.trim()).collect();
        !block.is_empty() && lines.windows(block.len()).any(|w| w == block.as_slice())
    }

    #[test]
    fn doc_snippets_match_the_examples() {
        let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
        for doc in ["getting-started.md", "guides/from-react-electron.md"] {
            let text = std::fs::read_to_string(docs.join(doc))
                .unwrap_or_else(|e| panic!("reading {doc}: {e}"));
            let blocks = rust_blocks(&text);
            assert!(!blocks.is_empty(), "{doc} has no rust blocks");
            for block in blocks {
                assert!(
                    EXAMPLES.iter().any(|file| contains(file, &block)),
                    "{doc}: this snippet is not in any example file:\n{}",
                    block.join("\n")
                );
            }
        }
    }
}
