//! One parse of a document, answering the questions a viewer asks of it:
//! which headings anchor the table of contents, where its Mermaid fences are,
//! and which task items can be toggled.
//!
//! Everything here is pure. The GPUI view parses on a background task with
//! the same parser the text view renders with, so ranges from this module
//! address the tree that is drawn, not a parallel reading of the source.

use std::ops::Range;

use markdown::{ParseOptions, mdast};

/// A stable 64-bit content hash (FNV-1a), used for element ids that must not
/// change between frames.
pub fn stable_hash(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The info string that marks a fence as a Mermaid diagram.
pub const MERMAID_LANG: &str = "mermaid";

/// Deepest heading the parser accepts.
pub const MAX_HEADING_DEPTH: u8 = 6;

/// One heading, as the table of contents shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 1..=6.
    pub depth: u8,
    /// The heading's text without inline markup.
    pub text: String,
    /// The anchor a link uses to jump to it, unique within the document.
    pub anchor: String,
    /// Source byte range of the whole heading, including its `#` markers.
    pub range: Range<usize>,
}

/// A Mermaid fence in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fence {
    /// Source byte range of the whole fence, including the backticks.
    pub range: Range<usize>,
    /// Diagram source, de-indented by the parser.
    pub source: String,
}

/// A task list item, with the range of its `[ ]` / `[x]` marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskItem {
    pub checked: bool,
    /// Range of the marker without the brackets, so replacing it is a one-byte
    /// edit.
    pub marker: Range<usize>,
    /// Range of the item's text (the content, without the marker).
    pub text: Range<usize>,
}

/// Everything the view needs to know beyond the rendered tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    pub headings: Vec<Heading>,
    pub fences: Vec<Fence>,
    pub tasks: Vec<TaskItem>,
}

impl Document {
    /// Parse `source` and collect headings, Mermaid fences and task items.
    ///
    /// A parse error cannot happen for Markdown (`markdown` reports errors for
    /// MDX constructs only), but is tolerated rather than propagated: the
    /// text view still renders what it can, so this returns an empty document
    /// rather than failing the whole view.
    pub fn analyze(source: &str) -> Self {
        let Ok(root) = markdown::to_mdast(source, &ParseOptions::gfm()) else {
            return Self::default();
        };
        let mut document = Self::default();
        let mut anchors = AnchorSlugs::default();
        walk(&root, source, &mut document, &mut anchors);
        document
    }

    /// Analyze `source` reusing `prev` (the analysis of an earlier prefix of it) for
    /// everything that ends before `base`, a block boundary both versions share
    /// (see [`crate::streaming::settled_prefix`]). Only `source[base..]` is parsed,
    /// so a streamed reply costs O(tail) per update instead of O(document).
    /// The result equals `Document::analyze(source)` (tested over many chunkings).
    pub fn analyze_incremental(prev: &Document, base: usize, source: &str) -> Self {
        let base = (0..=base.min(source.len()))
            .rev()
            .find(|b| source.is_char_boundary(*b))
            .unwrap_or(0);
        let mut out = Document {
            headings: prev
                .headings
                .iter()
                .filter(|h| h.range.end <= base)
                .cloned()
                .collect(),
            fences: prev
                .fences
                .iter()
                .filter(|f| f.range.end <= base)
                .cloned()
                .collect(),
            tasks: prev
                .tasks
                .iter()
                .filter(|t| t.text.end <= base && t.marker.end <= base)
                .cloned()
                .collect(),
        };
        let tail = Document::analyze(&source[base..]);
        let shift = |r: &Range<usize>| r.start + base..r.end + base;
        let mut anchors = AnchorSlugs {
            seen: out.headings.iter().map(|h| h.anchor.clone()).collect(),
        };
        out.headings
            .extend(tail.headings.into_iter().map(|h| Heading {
                anchor: anchors.unique(&slugify(&h.text)),
                range: shift(&h.range),
                ..h
            }));
        out.fences.extend(tail.fences.into_iter().map(|f| Fence {
            range: shift(&f.range),
            ..f
        }));
        out.tasks.extend(tail.tasks.into_iter().map(|t| TaskItem {
            marker: shift(&t.marker),
            text: shift(&t.text),
            ..t
        }));
        out
    }

    /// Whether the document has any heading.
    pub fn is_empty(&self) -> bool {
        self.headings.is_empty() && self.fences.is_empty() && self.tasks.is_empty()
    }
}

/// Walk the tree in document order, collecting what the view needs.
fn walk(node: &mdast::Node, source: &str, document: &mut Document, anchors: &mut AnchorSlugs) {
    match node {
        mdast::Node::Heading(heading) => {
            let text = plain_text(&heading.children);
            let range = node_range(heading.position.as_ref()).unwrap_or(0..0);
            document.headings.push(Heading {
                depth: heading.depth.clamp(1, MAX_HEADING_DEPTH),
                anchor: anchors.unique(&slugify(&text)),
                text,
                range,
            });
        }
        mdast::Node::Code(code) => {
            if code.lang.as_deref() == Some(MERMAID_LANG)
                && let Some(range) = node_range(code.position.as_ref())
            {
                document.fences.push(Fence {
                    range,
                    source: code.value.clone(),
                });
            }
        }
        mdast::Node::ListItem(item) => {
            if let Some(checked) = item.checked
                && let Some(position) = item.position.as_ref()
                && let Some(marker) =
                    marker_range(source, position.start.offset, position.end.offset)
            {
                document.tasks.push(TaskItem {
                    checked,
                    marker: marker.clone(),
                    text: marker.end.saturating_add(1)..position.end.offset,
                });
            }
        }
        _ => {}
    }
    for child in node.children().map(Vec::as_slice).unwrap_or_default() {
        walk(child, source, document, anchors);
    }
}

fn node_range(position: Option<&markdown::unist::Position>) -> Option<Range<usize>> {
    let position = position?;
    let start = position.start.offset;
    let end = position.end.offset;
    (start <= end).then_some(start..end)
}

/// Range of the single character inside the brackets of a task marker.
///
/// A GFM task item starts with `- [ ]` or `- [x]`; the parser reports the
/// item's range but not the marker's, so it is located by scanning the item's
/// first line. Anything unexpected yields `None` rather than a guessed range,
/// because a wrong range would let a toggle corrupt the source.
fn marker_range(source: &str, start: usize, end: usize) -> Option<Range<usize>> {
    let item = source.get(start..end)?;
    let line_end = item.find('\n').unwrap_or(item.len());
    let line = item.get(..line_end)?;
    let open = line.find('[')?;
    let close = line.get(open..)?.find(']')? + open;
    // Exactly one character between the brackets, and only a bullet or number
    // before them.
    if close.checked_sub(open)? != 2 {
        return None;
    }
    // Only a bullet (`-`, `*`, `+`) or an ordered marker (`1.`, `1)`) may
    // precede the brackets.
    let prefix = line.get(..open)?.trim();
    let looks_like_marker = matches!(prefix, "-" | "*" | "+")
        || prefix
            .strip_suffix('.')
            .or_else(|| prefix.strip_suffix(')'))
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()));
    if !looks_like_marker {
        return None;
    }
    Some(start + open + 1..start + open + 2)
}

/// The text of an inline subtree with the markup removed.
fn plain_text(nodes: &[mdast::Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            mdast::Node::Text(text) => out.push_str(&text.value),
            mdast::Node::InlineCode(code) => out.push_str(&code.value),
            mdast::Node::Code(code) => out.push_str(&code.value),
            mdast::Node::Image(image) => out.push_str(&image.alt),
            _ => {
                if let Some(children) = node.children() {
                    out.push_str(&plain_text(children));
                }
            }
        }
    }
    out.trim().to_string()
}

/// GitHub-style heading anchor: lowercase, spaces to dashes, punctuation
/// dropped.
pub fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    let mut last_dash = false;
    for c in text.chars() {
        // GitHub keeps underscores and hyphens in anchors and turns any run of
        // other punctuation or whitespace into one dash.
        if c == '_' || c == '-' {
            slug.push(c);
            last_dash = c == '-';
        } else if c.is_alphanumeric() {
            for lowered in c.to_lowercase() {
                slug.push(lowered);
            }
            last_dash = false;
        } else if c.is_whitespace() && !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "section".to_string()
    } else {
        slug
    }
}

/// Hands out anchors that are unique even when two headings have the same
/// text, by suffixing `-1`, `-2`, … as GitHub does.
#[derive(Default)]
struct AnchorSlugs {
    seen: Vec<String>,
}

impl AnchorSlugs {
    fn unique(&mut self, slug: &str) -> String {
        let mut candidate = slug.to_string();
        let mut index = 0;
        while self.seen.contains(&candidate) {
            index += 1;
            candidate = format!("{slug}-{index}");
        }
        self.seen.push(candidate.clone());
        candidate
    }
}

/// Apply a task toggle to a source, returning the new source.
///
/// `marker` is the range a [`TaskItem`] reports or a `TaskToggleEvent` carries:
/// the single character inside the brackets. Anything that is not exactly one
/// character at a char boundary is left alone, so a stale range from an edit
/// between the click and this call cannot corrupt the document.
pub fn apply_task_toggle(source: &str, marker: Range<usize>, checked: bool) -> String {
    let mut result = source.to_string();
    let valid = marker.end == marker.start + 1
        && source.is_char_boundary(marker.start)
        && source.is_char_boundary(marker.end)
        && matches!(source.get(marker.clone()), Some(" " | "x" | "X"));
    if valid {
        result.replace_range(marker, if checked { "x" } else { " " });
    }
    result
}

/// Table of contents entries with their nesting depth, for a widget that draws
/// an outline.
pub fn table_of_contents(headings: &[Heading]) -> Vec<(usize, &Heading)> {
    headings
        .iter()
        .map(|heading| (usize::from(heading.depth.saturating_sub(1)), heading))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "\
# Intro

Some text with `code`.

## Install

- [ ] first task
- [x] second task

```mermaid
flowchart LR
  A --> B
```

### Deeper

```rust
fn main() {}
```

## Install

Last line.
";

    #[test]
    fn headings_keep_order_depth_and_ranges() {
        let document = Document::analyze(DOC);
        let titles: Vec<_> = document
            .headings
            .iter()
            .map(|h| (h.depth, h.text.as_str()))
            .collect();
        assert_eq!(
            titles,
            [(1, "Intro"), (2, "Install"), (3, "Deeper"), (2, "Install")]
        );
        let first = &document.headings[0];
        assert_eq!(&DOC[first.range.clone()], "# Intro");
        assert_eq!(&DOC[document.headings[2].range.clone()], "### Deeper");
    }

    #[test]
    fn incremental_analysis_equals_full_analysis_for_every_chunking() {
        let doc = "# Intro\n\nSome *text* here.\n\n## Install\n\n```rust\nfn main() {}\n```\n\n- [ ] todo one\n- [x] done two\n\n## Install\n\nÜnïcode paragraph 🦒.\n\n```mermaid\ngraph TD; A-->B\n```\n\n### Deeper\ntext\n";
        for chunk in [1usize, 2, 3, 5, 7, 13, 64] {
            let mut prev = Document::default();
            let mut settled = 0;
            let mut end = 0;
            while end < doc.len() {
                end = (end + chunk).min(doc.len());
                while !doc.is_char_boundary(end) {
                    end += 1;
                }
                let src = &doc[..end];
                let next = Document::analyze_incremental(&prev, settled, src);
                assert_eq!(next, Document::analyze(src), "chunk {chunk}, prefix {end}");
                settled = crate::streaming::settled_prefix(src);
                prev = next;
            }
        }
    }

    #[test]
    fn duplicate_headings_get_unique_anchors() {
        let document = Document::analyze(DOC);
        let anchors: Vec<_> = document
            .headings
            .iter()
            .map(|h| h.anchor.as_str())
            .collect();
        assert_eq!(anchors, ["intro", "install", "deeper", "install-1"]);
    }

    #[test]
    fn only_mermaid_fences_are_diagrams() {
        let document = Document::analyze(DOC);
        assert_eq!(document.fences.len(), 1);
        let fence = &document.fences[0];
        assert_eq!(fence.source, "flowchart LR\n  A --> B");
        assert!(DOC[fence.range.clone()].starts_with("```mermaid"));
        assert!(DOC[fence.range.clone()].ends_with("```"));
    }

    #[test]
    fn task_markers_address_the_character_inside_the_brackets() {
        let document = Document::analyze(DOC);
        assert_eq!(document.tasks.len(), 2);
        let [first, second] = &document.tasks[..] else {
            panic!("two tasks");
        };
        assert!(!first.checked);
        assert!(second.checked);
        assert_eq!(&DOC[first.marker.clone()], " ");
        assert_eq!(&DOC[second.marker.clone()], "x");
        // Toggling replaces exactly that byte, leaving the source valid.
        let mut toggled = DOC.to_string();
        toggled.replace_range(first.marker.clone(), "x");
        assert!(toggled.contains("- [x] first task"));
    }

    #[test]
    fn a_marker_like_text_elsewhere_is_not_a_task() {
        // An inline code span containing brackets, and a link's text.
        let source = "- not a task [ ] here\n- `[ ]`\n";
        let document = Document::analyze(source);
        assert!(document.tasks.is_empty(), "{:?}", document.tasks);
    }

    #[test]
    fn toggling_rewrites_only_the_marker() {
        let document = Document::analyze(DOC);
        let [first, second] = &document.tasks[..] else {
            panic!("two tasks");
        };
        let checked_source = apply_task_toggle(DOC, first.marker.clone(), true);
        assert!(checked_source.contains("- [x] first task"));
        assert_eq!(checked_source.len(), DOC.len());
        let unchecked_source = apply_task_toggle(&checked_source, second.marker.clone(), false);
        assert!(unchecked_source.contains("- [ ] second task"));
        // Round trip: toggling both back reproduces the original exactly.
        let back = apply_task_toggle(&unchecked_source, second.marker.clone(), true);
        let back = apply_task_toggle(&back, first.marker.clone(), false);
        assert_eq!(back, DOC);
    }

    #[test]
    fn a_stale_or_wrong_toggle_range_leaves_the_source_alone() {
        let document = Document::analyze(DOC);
        let marker = document.tasks[0].marker.clone();
        // Correct range, but the character is no longer a marker.
        let edited = DOC.replacen("- [ ]", "- [y]", 1);
        assert_eq!(apply_task_toggle(&edited, marker.clone(), true), edited);
        // A range that is not exactly one character.
        assert_eq!(apply_task_toggle(DOC, 0..2, true), DOC);
        // A range past the end.
        let past = DOC.len() + 10;
        assert_eq!(apply_task_toggle(DOC, past..past + 1, true), DOC);
    }

    #[test]
    fn ordered_task_items_are_found() {
        let document = Document::analyze("1. [x] done\n2. [ ] todo\n");
        assert_eq!(document.tasks.len(), 2);
        assert!(document.tasks[0].checked);
        assert_eq!(&"-1. [x] done"[0..0], "");
    }

    #[test]
    fn headings_with_markup_are_plain_text() {
        let document = Document::analyze("## Use `Vec<T>` **now**\n");
        assert_eq!(document.headings[0].text, "Use Vec<T> now");
        assert_eq!(document.headings[0].anchor, "use-vect-now");
    }

    #[test]
    fn slugify_matches_github_rules() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  spaced  out  "), "spaced-out");
        assert_eq!(slugify("snake_case_and-kebab"), "snake_case_and-kebab");
        assert_eq!(slugify("Café ☕"), "café");
        assert_eq!(slugify("!!!"), "section");
        assert_eq!(slugify(""), "section");
    }

    #[test]
    fn empty_and_plain_documents_have_nothing_to_show() {
        assert!(Document::analyze("").is_empty());
        assert!(Document::analyze("just a paragraph\n").is_empty());
    }

    #[test]
    fn table_of_contents_indents_by_heading_depth() {
        let document = Document::analyze(DOC);
        let toc: Vec<_> = table_of_contents(&document.headings)
            .into_iter()
            .map(|(indent, heading)| (indent, heading.text.as_str()))
            .collect();
        assert_eq!(
            toc,
            [(0, "Intro"), (1, "Install"), (2, "Deeper"), (1, "Install")]
        );
    }

    #[test]
    fn a_nested_fence_inside_a_list_still_yields_a_diagram() {
        let source = "1. step\n\n   ```mermaid\n   flowchart LR\n     A --> B\n   ```\n";
        let document = Document::analyze(source);
        assert_eq!(document.fences.len(), 1);
        assert!(document.fences[0].source.starts_with("flowchart LR"));
    }
}
