use std::fmt;

use crate::{CellPos, parse_a1};

/// An inclusive rectangular dependency.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Reference {
    pub first: CellPos,
    pub last: CellPos,
}

impl Reference {
    pub fn new(a: CellPos, b: CellPos) -> Self {
        Self {
            first: CellPos::new(a.row.min(b.row), a.col.min(b.col)),
            last: CellPos::new(a.row.max(b.row), a.col.max(b.col)),
        }
    }

    pub fn contains(self, pos: CellPos) -> bool {
        (self.first.row..=self.last.row).contains(&pos.row)
            && (self.first.col..=self.last.col).contains(&pos.col)
    }
}

/// Spreadsheet errors produced by the evaluator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FormulaError {
    Syntax,
    Reference,
    Value,
    DivideByZero,
    Cycle,
    Cancelled,
}

impl fmt::Display for FormulaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Syntax => "#PARSE!",
            Self::Reference => "#REF!",
            Self::Value => "#VALUE!",
            Self::DivideByZero => "#DIV/0!",
            Self::Cycle => "#CIRC!",
            Self::Cancelled => "#BUSY!",
        })
    }
}

impl std::error::Error for FormulaError {}

/// Numeric values exposed to a formula. A missing cell is distinct from numeric zero.
pub trait FormulaValues {
    fn number(&self, pos: CellPos) -> Result<Option<f64>, FormulaError>;
    fn visit(
        &self,
        range: Reference,
        visitor: &mut dyn FnMut(f64),
    ) -> Result<(), FormulaError>;
}

/// A compiled formula. Implementations declare every dependency they may read.
pub trait Formula: Send {
    fn references(&self) -> &[Reference];
    fn evaluate(&self, values: &dyn FormulaValues) -> Result<f64, FormulaError>;
}

/// A pluggable compiler, called on the recalculation thread.
pub trait FormulaEngine: Send + Sync + 'static {
    /// Compile an expression following the initial equals sign.
    fn compile(&self, expression: &str) -> Result<Box<dyn Formula>, FormulaError>;
}

/// Standard arithmetic and spreadsheet aggregates.
#[derive(Default)]
pub struct BuiltinEngine;

#[derive(Clone, Copy)]
enum Function {
    Sum,
    Avg,
    Min,
    Max,
    Count,
    If,
}

enum Expr {
    Number(f64),
    Ref(CellPos),
    Range(Reference),
    Neg(Box<Self>),
    Binary(u8, Box<Self>, Box<Self>),
    Call(Function, Vec<Self>),
}

impl Expr {
    fn eval(&self, values: &dyn FormulaValues) -> Result<f64, FormulaError> {
        let result = match self {
            Self::Number(value) => *value,
            Self::Ref(pos) => values.number(*pos)?.unwrap_or_default(),
            Self::Range(_) => return Err(FormulaError::Value),
            Self::Neg(expr) => -expr.eval(values)?,
            Self::Binary(op, left, right) => {
                let a = left.eval(values)?;
                let b = right.eval(values)?;
                match op {
                    b'+' => a + b,
                    b'-' => a - b,
                    b'*' => a * b,
                    b'/' if b == 0. => return Err(FormulaError::DivideByZero),
                    b'/' => a / b,
                    b'^' => a.powf(b),
                    b'=' => f64::from(a == b),
                    b'<' => f64::from(a < b),
                    b'>' => f64::from(a > b),
                    b'l' => f64::from(a <= b),
                    b'g' => f64::from(a >= b),
                    b'n' => f64::from(a != b),
                    _ => return Err(FormulaError::Syntax),
                }
            }
            Self::Call(Function::If, args) => {
                let [condition, yes, no] = args.as_slice() else {
                    return Err(FormulaError::Syntax);
                };
                if condition.eval(values)? != 0. {
                    yes.eval(values)?
                } else {
                    no.eval(values)?
                }
            }
            Self::Call(function, args) => {
                let (mut sum, mut count, mut min, mut max) =
                    (0., 0u64, f64::INFINITY, f64::NEG_INFINITY);
                let mut visit = |value: f64| {
                    sum += value;
                    count += 1;
                    min = min.min(value);
                    max = max.max(value);
                };
                for arg in args {
                    match arg {
                        Self::Range(range) => values.visit(*range, &mut visit)?,
                        Self::Ref(pos) => {
                            if let Some(value) = values.number(*pos)? {
                                visit(value);
                            }
                        }
                        expr => visit(expr.eval(values)?),
                    }
                }
                match function {
                    Function::Sum => sum,
                    Function::Avg if count == 0 => return Err(FormulaError::DivideByZero),
                    Function::Avg => sum / count as f64,
                    Function::Min if count > 0 => min,
                    Function::Max if count > 0 => max,
                    Function::Count => count as f64,
                    _ => 0.,
                }
            }
        };
        result.is_finite().then_some(result).ok_or(FormulaError::Value)
    }
}

struct Compiled {
    expr: Expr,
    references: Vec<Reference>,
}

impl Formula for Compiled {
    fn references(&self) -> &[Reference] {
        &self.references
    }

    fn evaluate(&self, values: &dyn FormulaValues) -> Result<f64, FormulaError> {
        self.expr.eval(values)
    }
}

impl FormulaEngine for BuiltinEngine {
    fn compile(&self, expression: &str) -> Result<Box<dyn Formula>, FormulaError> {
        // Bound tree depth, including left-associative expressions.
        if expression.len() > 8192 {
            return Err(FormulaError::Syntax);
        }
        let mut parser = Parser { rest: expression, references: Vec::new(), budget: 256 };
        let expr = parser.expression(0)?;
        if parser.peek().is_some() {
            return Err(FormulaError::Syntax);
        }
        Ok(Box::new(Compiled { expr, references: parser.references }))
    }
}

struct Parser<'a> {
    rest: &'a str,
    references: Vec<Reference>,
    budget: usize,
}

impl<'a> Parser<'a> {
    fn peek(&mut self) -> Option<u8> {
        self.rest = self.rest.trim_start();
        self.rest.as_bytes().first().copied()
    }

    fn eat(&mut self, token: u8) -> bool {
        if self.peek() == Some(token) {
            self.rest = &self.rest[1..];
            true
        } else {
            false
        }
    }

    fn take(&mut self, predicate: impl FnMut(&u8) -> bool) -> &'a str {
        let length = self.rest.bytes().take_while(predicate).count();
        let (token, rest) = self.rest.split_at(length);
        self.rest = rest;
        token
    }

    fn node(&mut self) -> Result<(), FormulaError> {
        self.budget = self.budget.checked_sub(1).ok_or(FormulaError::Syntax)?;
        Ok(())
    }

    fn expression(&mut self, minimum: u8) -> Result<Expr, FormulaError> {
        self.node()?;
        let mut left = self.atom()?;
        while let Some(op) = self.peek() {
            let power = match op {
                b'=' | b'<' | b'>' => 1,
                b'+' | b'-' => 2,
                b'*' | b'/' => 3,
                b'^' => 5,
                _ => break,
            };
            if power < minimum {
                break;
            }
            self.eat(op);
            let op = match op {
                b'<' if self.eat(b'=') => b'l',
                b'<' if self.eat(b'>') => b'n',
                b'>' if self.eat(b'=') => b'g',
                op => op,
            };
            let right = self.expression(power + u8::from(op != b'^'))?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn atom(&mut self) -> Result<Expr, FormulaError> {
        match self.peek().ok_or(FormulaError::Syntax)? {
            b'+' => {
                self.eat(b'+');
                self.expression(4)
            }
            b'-' => {
                self.eat(b'-');
                Ok(Expr::Neg(Box::new(self.expression(4)?)))
            }
            b'(' => {
                self.eat(b'(');
                let expr = self.expression(0)?;
                self.eat(b')').then_some(expr).ok_or(FormulaError::Syntax)
            }
            b'0'..=b'9' | b'.' => {
                let original = self.rest;
                self.take(|b| b.is_ascii_digit() || *b == b'.');
                if self.rest.starts_with(['e', 'E']) {
                    self.rest = &self.rest[1..];
                    if self.rest.starts_with(['+', '-']) {
                        self.rest = &self.rest[1..];
                    }
                    self.take(u8::is_ascii_digit);
                }
                original[..original.len() - self.rest.len()]
                    .parse()
                    .map(Expr::Number)
                    .map_err(|_| FormulaError::Syntax)
            }
            b'A'..=b'Z' | b'a'..=b'z' => self.word(),
            _ => Err(FormulaError::Syntax),
        }
    }

    fn word(&mut self) -> Result<Expr, FormulaError> {
        let word = self.take(u8::is_ascii_alphanumeric);
        if self.eat(b'(') {
            let function = match word.to_ascii_uppercase().as_str() {
                "SUM" => Function::Sum,
                "AVG" | "AVERAGE" => Function::Avg,
                "MIN" => Function::Min,
                "MAX" => Function::Max,
                "COUNT" => Function::Count,
                "IF" => Function::If,
                _ => return Err(FormulaError::Syntax),
            };
            let mut args = Vec::new();
            if !self.eat(b')') {
                loop {
                    args.push(self.expression(0)?);
                    if self.eat(b')') {
                        break;
                    }
                    if !self.eat(b',') {
                        return Err(FormulaError::Syntax);
                    }
                }
            }
            return Ok(Expr::Call(function, args));
        }
        let start = parse_a1(word).ok_or(FormulaError::Syntax)?;
        if self.eat(b':') {
            self.peek();
            let end = parse_a1(self.take(u8::is_ascii_alphanumeric)).ok_or(FormulaError::Syntax)?;
            let range = Reference::new(start, end);
            self.references.push(range);
            Ok(Expr::Range(range))
        } else {
            self.references.push(Reference::new(start, start));
            Ok(Expr::Ref(start))
        }
    }
}
