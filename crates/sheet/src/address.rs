use crate::CellPos;

/// Parse a one-based A1 address into zero-based coordinates.
pub fn parse_a1(text: &str) -> Option<CellPos> {
    parse_reference(text).map(|(pos, _, _)| pos)
}

pub(crate) fn parse_reference(text: &str) -> Option<(CellPos, bool, bool)> {
    let fixed_col = text.starts_with('$');
    let text = text.strip_prefix('$').unwrap_or(text);
    let split = text.bytes().take_while(u8::is_ascii_alphabetic).count();
    let (letters, row) = text.split_at(split);
    let fixed_row = row.starts_with('$');
    let digits = row.strip_prefix('$').unwrap_or(row);
    if letters.is_empty() || digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let col = letters.bytes().try_fold(0u128, |col, letter| {
        col.checked_mul(26)?
            .checked_add(u128::from(letter.to_ascii_uppercase() - b'A') + 1)
    })?;
    let row = digits
        .parse::<u128>()
        .ok()?
        .checked_sub(1)?
        .try_into()
        .ok()?;
    Some((
        CellPos::new(row, (col - 1).try_into().ok()?),
        fixed_row,
        fixed_col,
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
