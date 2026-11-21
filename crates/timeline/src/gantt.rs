//! Gantt scheduling (critical path method): tasks with durations and finish→start
//! dependencies; computes earliest/latest times, slack and the critical path.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub id: u32,
    pub name: String,
    /// Earliest permitted start (a constraint; e.g. "not before Monday").
    pub not_before: f64,
    pub duration: f64,
    /// Ids of tasks that must finish before this one starts.
    pub after: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scheduled {
    pub early_start: f64,
    pub early_finish: f64,
    pub late_start: f64,
    pub late_finish: f64,
}

impl Scheduled {
    pub fn slack(&self) -> f64 {
        self.late_start - self.early_start
    }
    pub fn critical(&self) -> bool {
        self.slack().abs() < 1e-9
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScheduleError {
    UnknownDependency { task: u32, missing: u32 },
    Cycle(Vec<u32>),
    DuplicateId(u32),
}

/// Two contiguous arrays replace a heap allocation per nonempty adjacency list.
#[derive(Default)]
pub(crate) struct AdjacentRows {
    offsets: Vec<usize>,
    rows: Vec<usize>,
}

impl AdjacentRows {
    fn new(count: usize, edges: impl Iterator<Item = (usize, usize)> + Clone) -> Self {
        let mut offsets = vec![0; count + 1];
        for (from, _) in edges.clone() {
            offsets[from + 1] += 1;
        }
        for row in 0..count {
            offsets[row + 1] += offsets[row];
        }
        let mut rows = vec![0; offsets[count]];
        // Use each start offset as a write cursor and then restore the starts.
        for (from, to) in edges {
            rows[offsets[from]] = to;
            offsets[from] += 1;
        }
        offsets.rotate_right(1);
        offsets[0] = 0;
        Self { offsets, rows }
    }

    pub(crate) fn get(&self, row: usize) -> &[usize] {
        &self.rows[self.offsets[row]..self.offsets[row + 1]]
    }
}

/// Schedule `tasks`; result keyed by id. Negative/NaN durations count as 0.
pub fn schedule(tasks: &[Task]) -> Result<HashMap<u32, Scheduled>, ScheduleError> {
    let index = task_index(tasks);
    SchedulePlan::new(tasks, &index).map(|plan| plan.schedule(tasks))
}

pub(crate) fn task_index(tasks: &[Task]) -> HashMap<u32, usize> {
    tasks.iter().enumerate().map(|(i, t)| (t.id, i)).collect()
}

/// Graph structure and topological order are independent of task timing edits.
pub(crate) struct SchedulePlan {
    pub(crate) predecessors: AdjacentRows,
    pub(crate) successors: AdjacentRows,
    order: Vec<usize>,
}

impl SchedulePlan {
    pub(crate) fn new(tasks: &[Task], index: &HashMap<u32, usize>) -> Result<Self, ScheduleError> {
        if index.len() != tasks.len() {
            let mut seen = std::collections::HashSet::new();
            for task in tasks {
                if !seen.insert(task.id) {
                    return Err(ScheduleError::DuplicateId(task.id));
                }
            }
        }
        let mut offsets = Vec::with_capacity(tasks.len() + 1);
        let mut rows = Vec::with_capacity(tasks.iter().map(|task| task.after.len()).sum());
        offsets.push(0);
        for task in tasks {
            for id in &task.after {
                let Some(&row) = index.get(id) else {
                    return Err(ScheduleError::UnknownDependency {
                        task: task.id,
                        missing: *id,
                    });
                };
                rows.push(row);
            }
            offsets.push(rows.len());
        }
        let predecessors = AdjacentRows { offsets, rows };
        let by_target = &predecessors;
        let edges =
            (0..tasks.len()).flat_map(|to| by_target.get(to).iter().map(move |&from| (from, to)));
        let successors = AdjacentRows::new(tasks.len(), edges);

        let mut indeg: Vec<_> = tasks.iter().map(|task| task.after.len()).collect();
        let mut order = Vec::with_capacity(tasks.len());
        order.extend((0..tasks.len()).filter(|i| indeg[*i] == 0));
        let mut head = 0;
        while head < order.len() {
            let u = order[head];
            head += 1;
            for &v in successors.get(u) {
                indeg[v] -= 1;
                if indeg[v] == 0 {
                    order.push(v);
                }
            }
        }
        if order.len() != tasks.len() {
            let mut stuck: Vec<_> = tasks
                .iter()
                .zip(indeg)
                .filter_map(|(task, degree)| (degree > 0).then_some(task.id))
                .collect();
            stuck.sort_unstable();
            return Err(ScheduleError::Cycle(stuck));
        }
        Ok(Self {
            predecessors,
            successors,
            order,
        })
    }

    pub(crate) fn schedule(&self, tasks: &[Task]) -> HashMap<u32, Scheduled> {
        let dur = |t: &Task| {
            if t.duration.is_finite() {
                t.duration.max(0.0)
            } else {
                0.0
            }
        };
        let mut es = vec![0.0f64; tasks.len()];
        let mut ef = vec![0.0f64; tasks.len()];
        for &i in &self.order {
            let t = &tasks[i];
            let start = self.predecessors.get(i).iter().map(|&d| ef[d]).fold(
                if t.not_before.is_finite() {
                    t.not_before
                } else {
                    0.0
                },
                f64::max,
            );
            es[i] = start;
            ef[i] = start + dur(t);
        }
        let project_end = ef
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max)
            .max(0.0);
        let mut lf = vec![project_end; tasks.len()];
        let mut ls = vec![project_end; tasks.len()];
        for &i in self.order.iter().rev() {
            let finish = self
                .successors
                .get(i)
                .iter()
                .map(|&s| ls[s])
                .fold(project_end, f64::min);
            lf[i] = finish;
            ls[i] = finish - dur(&tasks[i]);
        }
        tasks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                (
                    t.id,
                    Scheduled {
                        early_start: es[i],
                        early_finish: ef[i],
                        late_start: ls[i],
                        late_finish: lf[i],
                    },
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacency_indexes_preserve_direction_order_and_duplicate_edges() {
        let edges = [(0, 4), (3, 2), (0, 4), (0, 2)];
        let outgoing = AdjacentRows::new(5, edges.into_iter());
        let incoming = AdjacentRows::new(5, edges.into_iter().map(|(a, b)| (b, a)));
        assert_eq!(outgoing.get(0), &[4, 4, 2]);
        assert!(outgoing.get(1).is_empty());
        assert_eq!(incoming.get(2), &[3, 0]);
        assert_eq!(incoming.get(4), &[0, 0]);
    }

    #[test]
    fn timing_edits_reuse_the_same_plan() {
        let mut tasks = [t(1, 2.0, &[]), t(2, 3.0, &[1]), t(3, 1.0, &[1, 2])];
        let plan = SchedulePlan::new(&tasks, &task_index(&tasks)).expect("acyclic graph");
        tasks[0].not_before = 4.0;
        tasks[1].duration = 8.0;
        let edited = plan.schedule(&tasks);
        assert_eq!(edited[&3].early_finish, 15.0);
        assert_eq!(Some(edited), schedule(&tasks).ok());
    }

    fn t(id: u32, d: f64, after: &[u32]) -> Task {
        Task {
            id,
            name: format!("t{id}"),
            not_before: 0.0,
            duration: d,
            after: after.to_vec(),
        }
    }

    #[test]
    fn classic_critical_path() {
        // 1(3) → 2(2) → 4(4); 1 → 3(1) → 4. Critical: 1,2,4 (length 9); 3 has slack 1.
        let s = schedule(&[
            t(1, 3.0, &[]),
            t(2, 2.0, &[1]),
            t(3, 1.0, &[1]),
            t(4, 4.0, &[2, 3]),
        ])
        .unwrap_or_default();
        assert_eq!(s[&4].early_finish, 9.0);
        assert!(s[&1].critical() && s[&2].critical() && s[&4].critical());
        assert_eq!(s[&3].slack(), 1.0);
        assert!(!s[&3].critical());
    }

    #[test]
    fn constraints_and_bad_durations() {
        let mut a = t(1, 2.0, &[]);
        a.not_before = 5.0;
        let mut b = t(2, f64::NAN, &[1]);
        b.duration = -3.0;
        let s = schedule(&[a, b]).unwrap_or_default();
        assert_eq!(
            (s[&1].early_start, s[&2].early_start, s[&2].early_finish),
            (5.0, 7.0, 7.0)
        );
    }

    #[test]
    fn errors() {
        assert_eq!(
            schedule(&[t(1, 1.0, &[2]), t(2, 1.0, &[1])]),
            Err(ScheduleError::Cycle(vec![1, 2]))
        );
        assert_eq!(
            schedule(&[t(1, 1.0, &[9])]),
            Err(ScheduleError::UnknownDependency {
                task: 1,
                missing: 9
            })
        );
        assert_eq!(
            schedule(&[t(1, 1.0, &[]), t(1, 1.0, &[])]),
            Err(ScheduleError::DuplicateId(1))
        );
        assert!(schedule(&[]).map(|m| m.is_empty()).unwrap_or(false));
    }

    #[test]
    fn large_chain_is_linear_time() {
        let tasks: Vec<Task> = (0..50_000u32)
            .map(|i| t(i, 1.0, &[]))
            .map(|mut x| {
                if x.id > 0 {
                    x.after = vec![x.id - 1];
                }
                x
            })
            .collect();
        let s = schedule(&tasks).unwrap_or_default();
        assert_eq!(s[&49_999].early_finish, 50_000.0);
    }
}
