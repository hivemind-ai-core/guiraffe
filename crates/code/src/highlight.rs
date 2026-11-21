//! Parse on a worker. Resolve styles only for the source range being shaped.
use crate::CodeDocument;
use gpui_kit::{
    HighlightStyle,
    component::highlighter::{HighlightTheme, SyntaxHighlighter},
};
use std::{ops::Range, sync::Arc, time::Duration};

#[derive(Clone)]
pub struct Highlighted {
    pub(crate) document: Arc<CodeDocument>,
    parser: Option<Arc<SyntaxHighlighter>>,
    pub error: Option<Arc<str>>,
    dark: bool,
}

impl std::fmt::Debug for Highlighted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Highlighted")
            .field("document", &self.document)
            .field("has_parser", &self.has_parser())
            .field("error", &self.error)
            .field("dark", &self.dark)
            .finish()
    }
}

impl Highlighted {
    pub fn document(&self) -> &Arc<CodeDocument> {
        &self.document
    }

    pub fn plain(document: Arc<CodeDocument>) -> Self {
        Self {
            document,
            parser: None,
            error: None,
            dark: false,
        }
    }

    pub(crate) fn has_parser(&self) -> bool {
        self.parser.is_some()
    }

    /// Call on a background executor. Unknown languages retain plain text; parser timeout or
    /// panic is reported without discarding source. Rust, JSON and JavaScript ship by default;
    /// hosts can register further Controls grammars.
    pub fn compute(document: Arc<CodeDocument>, language: &str, dark: bool) -> Self {
        Self::compute_cancellable(document.clone(), language, dark, || false)
            .unwrap_or_else(|| Self::plain(document))
    }

    pub(crate) fn compute_cancellable(
        document: Arc<CodeDocument>,
        language: &str,
        dark: bool,
        cancelled: impl Fn() -> bool,
    ) -> Option<Self> {
        if cancelled() {
            return None;
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut parser = SyntaxHighlighter::new(language);
            let rope = ropey::Rope::from_str(document.source());
            if cancelled() {
                return Ok(None);
            }
            if !parser.update(None, &rope, Some(Duration::from_secs(2))) {
                return Err("Syntax analysis exceeded its time budget");
            }
            Ok(Some(parser))
        }));
        if cancelled() {
            return None;
        }
        Some(match result {
            Ok(Ok(None)) => return None,
            Ok(Ok(Some(parser))) => Self {
                document,
                parser: parser.tree().is_some().then(|| Arc::new(parser)),
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
        })
    }

    pub fn styles_for(
        &self,
        range: Range<usize>,
    ) -> impl Iterator<Item = (Range<usize>, HighlightStyle)> {
        self.styles_for_scheme(range, self.dark)
    }

    pub fn styles_for_scheme(
        &self,
        mut range: Range<usize>,
        dark: bool,
    ) -> impl Iterator<Item = (Range<usize>, HighlightStyle)> {
        range.end = range.end.min(self.document.source().len());
        let styles = self
            .parser
            .as_ref()
            .filter(|_| !range.is_empty())
            .map(|parser| {
                let theme = if dark {
                    HighlightTheme::default_dark()
                } else {
                    HighlightTheme::default_light()
                };
                parser.styles(&range, theme.as_ref())
            })
            .unwrap_or_default();
        styles
            .into_iter()
            .filter(|(_, style)| style != &HighlightStyle::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distant_window_preserves_multiline_syntax_and_new_source_replaces_it() {
        let source = "/* opening\n".to_owned() + &"let first = 42;\n".repeat(10_000) + "*/\n";
        let doc = Arc::new(CodeDocument::new(source));
        let parsed = std::thread::spawn(move || Highlighted::compute(doc, "rust", false))
            .join()
            .expect("parser worker");
        assert!(parsed.error.is_none());
        let window = parsed.document.line_range(9_999).expect("source line");
        let near = parsed.document.line_range(1).expect("source line");
        let styles = parsed.styles_for(window.clone()).collect::<Vec<_>>();
        assert_eq!(styles.len(), 1);
        assert_eq!(styles[0].0, window);
        assert_eq!(
            styles[0].1,
            parsed.styles_for(near).next().expect("comment style").1
        );

        let edited = Arc::new(CodeDocument::new("let first = 42;\n".repeat(10_000)));
        let edited = Highlighted::compute(edited, "rust", false);
        let window = edited.document.line_range(9_999).expect("edited line");
        assert_ne!(
            edited.styles_for(window.clone()).collect::<Vec<_>>(),
            styles
        );
        let plain = Highlighted::compute(edited.document.clone(), "unregistered-language", true);
        assert!(!plain.has_parser());
        assert!(plain.styles_for(window).next().is_none());
    }
}
