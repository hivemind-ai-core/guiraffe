//! Pure path handling: splitting, percent-encoding of segments, query strings.

/// Percent-encode a path segment or query component (RFC 3986 unreserved set kept as-is).
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Decode percent-encoding; `None` on malformed escapes or invalid UTF-8.
pub fn decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = s.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// A parsed location: decoded path segments and query pairs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Location {
    pub segments: Vec<String>,
    pub query: Vec<(String, String)>,
}

impl Location {
    /// Parse `"/a/b%20c?x=1&y"`. Empty segments (`//`, trailing `/`) are ignored.
    /// Returns `None` if any component is malformed.
    pub fn parse(input: &str) -> Option<Location> {
        let input = input.split('#').next().unwrap_or_default();
        let (path, query) = match input.split_once('?') {
            Some((p, q)) => (p, Some(q)),
            None => (input, None),
        };
        let segments = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(decode)
            .collect::<Option<Vec<_>>>()?;
        let query = match query {
            None => Vec::new(),
            Some(q) => q
                .split('&')
                .filter(|kv| !kv.is_empty())
                .map(|kv| {
                    let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
                    Some((decode(k)?, decode(v)?))
                })
                .collect::<Option<Vec<_>>>()?,
        };
        Some(Location { segments, query })
    }

    pub fn query_value(&self, key: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Format back to a string (canonical: leading `/`, encoded components).
    pub fn format(&self) -> String {
        let mut s = String::from("/");
        s.push_str(
            &self
                .segments
                .iter()
                .map(|x| encode(x))
                .collect::<Vec<_>>()
                .join("/"),
        );
        if !self.query.is_empty() {
            s.push('?');
            s.push_str(
                &self
                    .query
                    .iter()
                    .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encoding() {
        for s in ["plain", "with space", "ünïcødé", "a/b?c&d=e#f%", ""] {
            assert_eq!(decode(&encode(s)).as_deref(), Some(s));
        }
        assert_eq!(decode("%zz"), None);
        assert_eq!(decode("%4"), None);
        assert_eq!(decode("%FF"), None, "invalid utf-8");
        assert_eq!(decode("a+b").as_deref(), Some("a b"));
    }

    #[test]
    fn parse_and_format() {
        let l = Location::parse("//projects/42/?tab=files&q=a%20b&flag#frag").unwrap_or_default();
        assert_eq!(l.segments, vec!["projects", "42"]);
        assert_eq!(l.query_value("tab"), Some("files"));
        assert_eq!(l.query_value("q"), Some("a b"));
        assert_eq!(l.query_value("flag"), Some(""));
        assert_eq!(l.format(), "/projects/42?tab=files&q=a%20b&flag=");
        assert_eq!(
            Location::parse("/").map(|l| l.format()).as_deref(),
            Some("/")
        );
        assert_eq!(Location::parse("/a%zz"), None);
    }
}
