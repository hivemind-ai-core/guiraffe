use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

use crate::{CellPos, Formula, FormulaEngine, FormulaError, FormulaValues, Reference};

type Input = (CellPos, Arc<str>);
type Value = Result<f64, FormulaError>;
type Values = im::OrdMap<CellPos, Value>;
type Compiled = Result<Box<dyn Formula>, FormulaError>;

/// Immutable formula values from a completed revision.
pub struct Calculation {
    pub revision: u64,
    values: Values,
}

impl Calculation {
    pub fn value(&self, pos: CellPos) -> Option<Result<f64, FormulaError>> {
        self.values.get(&pos).copied()
    }
}

/// A single spreadsheet's asynchronous calculation session.
///
/// Submit only input changes, in edit order. Each new batch cancels superseded calculation
/// and the next publication covers every pending change. Dropping the session cancels it.
pub struct Recalculator {
    input: Sender<(u64, Vec<Input>)>,
    output: Receiver<Calculation>,
    revision: Arc<AtomicU64>,
}

impl Recalculator {
    pub fn new(rows: usize, cols: usize, engine: Arc<dyn FormulaEngine>) -> std::io::Result<Self> {
        let (input, incoming) = mpsc::channel::<(u64, Vec<Input>)>();
        let (outgoing, output) = mpsc::channel();
        let revision = Arc::new(AtomicU64::new(0));
        let latest = revision.clone();
        thread::Builder::new()
            .name("sheet-recalc".into())
            .spawn(move || {
                let mut graph = Graph::new(rows, cols);
                while let Ok((mut version, changes)) = incoming.recv() {
                    if latest.load(Ordering::Acquire) == u64::MAX {
                        return;
                    }
                    graph.update(changes, engine.as_ref());
                    for (pending, changes) in incoming.try_iter() {
                        version = pending;
                        graph.update(changes, engine.as_ref());
                    }
                    if graph
                        .calculate(|| latest.load(Ordering::Acquire) != version)
                        .is_some()
                        && outgoing
                            .send(Calculation {
                                revision: version,
                                values: graph.values.clone(),
                            })
                            .is_err()
                    {
                        return;
                    }
                }
            })?;
        Ok(Self {
            input,
            output,
            revision,
        })
    }

    pub fn submit(&mut self, changes: Vec<Input>) -> Option<u64> {
        let version = self.revision.fetch_add(1, Ordering::AcqRel) + 1;
        self.input.send((version, changes)).ok()?;
        Some(version)
    }

    pub fn try_recv(&self) -> Option<Calculation> {
        self.output.try_recv().ok()
    }

    pub fn recv(&self) -> Option<Calculation> {
        self.output.recv().ok()
    }
}

impl Drop for Recalculator {
    fn drop(&mut self) {
        self.revision.store(u64::MAX, Ordering::Release);
    }
}

struct Graph {
    rows: usize,
    cols: usize,
    numbers: BTreeMap<CellPos, f64>,
    formulas: BTreeMap<CellPos, Compiled>,
    values: Values,
    reverse: HashMap<CellPos, HashSet<CellPos>>,
    ranges: HashMap<CellPos, Vec<Reference>>,
    dirty: HashSet<CellPos>,
}

impl Graph {
    fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            numbers: BTreeMap::new(),
            formulas: BTreeMap::new(),
            values: Values::new(),
            reverse: HashMap::new(),
            ranges: HashMap::new(),
            dirty: HashSet::new(),
        }
    }

    fn update(&mut self, changes: Vec<Input>, engine: &dyn FormulaEngine) {
        for (pos, text) in changes {
            if let Some(Ok(previous)) = self.formulas.remove(&pos) {
                for r in previous.references().iter().filter(|r| r.first == r.last) {
                    if let Some(dependents) = self.reverse.get_mut(&r.first) {
                        dependents.remove(&pos);
                        if dependents.is_empty() {
                            self.reverse.remove(&r.first);
                        }
                    }
                }
            }
            self.ranges.remove(&pos);
            self.numbers.remove(&pos);
            if let Some(expression) = text.strip_prefix('=') {
                let formula = engine.compile(expression).and_then(|formula| {
                    let valid = formula.references().iter().all(|r| {
                        r.first.row <= r.last.row
                            && r.first.col <= r.last.col
                            && r.last.row < self.rows
                            && r.last.col < self.cols
                    });
                    valid.then_some(formula).ok_or(FormulaError::Reference)
                });
                if let Ok(formula) = &formula {
                    for r in formula.references() {
                        if r.first == r.last {
                            self.reverse.entry(r.first).or_default().insert(pos);
                        } else {
                            self.ranges.entry(pos).or_default().push(*r);
                        }
                    }
                }
                self.formulas.insert(pos, formula);
            } else if let Ok(number) = text.trim().parse::<f64>()
                && number.is_finite()
            {
                self.numbers.insert(pos, number);
            }
            self.dirty.insert(pos);
        }
    }

    fn dependents(&self, pos: CellPos) -> impl Iterator<Item = CellPos> + '_ {
        let direct = self.reverse.get(&pos);
        direct
            .into_iter()
            .flatten()
            .copied()
            .chain(self.ranges.iter().filter_map(move |(&dependent, ranges)| {
                (direct.is_none_or(|direct| !direct.contains(&dependent))
                    && ranges.iter().any(|r| r.contains(pos)))
                .then_some(dependent)
            }))
    }

    fn calculate(&mut self, cancelled: impl Fn() -> bool) -> Option<()> {
        let mut dirty = self.dirty.clone();
        let mut queue: VecDeque<_> = dirty.iter().copied().collect();
        while let Some(pos) = queue.pop_front() {
            if cancelled() {
                return None;
            }
            for dependent in self.dependents(pos) {
                if dirty.insert(dependent) {
                    queue.push_back(dependent);
                }
            }
        }

        let mut pending = HashMap::new();
        let mut changes = HashMap::new();
        for &pos in &dirty {
            if cancelled() {
                return None;
            }
            let Some(formula) = self.formulas.get(&pos) else {
                changes.insert(pos, None);
                continue;
            };
            let mut dependencies = HashSet::new();
            if let Ok(formula) = formula {
                for r in formula.references() {
                    for (&input, _) in self.formulas.range(r.first..=r.last) {
                        if cancelled() {
                            return None;
                        }
                        if r.contains(input) && dirty.contains(&input) {
                            dependencies.insert(input);
                        }
                    }
                }
            }
            if dependencies.is_empty() {
                queue.push_back(pos);
            }
            pending.insert(pos, dependencies.len());
        }
        while let Some(pos) = queue.pop_front() {
            if cancelled() {
                return None;
            }
            let formula = self.formulas.get(&pos)?;
            let reader = Reader {
                graph: self,
                changes: &changes,
                cancelled: &cancelled,
            };
            let value = match formula {
                Ok(formula) => formula.evaluate(&reader),
                Err(error) => Err(*error),
            };
            changes.insert(pos, Some(value));
            pending.remove(&pos);
            for successor in self.dependents(pos) {
                if let Some(count) = pending.get_mut(&successor) {
                    *count -= 1;
                    if *count == 0 {
                        queue.push_back(successor);
                    }
                }
            }
        }
        if cancelled() {
            return None;
        }
        let mut values = self.values.clone();
        for (pos, value) in changes.into_iter().chain(
            pending
                .into_keys()
                .map(|pos| (pos, Some(Err(FormulaError::Cycle)))),
        ) {
            if cancelled() {
                return None;
            }
            if let Some(value) = value {
                values.insert(pos, value);
            } else {
                values.remove(&pos);
            }
        }
        self.values = values;
        self.dirty.clear();
        Some(())
    }
}

struct Reader<'a> {
    graph: &'a Graph,
    changes: &'a HashMap<CellPos, Option<Value>>,
    cancelled: &'a dyn Fn() -> bool,
}

impl FormulaValues for Reader<'_> {
    fn number(&self, pos: CellPos) -> Result<Option<f64>, FormulaError> {
        if (self.cancelled)() {
            return Err(FormulaError::Cancelled);
        }
        if pos.row >= self.graph.rows || pos.col >= self.graph.cols {
            return Err(FormulaError::Reference);
        }
        let formula = self
            .changes
            .get(&pos)
            .copied()
            .unwrap_or_else(|| self.graph.values.get(&pos).copied());
        if let Some(value) = formula {
            value.map(Some)
        } else {
            Ok(self.graph.numbers.get(&pos).copied())
        }
    }

    fn visit(&self, r: Reference, visitor: &mut dyn FnMut(f64)) -> Result<(), FormulaError> {
        if r.first.row > r.last.row
            || r.first.col > r.last.col
            || r.last.row >= self.graph.rows
            || r.last.col >= self.graph.cols
        {
            return Err(FormulaError::Reference);
        }
        for pos in self
            .graph
            .numbers
            .range(r.first..=r.last)
            .map(|(&pos, _)| pos)
            .chain(
                self.graph
                    .formulas
                    .range(r.first..=r.last)
                    .map(|(&pos, _)| pos),
            )
        {
            if (self.cancelled)() {
                return Err(FormulaError::Cancelled);
            }
            if r.contains(pos)
                && let Some(value) = self.number(pos)?
            {
                visitor(value);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuiltinEngine, a1};
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn incremental_chain_matches_naive(seed in -100i32..100, increments in proptest::collection::vec(-10i32..10, 1..50)) {
            let mut graph = Graph::new(increments.len() + 1, 1);
            let engine = BuiltinEngine;
            let mut batch = vec![(CellPos::default(), Arc::from(seed.to_string()))];
            for (index, increment) in increments.iter().enumerate() {
                batch.push((CellPos::new(index + 1, 0), Arc::from(format!("={}+({increment})", a1(CellPos::new(index, 0))))));
            }
            graph.update(batch, &engine);
            assert!(graph.calculate(|| false).is_some());
            for seed in [seed, seed + 1] {
                graph.update(vec![(CellPos::default(), Arc::from(seed.to_string()))], &engine);
                assert!(graph.calculate(|| true).is_none());
                assert!(graph.calculate(|| false).is_some());
                let mut expected = seed;
                for (index, change) in increments.iter().enumerate() {
                    expected += change;
                    prop_assert_eq!(graph.values.get(&CellPos::new(index + 1, 0)), Some(&Ok(f64::from(expected))));
                }
            }
        }

        #[test]
        fn parser_handles_arbitrary_input(input in any::<String>()) {
            let _ = BuiltinEngine.compile(&input);
        }
    }

    #[test]
    fn arithmetic_aggregates_and_conditional_use_latest_inputs() {
        let a = CellPos::new(0, 0);
        let b = CellPos::new(1, 0);
        let c = CellPos::new(2, 0);
        let result = CellPos::new(0, 1);
        let mut graph = Graph::new(3, 2);
        let engine = BuiltinEngine;
        graph.update(
            vec![
                (a, Arc::from("1")),
                (b, Arc::from("2")),
                (c, Arc::from("=A2+1")),
                (
                    result,
                    Arc::from(
                        "=IF(COUNT(A1:A3)=3,SUM(A1:A3)+AVG(A1:A3)-MIN(A1:A3)+MAX(A1:A3),1/0)",
                    ),
                ),
            ],
            &engine,
        );
        assert!(graph.calculate(|| false).is_some());
        assert_eq!(graph.values.get(&result), Some(&Ok(10.)));
        graph.update(vec![(b, Arc::from("5"))], &engine);
        assert!(graph.calculate(|| false).is_some());
        assert_eq!(graph.values.get(&result), Some(&Ok(21.)));
    }

    #[test]
    fn cycles_break_and_ranges_update() {
        let a = CellPos::new(0, 0);
        let b = CellPos::new(1, 0);
        let c = CellPos::new(2, 0);
        let mut graph = Graph::new(3, 1);
        let engine = BuiltinEngine;
        graph.update(
            vec![
                (a, Arc::from("=A2")),
                (b, Arc::from("=A1")),
                (c, Arc::from("=SUM(A1:A2)")),
            ],
            &engine,
        );
        assert!(graph.calculate(|| false).is_some());
        assert_eq!(graph.values.get(&c), Some(&Err(FormulaError::Cycle)));
        graph.update(vec![(a, Arc::from("3"))], &engine);
        assert!(graph.calculate(|| false).is_some());
        assert_eq!(graph.values.get(&c), Some(&Ok(6.)));
    }

    #[test]
    fn asynchronous_chain_of_one_hundred_thousand_formulas() -> std::io::Result<()> {
        let mut worker = Recalculator::new(1_000_000, 1000, Arc::new(BuiltinEngine))?;
        let entries = (1..=100_000)
            .map(|row| (CellPos::new(row, 0), Arc::from(format!("=A{row}+1"))))
            .collect();
        let initial = worker.submit(entries);
        let latest = worker.submit(vec![(CellPos::default(), Arc::from("7"))]);
        assert_ne!(initial, latest);
        while let Some(result) = worker.recv() {
            if Some(result.revision) == latest {
                assert!(result.value(CellPos::new(100_000, 0)) == Some(Ok(100_007.)));
                return Ok(());
            }
        }
        Err(std::io::Error::other("worker exited before latest result"))
    }
}
