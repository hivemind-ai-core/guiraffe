#![no_main]

use guiraffe_code::{CodeDocument, Position};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if bytes.len() > 16 * 1024 {
        return;
    }
    let source = String::from_utf8_lossy(bytes);
    let doc = CodeDocument::new(source.as_ref());
    let mut start = 0;
    let mut count = 0;
    for (row, raw) in source.split('\n').enumerate() {
        let text = raw.strip_suffix('\r').unwrap_or(raw);
        assert_eq!(doc.line(row), Some(text));
        assert_eq!(doc.line_range(row), Some(start..start + text.len()));
        for (column, _) in text.char_indices().chain([(text.len(), '\0')]) {
            let position = Position::new(row, column);
            assert_eq!(doc.offset(position), Some(start + column));
            assert_eq!(doc.position(start + column), Some(position));
        }
        start += raw.len() + 1;
        count += 1;
    }
    assert_eq!(doc.line_count(), count);
    assert_eq!(doc.line(count), None);
    assert_eq!(doc.position(source.len() + 1), None);
});
