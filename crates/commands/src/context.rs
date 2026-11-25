//! The portable subset of GPUI when expressions: flags, equality, negation,
//! conjunction, disjunction and strict ancestor (`>`) relationships.
use crate::Error;
use std::fmt;
use std::{collections::BTreeMap, ops::Deref};
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    entries: BTreeMap<String, Option<String>>,
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}
impl Context {
    pub fn new(name: &str) -> Result<Self, Error> {
        Self::parse(name)
    }
    pub fn parse(source: &str) -> Result<Self, Error> {
        let tokens = lex(source)?;
        let mut entries = BTreeMap::new();
        let mut at = 0;
        while let Some(Token::Word(key)) = tokens.get(at) {
            let key = key.clone();
            at += 1;
            let value = if tokens.get(at) == Some(&Token::Assign) {
                at += 1;
                let Some(Token::Word(value)) = tokens.get(at) else {
                    return Err(Error::InvalidContext(source.into()));
                };
                at += 1;
                Some(value.clone())
            } else {
                None
            };
            if entries.insert(key, value).is_some() {
                return Err(Error::InvalidContext(source.into()));
            }
        }
        if at != tokens.len() {
            return Err(Error::InvalidContext(source.into()));
        }
        Ok(Self { entries })
    }
    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).and_then(Option::as_deref)
    }
    pub fn entries(&self) -> impl Iterator<Item = (&str, Option<&str>)> {
        self.entries
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_deref()))
    }
    pub fn insert(&mut self, key: &str, value: Option<&str>) -> Result<(), Error> {
        if !identifier(key) || value.is_some_and(|v| !identifier(v)) {
            return Err(Error::InvalidContext(key.into()));
        }
        self.entries.insert(key.into(), value.map(str::to_owned));
        Ok(())
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextStack {
    frames: Vec<Context>,
}
impl ContextStack {
    pub fn push(&mut self, context: Context) {
        self.frames.push(context);
    }
    pub fn pop(&mut self) -> Option<Context> {
        self.frames.pop()
    }
    pub fn contains(&self, key: &str) -> bool {
        self.frames.iter().any(|frame| frame.contains(key))
    }
    /// The value of `key` in the deepest frame that sets it.
    pub fn value(&self, key: &str) -> Option<&str> {
        self.frames.iter().rev().find_map(|frame| frame.get(key))
    }
    pub fn frames(&self) -> &[Context] {
        &self.frames
    }
    pub fn extend(&mut self, other: &Self) {
        self.frames.extend(other.frames.iter().cloned());
    }
    /// The guard exposes read access and nested scopes, never removal of its ancestors.
    pub fn scoped(&mut self, context: Context) -> Scope<'_> {
        let depth = self.frames.len();
        self.push(context);
        Scope { stack: self, depth }
    }
}
pub struct Scope<'a> {
    stack: &'a mut ContextStack,
    depth: usize,
}
impl Scope<'_> {
    pub fn scoped(&mut self, context: Context) -> Scope<'_> {
        self.stack.scoped(context)
    }
}
impl Deref for Scope<'_> {
    type Target = ContextStack;
    fn deref(&self) -> &Self::Target {
        self.stack
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.stack.frames.truncate(self.depth);
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct When {
    source: String,
    expr: Expr,
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum Expr {
    Always,
    Has(String),
    Equal(String, String),
    NotEqual(String, String),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Descendant(Box<Expr>, Box<Expr>),
}
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Always => Ok(()),
            Self::Has(key) => f.write_str(key),
            Self::Equal(key, value) => write!(f, "{key} == {value}"),
            Self::NotEqual(key, value) => write!(f, "{key} != {value}"),
            Self::Not(inner) => write!(f, "!({inner})"),
            Self::And(a, b) => write!(f, "({a} && {b})"),
            Self::Or(a, b) => write!(f, "({a} || {b})"),
            Self::Descendant(a, b) => write!(f, "({a} > {b})"),
        }
    }
}
impl Default for When {
    fn default() -> Self {
        Self::always()
    }
}
impl When {
    pub fn always() -> Self {
        Self {
            source: String::new(),
            expr: Expr::Always,
        }
    }
    pub fn parse(source: &str) -> Result<Self, Error> {
        let tokens = lex(source)?;
        if tokens.is_empty() {
            return Err(Error::InvalidContext(source.into()));
        }
        let mut parser = Parser { tokens, at: 0 };
        let expr = parser.expression(0)?;
        if parser.at != parser.tokens.len() {
            return Err(Error::InvalidContext(source.into()));
        }
        Ok(Self {
            source: expr.to_string(),
            expr,
        })
    }
    pub fn as_str(&self) -> &str {
        &self.source
    }
    pub fn matches(&self, contexts: &ContextStack) -> bool {
        self.depth(contexts).is_some()
    }
    /// Specificity: the deepest stack depth at which the expression matches (GPUI's
    /// `depth_of`), or `None`. An unconditional `When::always()` is the least specific
    /// (depth 0), so any matching context-specific binding outranks it.
    pub fn depth(&self, contexts: &ContextStack) -> Option<usize> {
        if matches!(self.expr, Expr::Always) {
            return Some(0);
        }
        (1..=contexts.frames.len())
            .rev()
            .find(|&depth| eval(&self.expr, &contexts.frames[..depth], &contexts.frames))
    }
}
fn eval(expr: &Expr, frames: &[Context], all: &[Context]) -> bool {
    if matches!(expr, Expr::Always) {
        return true;
    }
    let Some(frame) = frames.last() else {
        return false;
    };
    match expr {
        Expr::Always => true,
        Expr::Has(key) => frame.contains(key),
        Expr::Equal(key, value) => frame.get(key) == Some(value.as_str()),
        Expr::NotEqual(key, value) => frame.get(key) != Some(value.as_str()),
        Expr::Not(inner) => !(1..=all.len()).any(|end| eval(inner, &all[..end], all)),
        Expr::And(a, b) => eval(a, frames, all) && eval(b, frames, all),
        Expr::Or(a, b) => eval(a, frames, all) || eval(b, frames, all),
        Expr::Descendant(a, b) => {
            for split in 1..frames.len() {
                if eval(a, &frames[..split], all) {
                    return eval(b, &frames[split..], &frames[split..]);
                }
            }
            false
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    Assign,
    Eq,
    Ne,
    Not,
    And,
    Or,
    Child,
    Open,
    Close,
}
fn lex(source: &str) -> Result<Vec<Token>, Error> {
    if source.len() > 4096 {
        return Err(Error::InvalidContext("expression too long".into()));
    }
    let mut chars = source.chars().peekable();
    let mut result = Vec::new();
    while let Some(c) = chars.next() {
        let token = match c {
            c if c.is_whitespace() => continue,
            '(' => Token::Open,
            ')' => Token::Close,
            '>' => Token::Child,
            '=' => {
                if chars.next_if_eq(&'=').is_some() {
                    Token::Eq
                } else {
                    Token::Assign
                }
            }
            '!' => {
                if chars.next_if_eq(&'=').is_some() {
                    Token::Ne
                } else {
                    Token::Not
                }
            }
            '&' if chars.next_if_eq(&'&').is_some() => Token::And,
            '|' if chars.next_if_eq(&'|').is_some() => Token::Or,
            c if c.is_alphanumeric() || c == '_' || c == '-' => {
                let mut word = String::from(c);
                while let Some(next) =
                    chars.next_if(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                {
                    word.push(next);
                }
                Token::Word(word)
            }
            _ => return Err(Error::InvalidContext(source.into())),
        };
        result.push(token);
        if result.len() > 256 {
            return Err(Error::InvalidContext("too many expression tokens".into()));
        }
    }
    Ok(result)
}
struct Parser {
    tokens: Vec<Token>,
    at: usize,
}
impl Parser {
    fn expression(&mut self, min: u8) -> Result<Expr, Error> {
        let token = self
            .tokens
            .get(self.at)
            .cloned()
            .ok_or_else(|| Error::InvalidContext("missing operand".into()))?;
        self.at += 1;
        let mut left = match token {
            Token::Word(key) => Expr::Has(key),
            Token::Not => Expr::Not(Box::new(self.expression(5)?)),
            Token::Open => {
                let inner = self.expression(0)?;
                if self.tokens.get(self.at) != Some(&Token::Close) {
                    return Err(Error::InvalidContext("missing closing parenthesis".into()));
                }
                self.at += 1;
                inner
            }
            _ => return Err(Error::InvalidContext("missing operand".into())),
        };
        loop {
            let precedence = match self.tokens.get(self.at) {
                Some(Token::Child) => 1,
                Some(Token::Or) => 2,
                Some(Token::And) => 3,
                Some(Token::Eq | Token::Ne) => 4,
                _ => break,
            };
            if precedence < min {
                break;
            }
            let operator = self.tokens.get(self.at).cloned();
            self.at += 1;
            let right = self.expression(precedence + 1)?;
            left = match operator {
                Some(Token::Or) => Expr::Or(Box::new(left), Box::new(right)),
                Some(Token::And) => Expr::And(Box::new(left), Box::new(right)),
                Some(Token::Eq | Token::Ne) => match (left, right) {
                    (Expr::Has(key), Expr::Has(value)) => {
                        if operator == Some(Token::Ne) {
                            Expr::NotEqual(key, value)
                        } else {
                            Expr::Equal(key, value)
                        }
                    }
                    _ => {
                        return Err(Error::InvalidContext(
                            "comparison operands must be identifiers".into(),
                        ));
                    }
                },
                _ => Expr::Descendant(Box::new(left), Box::new(right)),
            };
        }
        Ok(left)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn stack(frames: &[&str]) -> ContextStack {
        let mut result = ContextStack::default();
        for frame in frames {
            result.push(Context::parse(frame).expect("valid"));
        }
        result
    }
    #[test]
    fn flags_boolean_equality_and_ancestors() {
        let s = stack(&["Workspace", "Pane", "Editor mode=vim enabled"]);
        for query in [
            "Workspace",
            "Editor",
            "Editor && enabled",
            "Editor && !Readonly",
            "mode == vim",
            "mode != insert",
            "Workspace > Editor",
            "Workspace > Pane > Editor",
            "(Editor || Terminal) && !Readonly",
        ] {
            assert!(When::parse(query).expect("query").matches(&s), "{query}");
        }
        for query in [
            "Editor > Workspace",
            "Readonly",
            "mode == insert",
            "Workspace && Editor",
            "Editor && Readonly",
            "!Workspace",
        ] {
            assert!(!When::parse(query).expect("query").matches(&s), "{query}");
        }
    }
    #[test]
    fn negation_scans_ancestors_and_descendant_is_strict() {
        let s = stack(&["Readonly", "Editor"]);
        assert!(
            !When::parse("Editor && !Readonly")
                .expect("query")
                .matches(&s)
        );
        assert!(!When::parse("Editor > Editor").expect("query").matches(&s));
        assert!(When::always().matches(&ContextStack::default()));
    }
    #[test]
    fn scopes_restore_even_during_unwind() {
        let mut s = stack(&["Workspace"]);
        {
            let mut editor = s.scoped(Context::new("Editor").expect("context"));
            assert!(editor.contains("Editor"));
            {
                let nested = editor.scoped(Context::new("Readonly").expect("context"));
                assert!(nested.contains("Readonly"));
            }
            assert!(!editor.contains("Readonly"));
        }
        assert!(!s.contains("Editor"));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _scope = s.scoped(Context::new("Temporary").expect("context"));
            panic!("test unwind");
        }));
        assert_eq!(s.frames().len(), 1);
        assert!(s.pop().is_some());
        assert!(s.pop().is_none());
    }
    #[test]
    fn malformed_predicates_and_frames_are_rejected() {
        for query in [
            "", "A & B", "A | B", "A &&", "!", "(", "A B", "A ==", "A = B", "A > > B", "A)",
            "A @ B",
        ] {
            assert!(When::parse(query).is_err(), "{query}");
        }
        for frame in ["Editor =", "A A", "A == B", "A @ B"] {
            assert!(Context::parse(frame).is_err());
        }
        assert!(When::parse(&"!".repeat(300)).is_err());
    }
    #[test]
    fn equality_spacing_and_unicode_contexts() {
        let s = stack(&["工程 mode = vim flag"]);
        assert!(s.contains("工程"));
        assert!(When::parse("工程 && mode==vim").expect("query").matches(&s));
        assert_eq!(s.frames().first().and_then(|f| f.get("mode")), Some("vim"));
    }
    #[test]
    fn boolean_truth_tables() {
        for a in [false, true] {
            for b in [false, true] {
                let mut frame = Context::default();
                if a {
                    frame.insert("A", None).expect("flag");
                }
                if b {
                    frame.insert("B", None).expect("flag");
                }
                let s = ContextStack {
                    frames: vec![frame],
                };
                for (query, value) in [
                    ("A && B", a && b),
                    ("A || B", a || b),
                    ("!(A && B)", !(a && b)),
                    ("A || B && !A", a || b && !a),
                ] {
                    assert_eq!(
                        When::parse(query).expect("query").matches(&s),
                        value,
                        "{query}"
                    );
                }
            }
        }
    }
}
