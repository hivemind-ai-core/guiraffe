//! Data access without copying the dataset.
//!
//! The grid never owns or clones rows. It asks a [`GridSource`] for one cell at a time and
//! only for the cells it shows (or sorts/filters, off the UI thread). Cells come back as a
//! borrowed [`CellRef`], so reading a `String` field costs a pointer copy.

use std::{any::Any, cmp::Ordering, fmt::Write as _, sync::Arc};

/// Stable identity of a source row (survives sorting and filtering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct RowKey(pub u64);

/// One cell, borrowed from the source.
#[derive(Clone, Copy)]
pub enum CellRef<'a> {
    Empty,
    Text(&'a str),
    Int(i64),
    Number(f64),
    Bool(bool),
    /// App-defined payload; display it with a column formatter.
    Custom(&'a (dyn Any + Send + Sync)),
}

impl std::fmt::Debug for CellRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("Empty"),
            Self::Text(s) => write!(f, "Text({s:?})"),
            Self::Int(v) => write!(f, "Int({v})"),
            Self::Number(v) => write!(f, "Number({v})"),
            Self::Bool(v) => write!(f, "Bool({v})"),
            Self::Custom(_) => f.write_str("Custom(..)"),
        }
    }
}

impl PartialEq for CellRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.total_cmp(other) == Ordering::Equal
    }
}

impl<'a> CellRef<'a> {
    fn rank(&self) -> u8 {
        match self {
            Self::Empty => 0,
            Self::Bool(_) => 1,
            Self::Int(_) | Self::Number(_) => 2,
            Self::Text(_) => 3,
            Self::Custom(_) => 4,
        }
    }

    /// A total order: empty < bool < number < text < custom. Ints and floats compare by
    /// value; NaN sorts after every number. Custom cells compare equal (use a column
    /// comparator for them).
    pub fn total_cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a.cmp(b),
            (Self::Int(a), Self::Number(b)) => (*a as f64).total_cmp(b),
            (Self::Number(a), Self::Int(b)) => a.total_cmp(&(*b as f64)),
            (Self::Number(a), Self::Number(b)) => a.total_cmp(b),
            (Self::Bool(a), Self::Bool(b)) => a.cmp(b),
            (Self::Text(a), Self::Text(b)) => a.cmp(b),
            _ => self.rank().cmp(&other.rank()),
        }
    }

    /// Append the default display text to `out` (no allocation beyond `out`'s growth).
    pub fn write_display(&self, out: &mut String) {
        match self {
            Self::Empty | Self::Custom(_) => {}
            Self::Text(s) => out.push_str(s),
            Self::Int(v) => {
                let _ = write!(out, "{v}");
            }
            Self::Number(v) => {
                if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e15 {
                    let _ = write!(out, "{v:.0}");
                } else {
                    let _ = write!(out, "{v:.2}");
                }
            }
            Self::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        }
    }

    /// Whether numbers should right-align by default.
    pub fn is_numeric(&self) -> bool {
        matches!(self, Self::Int(_) | Self::Number(_))
    }
}

/// Why an edit was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// The source (or column) does not accept edits.
    ReadOnly,
    /// The value did not parse or failed validation.
    Invalid(String),
    /// The source is shared with a background job; retry after it finishes.
    Busy,
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadOnly => f.write_str("read-only"),
            Self::Invalid(m) => write!(f, "invalid: {m}"),
            Self::Busy => f.write_str("busy"),
        }
    }
}

/// Row data for a grid. Implement it over your own storage; the grid never copies rows.
///
/// `Send + Sync` so sorting and filtering can run on the background executor.
pub trait GridSource: Send + Sync + 'static {
    fn row_count(&self) -> usize;
    /// The cell at source `row`, source column `col`. Out-of-range reads return `Empty`.
    fn cell(&self, row: usize, col: usize) -> CellRef<'_>;
    /// Stable identity of `row`; defaults to the index.
    fn row_key(&self, row: usize) -> RowKey {
        RowKey(row as u64)
    }
    /// Bumped by the owner whenever data changes (lets caches and jobs detect staleness).
    fn revision(&self) -> u64 {
        0
    }
    /// Write a cell from edited text. Read-only by default.
    fn set_cell(&mut self, _row: usize, _col: usize, _text: &str) -> Result<(), EditError> {
        Err(EditError::ReadOnly)
    }
}

type Getter<T> = Arc<dyn for<'a> Fn(&'a T) -> CellRef<'a> + Send + Sync>;
type Setter<T> = Arc<dyn Fn(&mut T, &str) -> Result<(), EditError> + Send + Sync>;
type KeyFn<T> = Arc<dyn Fn(&T) -> u64 + Send + Sync>;

/// A `Vec<T>` with one accessor per column. Columns may be editable.
pub struct VecSource<T> {
    rows: Vec<T>,
    getters: Vec<Getter<T>>,
    setters: Vec<Option<Setter<T>>>,
    key: Option<KeyFn<T>>,
    revision: u64,
}

impl<T: Send + Sync + 'static> VecSource<T> {
    pub fn new(rows: Vec<T>) -> Self {
        Self {
            rows,
            getters: Vec::new(),
            setters: Vec::new(),
            key: None,
            revision: 0,
        }
    }

    /// Add a read-only column.
    pub fn column(
        mut self,
        get: impl for<'a> Fn(&'a T) -> CellRef<'a> + Send + Sync + 'static,
    ) -> Self {
        self.getters.push(Arc::new(get));
        self.setters.push(None);
        self
    }

    /// Add an editable column: `set` parses the edited text into the row.
    pub fn editable_column(
        mut self,
        get: impl for<'a> Fn(&'a T) -> CellRef<'a> + Send + Sync + 'static,
        set: impl Fn(&mut T, &str) -> Result<(), EditError> + Send + Sync + 'static,
    ) -> Self {
        self.getters.push(Arc::new(get));
        self.setters.push(Some(Arc::new(set)));
        self
    }

    /// Derive row keys from the data instead of the index.
    pub fn key(mut self, key: impl Fn(&T) -> u64 + Send + Sync + 'static) -> Self {
        self.key = Some(Arc::new(key));
        self
    }

    pub fn rows(&self) -> &[T] {
        &self.rows
    }

    /// Mutable access; bumps the revision.
    pub fn rows_mut(&mut self) -> &mut Vec<T> {
        self.revision += 1;
        &mut self.rows
    }
}

impl<T: Send + Sync + 'static> GridSource for VecSource<T> {
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, col: usize) -> CellRef<'_> {
        match (self.rows.get(row), self.getters.get(col)) {
            (Some(r), Some(g)) => g(r),
            _ => CellRef::Empty,
        }
    }
    fn row_key(&self, row: usize) -> RowKey {
        match (&self.key, self.rows.get(row)) {
            (Some(k), Some(r)) => RowKey(k(r)),
            _ => RowKey(row as u64),
        }
    }
    fn revision(&self) -> u64 {
        self.revision
    }
    fn set_cell(&mut self, row: usize, col: usize, text: &str) -> Result<(), EditError> {
        let set = self
            .setters
            .get(col)
            .and_then(Option::as_ref)
            .ok_or(EditError::ReadOnly)?
            .clone();
        let r = self
            .rows
            .get_mut(row)
            .ok_or_else(|| EditError::Invalid("row out of range".into()))?;
        set(r, text)?;
        self.revision += 1;
        Ok(())
    }
}

/// An immutable shared slice (`Arc<[T]>`) plus column accessors. Cloning the source or
/// handing it to a background sort is a pointer copy.
pub struct ArcSource<T> {
    rows: Arc<[T]>,
    getters: Vec<Getter<T>>,
    key: Option<KeyFn<T>>,
}

impl<T: Send + Sync + 'static> ArcSource<T> {
    pub fn new(rows: Arc<[T]>) -> Self {
        Self {
            rows,
            getters: Vec::new(),
            key: None,
        }
    }
    pub fn column(
        mut self,
        get: impl for<'a> Fn(&'a T) -> CellRef<'a> + Send + Sync + 'static,
    ) -> Self {
        self.getters.push(Arc::new(get));
        self
    }
    pub fn key(mut self, key: impl Fn(&T) -> u64 + Send + Sync + 'static) -> Self {
        self.key = Some(Arc::new(key));
        self
    }
    pub fn rows(&self) -> &Arc<[T]> {
        &self.rows
    }
}

impl<T: Send + Sync + 'static> GridSource for ArcSource<T> {
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, col: usize) -> CellRef<'_> {
        match (self.rows.get(row), self.getters.get(col)) {
            (Some(r), Some(g)) => g(r),
            _ => CellRef::Empty,
        }
    }
    fn row_key(&self, row: usize) -> RowKey {
        match (&self.key, self.rows.get(row)) {
            (Some(k), Some(r)) => RowKey(k(r)),
            _ => RowKey(row as u64),
        }
    }
}

/// A source whose cells are computed on demand from `(row, col)`; nothing is stored.
/// Useful for huge synthetic data, and as an adapter over remote/paged storage.
pub struct FnSource<F> {
    rows: usize,
    cell: F,
}

impl<F> FnSource<F>
where
    F: Fn(usize, usize) -> CellRef<'static> + Send + Sync + 'static,
{
    pub fn new(rows: usize, cell: F) -> Self {
        Self { rows, cell }
    }
}

impl<F> GridSource for FnSource<F>
where
    F: Fn(usize, usize) -> CellRef<'static> + Send + Sync + 'static,
{
    fn row_count(&self) -> usize {
        self.rows
    }
    fn cell(&self, row: usize, col: usize) -> CellRef<'_> {
        if row < self.rows {
            (self.cell)(row, col)
        } else {
            CellRef::Empty
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct P {
        name: String,
        age: i64,
    }

    #[test]
    fn vec_source_borrows_and_edits() {
        let mut s = VecSource::new(vec![
            P {
                name: "Ada".into(),
                age: 36,
            },
            P {
                name: "Linus".into(),
                age: 54,
            },
        ])
        .column(|p: &P| CellRef::Text(&p.name))
        .editable_column(
            |p: &P| CellRef::Int(p.age),
            |p, t| {
                p.age = t
                    .trim()
                    .parse()
                    .map_err(|_| EditError::Invalid(t.to_string()))?;
                Ok(())
            },
        );
        assert_eq!(s.cell(1, 0), CellRef::Text("Linus"));
        assert_eq!(s.cell(9, 0), CellRef::Empty);
        assert_eq!(s.set_cell(0, 0, "x"), Err(EditError::ReadOnly));
        assert!(matches!(
            s.set_cell(0, 1, "abc"),
            Err(EditError::Invalid(_))
        ));
        assert_eq!(s.set_cell(0, 1, " 37"), Ok(()));
        assert_eq!(s.cell(0, 1), CellRef::Int(37));
        assert_eq!(s.revision(), 1);
    }

    #[test]
    fn ordering_is_total() {
        let cells = [
            CellRef::Text("b"),
            CellRef::Number(f64::NAN),
            CellRef::Int(3),
            CellRef::Number(2.5),
            CellRef::Empty,
            CellRef::Bool(true),
        ];
        let mut v = cells.to_vec();
        v.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(v[0], CellRef::Empty);
        assert_eq!(v[1], CellRef::Bool(true));
        assert_eq!(v[2], CellRef::Number(2.5));
        assert_eq!(v[3], CellRef::Int(3));
        assert_eq!(v[5], CellRef::Text("b"));
    }

    #[test]
    fn display() {
        let mut s = String::new();
        CellRef::Number(3.0).write_display(&mut s);
        s.push('|');
        CellRef::Number(1.23456).write_display(&mut s);
        s.push('|');
        CellRef::Int(-4).write_display(&mut s);
        assert_eq!(s, "3|1.23|-4");
    }
}
