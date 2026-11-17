//! The 15-field form's rules (contract: `examples/compare/README.md`). Pure logic.

pub const FIELDS: usize = 15;

pub const LABELS: [&str; FIELDS] = [
    "First name",
    "Last name",
    "Email",
    "Phone",
    "Company",
    "Job title",
    "Website",
    "Age",
    "Country",
    "City",
    "Postal code",
    "Password",
    "Confirm password",
    "Bio",
    "Accept terms",
];

/// Indices into [`LABELS`].
pub mod f {
    pub const FIRST: usize = 0;
    pub const LAST: usize = 1;
    pub const EMAIL: usize = 2;
    pub const PHONE: usize = 3;
    pub const WEBSITE: usize = 6;
    pub const AGE: usize = 7;
    pub const COUNTRY: usize = 8;
    pub const CITY: usize = 9;
    pub const POSTAL: usize = 10;
    pub const PASSWORD: usize = 11;
    pub const CONFIRM: usize = 12;
    pub const BIO: usize = 13;
    pub const TERMS: usize = 14;
}

/// Text of each field; the country is its label ("" when none), terms is "true"/"".
pub type Values = [String; FIELDS];

fn is_email(s: &str) -> bool {
    let Some((user, domain)) = s.split_once('@') else {
        return false;
    };
    let bad = |p: &str| p.is_empty() || p.chars().any(|c| c == '@' || c.is_whitespace());
    if bad(user) || bad(domain) {
        return false;
    }
    // `[^@\s]+\.[^@\s]+`: a dot with something on both sides.
    domain
        .char_indices()
        .any(|(i, c)| c == '.' && i > 0 && i + 1 < domain.len())
}

fn chars_in(s: &str, min: usize, max: usize, ok: impl Fn(char) -> bool) -> bool {
    let n = s.chars().count();
    (min..=max).contains(&n) && s.chars().all(ok)
}

pub fn validate_field(i: usize, v: &Values) -> Option<&'static str> {
    let s = v[i].as_str();
    let t = s.trim();
    match i {
        f::FIRST | f::LAST | f::CITY if t.is_empty() => Some("Required"),
        f::EMAIL if t.is_empty() => Some("Required"),
        f::EMAIL if !is_email(s) => Some("Enter a valid email address"),
        f::PHONE
            if !s.is_empty()
                && !chars_in(s, 7, 20, |c| c.is_ascii_digit() || "+ ()-".contains(c)) =>
        {
            Some("7–20 digits, spaces, + ( ) -")
        }
        f::WEBSITE
            if !s.is_empty() && !(s.starts_with("http://") || s.starts_with("https://")) =>
        {
            Some("Must start with http:// or https://")
        }
        f::AGE => match t.parse::<i64>() {
            _ if t.is_empty() => Some("Required"),
            Ok(n) if (18..=120).contains(&n) => None,
            Ok(_) => Some("Must be between 18 and 120"),
            Err(_) => Some("Must be a whole number"),
        },
        f::COUNTRY if t.is_empty() => Some("Required"),
        f::POSTAL if !chars_in(s, 3, 10, |c| c.is_ascii_alphanumeric() || c == ' ' || c == '-') => {
            Some("3–10 letters, digits, spaces or -")
        }
        f::PASSWORD
            if s.chars().count() < 8
                || !s.chars().any(|c| c.is_alphabetic())
                || !s.chars().any(|c| c.is_ascii_digit()) =>
        {
            Some("At least 8 characters with a letter and a digit")
        }
        f::CONFIRM if s != v[f::PASSWORD] => Some("Passwords do not match"),
        f::BIO if s.chars().count() > 500 => Some("At most 500 characters"),
        f::TERMS if s != "true" => Some("You must accept the terms"),
        _ => None,
    }
}

pub fn validate(v: &Values) -> [Option<&'static str>; FIELDS] {
    std::array::from_fn(|i| validate_field(i, v))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> Values {
        let mut v: Values = Default::default();
        for (i, s) in [
            "Ada",
            "Lovelace",
            "ada@example.com",
            "+44 20 7946 0958",
            "Analytical Engines",
            "Analyst",
            "https://example.com",
            "36",
            "Netherlands",
            "Amsterdam",
            "1011 AB",
            "engine42",
            "engine42",
            "Poet of science.",
            "true",
        ]
        .into_iter()
        .enumerate()
        {
            v[i] = s.to_string();
        }
        v
    }

    #[test]
    fn valid_form_has_no_errors() {
        assert!(validate(&valid()).iter().all(Option::is_none));
    }

    #[test]
    fn empty_form_errors_on_required_fields_only() {
        let e = validate(&Values::default());
        let failing: Vec<_> = (0..FIELDS).filter(|i| e[*i].is_some()).collect();
        assert_eq!(
            failing,
            vec![
                f::FIRST,
                f::LAST,
                f::EMAIL,
                f::AGE,
                f::COUNTRY,
                f::CITY,
                f::POSTAL,
                f::PASSWORD,
                f::TERMS
            ]
        );
    }

    #[test]
    fn rules() {
        let mut v = valid();
        for (field, bad) in [
            (f::EMAIL, "ada@example"),
            (f::EMAIL, "a da@x.io"),
            (f::PHONE, "12"),
            (f::WEBSITE, "example.com"),
            (f::AGE, "17"),
            (f::AGE, "x"),
            (f::POSTAL, "1!"),
            (f::PASSWORD, "abcdefgh"),
            (f::TERMS, ""),
        ] {
            let old = std::mem::replace(&mut v[field], bad.into());
            assert!(validate_field(field, &v).is_some(), "{field} {bad:?}");
            v[field] = old;
        }
        v[f::CONFIRM] = "other".into();
        assert!(validate_field(f::CONFIRM, &v).is_some());
        v[f::BIO] = "x".repeat(501);
        assert!(validate_field(f::BIO, &v).is_some());
    }
}
