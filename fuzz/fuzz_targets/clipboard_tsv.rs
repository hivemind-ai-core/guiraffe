#![no_main]

use guiraffe_datagrid::tsv::{parse_tsv, write_tsv};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if bytes.len() > 16 * 1024 {
        return;
    }
    let source = String::from_utf8_lossy(bytes);
    let block = parse_tsv(&source);
    let (rows, cols) = (block.rows(), block.cols());
    assert!((1..=source.len() + 1).contains(&rows));
    assert!((1..=source.len() + 1).contains(&cols));
    assert_eq!(block.get(rows, 0), None);
    assert_eq!(block.get(0, cols), None);

    // Ragged input can describe a much larger rectangle than its byte length.
    if rows.saturating_mul(cols) > 16 * 1024 {
        return;
    }
    let columns: Vec<_> = (0..cols).collect();
    let mut encoded = String::new();
    assert!(write_tsv(
        &mut encoded,
        0..rows,
        &columns,
        |row, col, out| {
            out.push_str(block.get(row, col).unwrap());
        }
    ));
    // The parser ignores one terminal row break, which is also how an empty
    // final single-column row is encoded. Append an explicit delimiter.
    encoded.push('\n');
    assert_eq!(parse_tsv(&encoded), block);
});
