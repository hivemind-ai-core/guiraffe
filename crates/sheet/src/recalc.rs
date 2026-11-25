use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
};

use arc_swap::ArcSwapOption;

use crate::{CellPos, Formula, FormulaEngine, FormulaError, FormulaValues, Reference};

type Input = (CellPos, Arc<str>);
type Value = Result<f64, FormulaError>;
type Values = imbl::OrdMap<CellPos, Value>;
type Compiled = Result<Box<dyn Formula>, FormulaError>;

/// Immutable formula values from a completed revision.
#[derive(Clone)]
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
    input: SyncSender<()>,
    pending: Arc<ArcSwapOption<Pending>>,
    output: Receiver<()>,
    published: Arc<ArcSwapOption<Calculation>>,
    revision: Arc<AtomicU64>,
}

struct Pending {
    revision: u64,
    // Move the first batch into the slot as-is. Later submits only index their own
    // edits, and the worker skips replaced base inputs when applying the mailbox.
    base: Arc<Vec<Input>>,
    overlay: imbl::OrdMap<CellPos, Arc<str>>,
}

impl Recalculator {
    pub fn new(rows: usize, cols: usize, engine: Arc<dyn FormulaEngine>) -> std::io::Result<Self> {
        // Channels carry wakeups only. Each slot owns the latest data independently of
        // whether a wakeup is already queued.
        let (input, incoming) = mpsc::sync_channel(1);
        let (outgoing, output) = mpsc::sync_channel(1);
        let pending = Arc::new(ArcSwapOption::<Pending>::empty());
        let published = Arc::new(ArcSwapOption::empty());
        let revision = Arc::new(AtomicU64::new(0));
        let latest = revision.clone();
        let work = pending.clone();
        let results = published.clone();
        thread::Builder::new()
            .name("sheet-recalc".into())
            .spawn(move || {
                let mut graph = Graph::new(rows, cols);
                while incoming.recv().is_ok() {
                    if latest.load(Ordering::Acquire) == u64::MAX {
                        return;
                    }
                    let Some(batch) = work.swap(None) else {
                        continue;
                    };
                    let version = batch.revision;
                    graph.update(
                        batch
                            .base
                            .iter()
                            .filter(|(pos, _)| !batch.overlay.contains_key(pos))
                            .cloned()
                            .chain(batch.overlay.iter().map(|(&pos, text)| (pos, text.clone()))),
                        engine.as_ref(),
                    );
                    drop(batch);
                    if graph
                        .calculate(|| latest.load(Ordering::Acquire) != version)
                        .is_some()
                        && latest.load(Ordering::Acquire) == version
                    {
                        results.store(Some(Arc::new(Calculation {
                            revision: version,
                            values: graph.values.clone(),
                        })));
                        if outgoing.try_send(()) == Err(TrySendError::Disconnected(())) {
                            return;
                        }
                    }
                }
            })?;
        Ok(Self {
            input,
            pending,
            output,
            published,
            revision,
        })
    }

    pub fn submit(&mut self, changes: Vec<Input>) -> Option<u64> {
        let revision = self.revision.fetch_add(1, Ordering::AcqRel) + 1;
        let submitted = Arc::new(changes);
        // Retry only if the worker took the previous slot concurrently. The initial
        // submission moves its Vec; later ones copy only changed map branches.
        self.pending.rcu(|current| {
            let (base, overlay) = match current {
                Some(batch) => {
                    let mut overlay = batch.overlay.clone();
                    overlay.extend(submitted.iter().cloned());
                    (batch.base.clone(), overlay)
                }
                None => (submitted.clone(), imbl::OrdMap::new()),
            };
            Some(Arc::new(Pending {
                revision,
                base,
                overlay,
            }))
        });
        match self.input.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) => Some(revision),
            Err(TrySendError::Disconnected(())) => None,
        }
    }

    pub fn try_recv(&self) -> Option<Calculation> {
        let _ = self.output.try_recv();
        self.take_publication()
    }

    pub fn recv(&self) -> Option<Calculation> {
        loop {
            if let Some(result) = self.try_recv() {
                return Some(result);
            }
            self.output.recv().ok()?;
        }
    }

    fn take_publication(&self) -> Option<Calculation> {
        let result = self.published.swap(None)?;
        Some(Arc::unwrap_or_clone(result))
    }

    #[cfg(test)]
    fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<Calculation, mpsc::RecvTimeoutError> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(result) = self.try_recv() {
                return Ok(result);
            }
            self.output
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))?;
        }
    }
}

impl Drop for Recalculator {
    fn drop(&mut self) {
        self.revision.store(u64::MAX, Ordering::Release);
        self.pending.store(None);
        self.published.store(None);
    }
}

struct Graph {
    rows: usize,
    cols: usize,
    numbers: BTreeMap<CellPos, f64>,
    formulas: BTreeMap<CellPos, Compiled>,
    values: Values,
    reverse: BTreeSet<(CellPos, CellPos)>,
    ranges: HashMap<CellPos, Vec<Reference>>,
    dirty: BTreeSet<CellPos>,
}

impl Graph {
    fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            numbers: BTreeMap::new(),
            formulas: BTreeMap::new(),
            values: Values::new(),
            reverse: BTreeSet::new(),
            ranges: HashMap::new(),
            dirty: BTreeSet::new(),
        }
    }

    fn update(&mut self, changes: impl IntoIterator<Item = Input>, engine: &dyn FormulaEngine) {
        for (pos, text) in changes {
            if let Some(Ok(previous)) = self.formulas.remove(&pos) {
                for r in previous.references().iter().filter(|r| r.first == r.last) {
                    self.reverse.remove(&(r.first, pos));
                }
            }
            self.ranges.remove(&pos);
            self.numbers.remove(&pos);
            if let Some(expression) = text.strip_prefix('=') {
                let formula = engine_call(|| engine.compile(expression)).and_then(|formula| {
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
                            self.reverse.insert((r.first, pos));
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
        self.reverse
            .range((pos, CellPos::default())..=(pos, CellPos::new(usize::MAX, usize::MAX)))
            .map(|&(_, dependent)| dependent)
            .chain(self.ranges.iter().filter_map(move |(&dependent, ranges)| {
                (!self.reverse.contains(&(pos, dependent))
                    && ranges.iter().any(|r| r.contains(pos)))
                .then_some(dependent)
            }))
    }

    fn evaluate(&self, formula: &Compiled, values: &Values, cancelled: &dyn Fn() -> bool) -> Value {
        let reader = Reader {
            graph: self,
            values,
            cancelled,
        };
        match formula {
            Ok(formula) => engine_call(|| formula.evaluate(&reader)),
            Err(error) => Err(*error),
        }
    }

    fn calculate(&mut self, cancelled: impl Fn() -> bool) -> Option<()> {
        let mut dirty = std::mem::take(&mut self.dirty);
        let result = self.calculate_dirty(&mut dirty, cancelled);
        if result.is_some() {
            dirty.clear();
        }
        // On cancellation the next revision also covers every invalidated dependent.
        self.dirty = dirty;
        result
    }

    fn calculate_dirty(
        &mut self,
        dirty: &mut BTreeSet<CellPos>,
        cancelled: impl Fn() -> bool,
    ) -> Option<()> {
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

        // An address-ordered pass is already topological if every reference
        // precedes its formula. Other sheets use the general cycle-aware traversal.
        if dirty.iter().all(|pos| {
            !cancelled()
                && self.formulas.get(pos).is_none_or(|formula| {
                    formula.as_ref().map_or(true, |formula| {
                        formula.references().iter().all(|r| r.last < *pos)
                    })
                })
        }) {
            drop(queue);
            let mut values = self.values.clone();
            for &pos in dirty.iter() {
                if cancelled() {
                    return None;
                }
                if let Some(formula) = self.formulas.get(&pos) {
                    let value = self.evaluate(formula, &values, &cancelled);
                    values.insert(pos, value);
                } else {
                    values.remove(&pos);
                }
            }
            self.values = values;
            return Some(());
        }

        let mut pending = HashMap::new();
        let mut values = self.values.clone();
        for &pos in dirty.iter() {
            if cancelled() {
                return None;
            }
            let Some(formula) = self.formulas.get(&pos) else {
                values.remove(&pos);
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
            let value = self.evaluate(formula, &values, &cancelled);
            values.insert(pos, value);
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
        for pos in pending.into_keys() {
            if cancelled() {
                return None;
            }
            values.insert(pos, Err(FormulaError::Cycle));
        }
        self.values = values;
        Some(())
    }
}

fn engine_call<T>(evaluate: impl FnOnce() -> Result<T, FormulaError>) -> Result<T, FormulaError> {
    catch_unwind(AssertUnwindSafe(evaluate)).unwrap_or(Err(FormulaError::Value))
}

struct Reader<'a> {
    graph: &'a Graph,
    values: &'a Values,
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
        let formula = self.values.get(&pos).copied();
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
    use std::sync::mpsc::Sender;

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
    fn a_running_formula_is_superseded_off_thread() -> Result<(), Box<dyn std::error::Error>> {
        use std::{sync::Mutex, time::Duration};

        struct PausingEngine {
            entered: Sender<thread::ThreadId>,
            release: Arc<Mutex<Receiver<()>>>,
        }

        struct PausingFormula {
            entered: Sender<thread::ThreadId>,
            release: Arc<Mutex<Receiver<()>>>,
        }

        impl FormulaEngine for PausingEngine {
            fn compile(&self, expression: &str) -> Result<Box<dyn Formula>, FormulaError> {
                if expression != "WAIT" {
                    return BuiltinEngine.compile(expression);
                }
                Ok(Box::new(PausingFormula {
                    entered: self.entered.clone(),
                    release: self.release.clone(),
                }))
            }
        }

        impl Formula for PausingFormula {
            fn references(&self) -> &[Reference] {
                &[]
            }

            fn evaluate(&self, _: &dyn FormulaValues) -> Result<f64, FormulaError> {
                self.entered
                    .send(thread::current().id())
                    .map_err(|_| FormulaError::Cancelled)?;
                self.release
                    .lock()
                    .map_err(|_| FormulaError::Cancelled)?
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|_| FormulaError::Cancelled)?;
                Ok(1.)
            }
        }

        let timeout = Duration::from_secs(5);
        let (entered, wait_for_enter) = mpsc::channel();
        let (release, wait_for_release) = mpsc::channel();
        let engine = Arc::new(PausingEngine {
            entered,
            release: Arc::new(Mutex::new(wait_for_release)),
        });
        let mut worker = Recalculator::new(1, 2, engine)?;
        let pos = CellPos::default();
        let first = worker.submit(vec![(pos, Arc::from("=WAIT"))]);
        assert!(first.is_some());
        assert_ne!(
            wait_for_enter.recv_timeout(timeout)?,
            thread::current().id()
        );
        let independent = CellPos::new(0, 1);
        worker.submit(vec![(independent, Arc::from("7"))]);
        let payload: Arc<str> = Arc::from("=99");
        for _ in 0..10_000 {
            worker.submit(vec![(pos, payload.clone())]);
        }
        let latest = worker.submit(vec![(pos, Arc::from("=B1+100"))]);
        assert_eq!(Arc::strong_count(&payload), 1);

        release.send(())?;
        let result = worker.recv_timeout(timeout)?;
        assert_eq!(Some(result.revision), latest);
        assert_eq!(result.value(pos), Some(Ok(107.)));
        Ok(())
    }

    #[test]
    fn unread_calculation_churn_retains_latest_publication_only()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::time::Duration;
        let mut worker = Recalculator::new(1, 2, Arc::new(BuiltinEngine))?;
        let input = CellPos::default();
        let formula = CellPos::new(0, 1);
        worker.submit(vec![(formula, Arc::from("=A1+1"))]);
        worker.output.recv_timeout(Duration::from_secs(5))?;
        let mut latest = None;
        for value in 1..=1_000 {
            latest = worker.submit(vec![(input, Arc::from(value.to_string()))]);
            // Read only the wakeup, leaving the publication slot unconsumed.
            worker.output.recv_timeout(Duration::from_secs(5))?;
        }
        let result = worker.try_recv().ok_or("latest result is missing")?;
        assert_eq!(Some(result.revision), latest);
        assert_eq!(result.value(formula), Some(Ok(1_001.)));
        assert!(worker.try_recv().is_none());
        Ok(())
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
        while let Ok(result) = worker.recv_timeout(std::time::Duration::from_secs(10)) {
            if Some(result.revision) == latest {
                assert!(result.value(CellPos::new(100_000, 0)) == Some(Ok(100_007.)));
                return Ok(());
            }
        }
        Err(std::io::Error::other("worker exited before latest result"))
    }
}
