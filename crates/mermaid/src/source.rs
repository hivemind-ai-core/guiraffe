//! Diagram source identity and admission limits.
//!
//! Rendering is cached by content, so a source needs a cheap, stable identity:
//! a 64-bit hash of its normalized text plus the theme and scale it was drawn
//! with. Keeping this pure lets the cache be tested without GPUI or the
//! renderer.

/// Largest diagram source accepted for rendering, in bytes.
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
/// Largest diagram source accepted, in lines. Guards against pathological
/// single-diagram inputs that are cheap in bytes but expensive to lay out.
pub const MAX_SOURCE_LINES: usize = 2_000;

/// Why a diagram source was rejected before parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// The source is blank (only whitespace).
    Empty,
    /// The source exceeds [`MAX_SOURCE_BYTES`].
    TooLarge { bytes: usize },
    /// The source exceeds [`MAX_SOURCE_LINES`].
    TooManyLines { lines: usize },
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "The diagram is empty."),
            Self::TooLarge { bytes } => write!(
                f,
                "The diagram is {bytes} bytes; the limit is {MAX_SOURCE_BYTES} bytes."
            ),
            Self::TooManyLines { lines } => write!(
                f,
                "The diagram has {lines} lines; the limit is {MAX_SOURCE_LINES} lines."
            ),
        }
    }
}

/// Normalize a diagram source for hashing and comparison.
///
/// Trailing whitespace on each line is dropped, and leading/trailing blank
/// lines are dropped, so an agent chat that streams a fence keeps the same
/// identity as it settles. Interior layout is preserved: Mermaid indentation
/// and blank lines inside subgraphs are significant.
pub fn normalize(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        out.push_str(line.trim_end());
        out.push('\n');
    }
    let trimmed = out.trim_start_matches('\n');
    let trimmed = trimmed.trim_end_matches('\n');
    if trimmed.is_empty() {
        return String::new();
    }
    trimmed.to_string()
}

/// A stable 64-bit content identity (FNV-1a).
///
/// Chosen over `DefaultHasher` because its output is fixed across builds and
/// platforms: a cache key may be persisted or compared between processes, and
/// `DefaultHasher`'s algorithm is explicitly not stable.
pub fn hash(source: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in source.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// A cache identity: the normalized source hash plus everything that changes
/// the picture drawn for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub source: u64,
    pub source_len: u32,
    pub dark: bool,
    /// Zoom is not part of the key: layout is zoom-independent, and only the
    /// raster is re-rendered, keyed on the pixel size.
    pub font_size_milli: u32,
}

impl CacheKey {
    /// Key for `source` drawn in `dark` mode at `font_size` points.
    pub fn new(source: &str, dark: bool, font_size: f32) -> Self {
        Self {
            source: hash(source),
            // Mixed into the key so a hash collision on different lengths is
            // far likelier to be caught.
            source_len: source.len().min(u32::MAX as usize) as u32,
            dark,
            font_size_milli: (font_size.max(0.0) * 1000.0) as u32,
        }
    }
}

/// Validate a source before handing it to the renderer.
///
/// Returns the normalized source so callers hash exactly what they render.
pub fn admit(source: &str) -> Result<String, SourceError> {
    let normalized = normalize(source);
    if normalized.is_empty() {
        return Err(SourceError::Empty);
    }
    if source.len() > MAX_SOURCE_BYTES {
        return Err(SourceError::TooLarge {
            bytes: source.len(),
        });
    }
    let lines = source.lines().count();
    if lines > MAX_SOURCE_LINES {
        return Err(SourceError::TooManyLines { lines });
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_drops_trailing_blanks_and_outer_blank_lines() {
        assert_eq!(
            normalize("\n\nflowchart LR\n  A-->B  \n\n"),
            "flowchart LR\n  A-->B"
        );
        assert_eq!(normalize("   \n\t\n"), "");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn normalize_keeps_interior_layout() {
        // Interior blank lines and indentation change layout and must survive.
        let source = "flowchart TD\n  subgraph one\n    A-->B\n\n  end";
        assert_eq!(normalize(source), source);
    }

    #[test]
    fn streaming_whitespace_keeps_identity() {
        let settled = "flowchart LR\n  A --> B";
        let streaming = "flowchart LR\n  A --> B  \n\n";
        assert_eq!(normalize(settled), normalize(streaming));
        assert_eq!(hash(&normalize(settled)), hash(&normalize(streaming)));
    }

    #[test]
    fn hash_is_stable_and_sensitive() {
        // Pinned so a persistent cache entry from an older build stays valid
        // only as long as the algorithm is unchanged.
        assert_eq!(hash(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(hash("a"), hash("a"));
        assert_ne!(hash("a"), hash("b"));
        assert_ne!(hash("ab"), hash("ba"));
    }

    #[test]
    fn cache_key_separates_theme_and_font_size() {
        let source = "flowchart LR\nA-->B";
        let light = CacheKey::new(source, false, 14.0);
        assert_eq!(light, CacheKey::new(source, false, 14.0));
        assert_ne!(light, CacheKey::new(source, true, 14.0));
        assert_ne!(light, CacheKey::new(source, false, 16.0));
        assert_ne!(light, CacheKey::new("flowchart LR\nA-->C", false, 14.0));
    }

    #[test]
    fn admit_rejects_blank_oversized_and_tall_sources() {
        assert_eq!(admit("  \n\n"), Err(SourceError::Empty));
        assert_eq!(
            admit(&"x".repeat(MAX_SOURCE_BYTES + 1)),
            Err(SourceError::TooLarge {
                bytes: MAX_SOURCE_BYTES + 1
            })
        );
        let tall = "a\n".repeat(MAX_SOURCE_LINES + 1);
        assert_eq!(
            admit(&tall),
            Err(SourceError::TooManyLines {
                lines: MAX_SOURCE_LINES + 1
            })
        );
        assert!(admit("flowchart LR\nA-->B").is_ok());
    }

    #[test]
    fn display_messages_name_the_limit() {
        assert!(SourceError::Empty.to_string().contains("empty"));
        assert!(
            SourceError::TooLarge { bytes: 10 }
                .to_string()
                .contains("65536")
        );
        assert!(
            SourceError::TooManyLines { lines: 10 }
                .to_string()
                .contains("2000")
        );
    }
}
