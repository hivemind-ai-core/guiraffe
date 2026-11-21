//! Streaming updates: what to do with the next chunk of Markdown.
//!
//! Agent chat appends a token, or a few, at a time. Re-rendering the whole
//! document for each chunk is the obvious implementation and the wrong one:
//! the text view re-lays out every block after the changed one, and a
//! sentence's worth of tokens re-lays out a screen's worth of text.
//!
//! This module owns the decisions, purely:
//!
//! - whether the new text only appends to the old (the common case),
//!   replaces it wholesale, or rewrites it in the middle (rare: a tool result
//!   patching a summary);
//! - how much of the document is *settled* — a paragraph followed by a blank
//!   line, a closed fence, a complete table — so a fence that has finished
//!   streaming is not re-parsed on every further chunk of the reply;
//! - how long to wait before the next render, so a fast stream costs one
//!   layout per visible frame rather than per chunk.
//!
//! Ranges are byte offsets into the source.

use std::time::Duration;

/// Longest a chunk can go unrendered. A fast stream is coalesced up to this
/// delay; a slow one renders immediately.
pub const MAX_COALESCE: Duration = Duration::from_millis(66);

/// Shortest delay between renders while text is still arriving.
pub const MIN_COALESCE: Duration = Duration::from_millis(16);

/// What changed between two versions of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Nothing changed; no render is needed.
    None,
    /// `from` bytes are untouched and `appended` was added.
    Append { from: usize, appended: usize },
    /// Everything from `prefix` on was replaced (a rewrite in the middle), so
    /// the text view must parse a full document again.
    Replace { prefix: usize },
}

/// Classify an edit.
///
/// A prefix relationship is an append. A common prefix followed by different
/// tails is a replacement, and the returned `prefix` is where they diverge —
/// it tells the caller nothing before it needs re-parsing.
pub fn classify(previous: &str, next: &str) -> Change {
    if previous == next {
        return Change::None;
    }
    if let Some(rest) = next.strip_prefix(previous) {
        return Change::Append {
            from: previous.len(),
            appended: rest.len(),
        };
    }
    let prefix = common_prefix(previous, next);
    Change::Replace { prefix }
}

/// Length of the common byte prefix of two strings, on a char boundary.
pub fn common_prefix(a: &str, b: &str) -> usize {
    let mut index = 0;
    for (left, right) in a.bytes().zip(b.bytes()) {
        if left != right {
            break;
        }
        index += 1;
    }
    while index > 0 && (!a.is_char_boundary(index) || !b.is_char_boundary(index)) {
        index -= 1;
    }
    index
}

/// Length of the longest prefix of `source` that will not change as more text
/// streams in.
///
/// "Will not change" means: the block containing it is closed. A paragraph is
/// closed by a blank line, a fence by its closing run, a table by a blank line
/// after its rows, and a heading by the newline that ends it. Anything else is
/// still open, so a renderer may show it but must not cache it.
///
/// The heuristic is deliberately conservative: it may return less than the true
/// settled length (costing one extra re-parse), never more (which would show a
/// stale diagram).
pub fn settled_prefix(source: &str) -> usize {
    if source.is_empty() {
        return 0;
    }
    // A point past which the source is settled: the end of a closed block.
    let mut settled = 0;
    let mut fence: Option<(u8, usize)> = None; // (fence char, run length)
    let mut in_paragraph = false;

    for (index, line) in lines_with_offsets(source) {
        let after = index + line.len() + 1;
        // Only a line that has ended can settle anything: an unterminated last line may
        // still grow (`#` → `# Title`, a closing fence may gain another backtick), and the
        // empty segment after a final `\n` is not a blank line.
        if after > source.len() {
            break;
        }
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();

        if let Some((marker, run)) = fence {
            // Inside a fence: only a run at least as long as the opener closes it.
            if indent <= 3 && is_fence(trimmed, marker, run) {
                fence = None;
                settled = after;
            }
            continue;
        }

        if trimmed.is_empty() {
            // A blank line closes whatever came before it, and is itself settled.
            in_paragraph = false;
            settled = after;
            continue;
        }

        if in_paragraph {
            // The paragraph grows until a blank line ends it.
            continue;
        }

        if indent > 3 {
            // Continuation of a list item or quote: open until a blank line.
            in_paragraph = true;
            continue;
        }

        if let Some((marker, run)) = fence_opener(trimmed) {
            fence = Some((marker, run));
            continue;
        }

        if trimmed.starts_with('#') {
            // A heading is complete at its newline.
            settled = after;
            continue;
        }

        in_paragraph = true;
    }

    settled.min(source.len())
}

/// Iterator over lines with their byte offsets, keeping line endings off the
/// returned slice but counting them in the offsets.
fn lines_with_offsets(source: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut offset = 0;
    source.split('\n').map(move |line| {
        let start = offset;
        offset += line.len() + 1;
        (start, line)
    })
}

fn fence_opener(trimmed: &str) -> Option<(u8, usize)> {
    let marker = *trimmed.as_bytes().first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let run = trimmed.bytes().take_while(|b| *b == marker).count();
    (run >= 3).then_some((marker, run))
}

/// Whether `trimmed` is a closing fence for the given opener.
fn is_fence(trimmed: &str, marker: u8, run: usize) -> bool {
    let closing = trimmed.bytes().take_while(|b| *b == marker).count();
    closing >= run && trimmed[closing..].trim().is_empty()
}

/// Whether a Mermaid fence that is still open in `source` contains an
/// unfinished diagram.
///
/// The view uses this to avoid handing a half-streamed fence to the renderer:
/// a diagram re-laid out on every token is both expensive and flickery.
pub fn unfinished_mermaid_fence(source: &str) -> Option<usize> {
    // (fence char, run length, is mermaid, byte offset where the fence starts)
    let mut fence: Option<(u8, usize, bool, usize)> = None;
    for (index, line) in lines_with_offsets(source) {
        let terminated = index + line.len() < source.len();
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        match fence {
            // A closing run only counts once its line has ended: more characters on the
            // same line (three backticks then "x") would make it content, not a close.
            Some((marker, run, _, _))
                if terminated && indent <= 3 && is_fence(trimmed, marker, run) =>
            {
                fence = None
            }
            Some(_) => {}
            None => {
                if let Some((marker, run)) = fence_opener(trimmed) {
                    let lang = trimmed[run..].split_whitespace().next().unwrap_or_default();
                    fence = Some((marker, run, lang.eq_ignore_ascii_case("mermaid"), index));
                }
            }
        }
    }
    match fence {
        Some((_, _, true, start)) => Some(start),
        _ => None,
    }
}

/// Delay before rendering the next chunk, given how long the previous render
/// took.
///
/// A render that already takes a frame or more must not be followed by another
/// immediately: the stream coalesces onto the next frame instead. This keeps
/// CPU proportional to visible frames rather than to tokens.
pub fn coalesce_delay(last_render: Duration) -> Duration {
    last_render.max(MIN_COALESCE).min(MAX_COALESCE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_metadata_matches_one_shot_at_every_character_boundary() {
        use crate::document::Document;

        let source = concat!(
            "# Élan\n\n",
            "Setext heading\n==============\n\n",
            "- [ ] first task\n- [x] second task\n\n",
            "```mermaid\ngraph TD; A-->B\n```\n\n",
            "# Élan\nText and **markup**\n",
        );
        let expected = Document::analyze(source);
        for (split, _) in source.char_indices() {
            let first = &source[..split];
            let base = settled_prefix(first);
            let second = Document::merge_tail(
                &Document::analyze(first),
                base,
                Document::analyze(&source[base..]),
            );
            assert_eq!(
                base + settled_prefix(&source[base..]),
                settled_prefix(source)
            );
            assert_eq!(second, expected, "split at byte {split}");
        }
    }

    #[test]
    fn unfinished_mermaid_fence_reports_where_it_starts() {
        let open = "Intro\n\n```mermaid\ngraph TD; A-->";
        assert_eq!(unfinished_mermaid_fence(open), Some(7));
        assert_eq!(
            unfinished_mermaid_fence("Intro\n\n```mermaid\nA-->B\n```"),
            Some(7),
            "closing line not ended yet"
        );
        assert_eq!(unfinished_mermaid_fence("```mermaid\nA-->B\n```\n"), None);
        assert_eq!(
            unfinished_mermaid_fence("```rust\nfn x"),
            None,
            "only mermaid fences are held back"
        );
        assert_eq!(unfinished_mermaid_fence(""), None);
    }

    #[test]
    fn unterminated_lines_never_settle() {
        assert_eq!(settled_prefix("#"), 0, "may still become a heading");
        assert_eq!(settled_prefix("# Title"), 0, "the line has not ended");
        assert_eq!(settled_prefix("# Title\n"), 8);
        assert_eq!(
            settled_prefix("text\n"),
            0,
            "one newline is not a blank line"
        );
        assert_eq!(settled_prefix("text\n\n"), 6);
        assert_eq!(
            settled_prefix("```\ncode\n```"),
            0,
            "closing fence not ended yet"
        );
        assert_eq!(settled_prefix("```\ncode\n```\n"), 13);
    }

    #[test]
    fn identical_text_needs_no_render() {
        assert_eq!(classify("abc", "abc"), Change::None);
        assert_eq!(classify("", ""), Change::None);
    }

    #[test]
    fn appending_is_recognized_as_an_append() {
        assert_eq!(
            classify("Hello", "Hello, world"),
            Change::Append {
                from: 5,
                appended: 7
            }
        );
        assert_eq!(
            classify("", "# Title"),
            Change::Append {
                from: 0,
                appended: 7
            }
        );
    }

    #[test]
    fn a_mid_document_rewrite_is_a_replace_at_the_divergence() {
        assert_eq!(
            classify("one two three", "one 2 three"),
            Change::Replace { prefix: 4 }
        );
        assert_eq!(
            classify("alpha", "beta"),
            Change::Replace { prefix: 0 },
            "no shared prefix"
        );
    }

    #[test]
    fn common_prefix_lands_on_char_boundaries() {
        // 'é' is two bytes; a byte-wise comparison must not split it.
        assert_eq!(common_prefix("aé", "aè"), 1);
        assert_eq!(common_prefix("é", "è"), 0);
        assert_eq!(common_prefix("", "x"), 0);
        assert_eq!(common_prefix("same", "same"), 4);
    }

    #[test]
    fn a_finished_paragraph_is_settled() {
        let source = "First paragraph.\nStill first.\n\nSecond para";
        let settled = settled_prefix(source);
        assert_eq!(&source[settled..], "Second para");
    }

    #[test]
    fn a_closed_fence_is_settled_down_to_its_end() {
        let source = "text\n\n```mermaid\nflowchart LR\n  A-->B\n```\n\nstill streaming";
        let settled = settled_prefix(source);
        assert!(source[..settled].trim_end().ends_with("```"), "{settled}");
        assert_eq!(&source[settled..], "still streaming");
    }

    #[test]
    fn an_unclosed_fence_settles_nothing_inside_it() {
        let source = "intro\n\n```mermaid\nflowchart LR\n  A-->B";
        let settled = settled_prefix(source);
        assert_eq!(&source[settled..], "```mermaid\nflowchart LR\n  A-->B");
    }

    #[test]
    fn a_longer_closing_run_closes_a_fence() {
        let source = "````mermaid\nflowchart LR\n  A-->B\n````\ntail";
        assert_eq!(settled_prefix(source), source.find("tail").unwrap_or(0));
    }

    #[test]
    fn a_heading_is_complete_at_its_newline() {
        let source = "# Title\ntext still arriving";
        let settled = settled_prefix(source);
        assert_eq!(&source[settled..], "text still arriving");
    }

    #[test]
    fn a_table_settles_once_a_blank_line_follows_it() {
        let source = "| a | b |\n|---|---|\n| 1 | 2 |\n\nnext";
        let settled = settled_prefix(source);
        assert_eq!(&source[settled..], "next");
    }

    #[test]
    fn empty_and_whitespace_sources_settle_nothing_that_matters() {
        assert_eq!(settled_prefix(""), 0);
        // Blank lines are settled, but there is nothing after them.
        assert_eq!(settled_prefix("   \n\n"), 5);
        assert_eq!(settled_prefix("open"), 0);
    }

    #[test]
    fn settled_prefix_never_splits_a_character() {
        let source = "é\n\nnext";
        let settled = settled_prefix(source);
        assert!(source.is_char_boundary(settled));
    }

    #[test]
    fn an_unfinished_mermaid_fence_is_reported() {
        assert_eq!(
            unfinished_mermaid_fence("```mermaid\nflowchart LR\n  A-->B"),
            Some(0),
            "reports where the open fence starts (it used to return the source length)"
        );
        assert_eq!(
            unfinished_mermaid_fence("```mermaid\nflowchart LR\n```\nafter"),
            None
        );
        assert_eq!(unfinished_mermaid_fence("```rust\nfn f() {}"), None);
        assert_eq!(unfinished_mermaid_fence("plain text"), None);
    }

    #[test]
    fn only_an_open_mermaid_fence_counts_not_an_earlier_closed_one() {
        let source = "```mermaid\nA-->B\n```\n\ntext\n\n```mermaid\nC-->D";
        assert!(unfinished_mermaid_fence(source).is_some());
    }

    #[test]
    fn coalescing_bounds_the_render_rate() {
        assert_eq!(coalesce_delay(Duration::from_millis(1)), MIN_COALESCE);
        assert_eq!(
            coalesce_delay(Duration::from_millis(30)),
            Duration::from_millis(30)
        );
        assert_eq!(coalesce_delay(Duration::from_secs(5)), MAX_COALESCE);
    }
}
