use crate::CellPos;

/// Parse a one-based A1 address into zero-based coordinates.
pub fn parse_a1(text: &str) -> Option<CellPos> {
    let split = text.bytes().take_while(u8::is_ascii_alphabetic).count();
    let (letters, digits) = text.split_at(split);
    if letters.is_empty() || digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let col = letters.bytes().try_fold(0usize, |col, letter| {
        col.checked_mul(26)?
            .checked_add(usize::from(letter.to_ascii_uppercase() - b'A') + 1)
    })?;
    Some(CellPos::new(
        digits.parse::<usize>().ok()?.checked_sub(1)?,
        col - 1,
    ))
}

/// Format a zero-based column as a spreadsheet column label.
pub fn column_name(mut col: usize) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push(char::from(b'A' + (col % 26) as u8));
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    letters.into_iter().rev().collect()
}

/// Format an address using one-based row labels.
pub fn a1(pos: CellPos) -> String {
    format!("{}{}", column_name(pos.col), pos.row as u128 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn round_trip(row in 0usize..1_000_000, col in 0usize..1_000_000) {
            let pos = CellPos::new(row, col);
            prop_assert_eq!(parse_a1(&a1(pos)), Some(pos));
        }

        #[test]
        fn arbitrary_input(text in any::<String>()) {
            if let Some(pos) = parse_a1(&text) {
                prop_assert_eq!(parse_a1(&a1(pos)), Some(pos));
            }
        }
    }
}
