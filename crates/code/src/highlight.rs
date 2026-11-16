//! Controls' Tree-sitter parser runs on a worker, publishing one immutable snapshot.
use crate::CodeDocument;
use gpui_kit::{
    HighlightStyle,
    component::highlighter::{HighlightTheme, SyntaxHighlighter},
};
use std::{ops::Range, sync::Arc, time::Duration};

#[derive(Debug, Clone)]
pub struct Highlighted {
    pub document: Arc<CodeDocument>,
    pub spans: Arc<[(Range<usize>, HighlightStyle)]>,
    pub error: Option<Arc<str>>,
    alternate: Arc<[(Range<usize>, HighlightStyle)]>,
    dark: bool,
}
impl Highlighted {
    pub fn plain(document: Arc<CodeDocument>) -> Self {
        Self {
            document,
            spans: Arc::from([]),
            error: None,
            alternate: Arc::from([]),
            dark: false,
        }
    }
    /// Call on a background executor. Unknown languages retain plain text; parser timeout or
    /// panic is reported without discarding source. Rust, JSON and JavaScript ship by default;
    /// hosts can register further Controls grammars.
    pub fn compute(document: Arc<CodeDocument>, language: &str, dark: bool) -> Self {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut parser = SyntaxHighlighter::new(language);
            let rope = ropey::Rope::from_str(document.source());
            if !parser.update(None, &rope, Some(Duration::from_secs(2))) {
                return Err("Syntax analysis exceeded its time budget");
            }
            let theme = if dark {
                HighlightTheme::default_dark()
            } else {
                HighlightTheme::default_light()
            };
            let alternate = if dark {
                HighlightTheme::default_light()
            } else {
                HighlightTheme::default_dark()
            };
            let clean = |spans: Vec<(Range<usize>, HighlightStyle)>| {
                spans
                    .into_iter()
                    .filter(|(_, s)| s != &HighlightStyle::default())
                    .collect::<Vec<_>>()
            };
            Ok((
                clean(parser.styles(&(0..document.source().len()), theme.as_ref())),
                clean(parser.styles(&(0..document.source().len()), alternate.as_ref())),
            ))
        }));
        match result {
            Ok(Ok((spans, alternate))) => Self {
                document,
                spans: spans.into(),
                alternate: alternate.into(),
                dark,
                error: None,
            },
            Ok(Err(error)) => Self {
                error: Some(error.into()),
                ..Self::plain(document)
            },
            Err(_) => Self {
                error: Some("Syntax analysis failed; showing plain source".into()),
                ..Self::plain(document)
            },
        }
    }
    pub fn styles_for(
        &self,
        range: Range<usize>,
    ) -> impl Iterator<Item = &(Range<usize>, HighlightStyle)> {
        self.styles_for_scheme(range, self.dark)
    }
    pub fn styles_for_scheme(
        &self,
        range: Range<usize>,
        dark: bool,
    ) -> impl Iterator<Item = &(Range<usize>, HighlightStyle)> {
        let spans = if dark == self.dark {
            &self.spans
        } else {
            &self.alternate
        };
        let first = spans.partition_point(|(r, _)| r.end <= range.start);
        spans
            .get(first..)
            .unwrap_or_default()
            .iter()
            .take_while(move |(r, _)| r.start < range.end)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rust_highlighting_and_unknown_language_fallback_are_thread_safe() {
        let doc = Arc::new(CodeDocument::new(Arc::<str>::from(
            "fn main() { let value = 42; }",
        )));
        let cloned = doc.clone();
        let result = std::thread::spawn(move || Highlighted::compute(cloned, "rust", false))
            .join()
            .unwrap_or_else(|_| panic!("worker panic"));
        assert!(Arc::ptr_eq(&doc, &result.document));
        assert!(result.error.is_none());
        assert!(result.spans.iter().any(|(_, s)| s.color.is_some()));
        let plain = Highlighted::compute(doc, "unregistered-language", true);
        assert!(plain.error.is_none());
        assert!(plain.spans.iter().all(|(_, s)| s.color.is_none()));
    }
}
