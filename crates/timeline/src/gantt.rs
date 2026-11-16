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

/// Schedule `tasks`; result keyed by id. Negative/NaN durations count as 0.
pub fn schedule(tasks: &[Task]) -> Result<HashMap<u32, Scheduled>, ScheduleError> {
    let mut index = HashMap::new();
    for (i, t) in tasks.iter().enumerate() {
        if index.insert(t.id, i).is_some() {
            return Err(ScheduleError::DuplicateId(t.id));
        }
    }
    for t in tasks {
        if let Some(&m) = t.after.iter().find(|d| !index.contains_key(d)) {
            return Err(ScheduleError::UnknownDependency {
                task: t.id,
                missing: m,
            });
        }
    }
    // Kahn's algorithm for a topological order.
    let mut indeg: Vec<usize> = tasks.iter().map(|t| t.after.len()).collect();
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); tasks.len()];
    for (i, t) in tasks.iter().enumerate() {
        for d in &t.after {
            succ[index[d]].push(i);
        }
    }
    let mut order: Vec<usize> = (0..tasks.len()).filter(|i| indeg[*i] == 0).collect();
    let mut head = 0;
    while head < order.len() {
        let u = order[head];
        head += 1;
        for &v in &succ[u] {
            indeg[v] -= 1;
            if indeg[v] == 0 {
                order.push(v);
            }
        }
    }
    if order.len() != tasks.len() {
        let mut stuck: Vec<u32> = (0..tasks.len())
            .filter(|i| indeg[*i] > 0)
            .map(|i| tasks[i].id)
            .collect();
        stuck.sort_unstable();
        return Err(ScheduleError::Cycle(stuck));
    }
    let dur = |t: &Task| {
        if t.duration.is_finite() {
            t.duration.max(0.0)
        } else {
            0.0
        }
    };
    let mut es = vec![0.0f64; tasks.len()];
    let mut ef = vec![0.0f64; tasks.len()];
    for &i in &order {
        let t = &tasks[i];
        let start = t.after.iter().map(|d| ef[index[d]]).fold(
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
    for &i in order.iter().rev() {
        let finish = succ[i].iter().map(|&s| ls[s]).fold(project_end, f64::min);
        lf[i] = finish;
        ls[i] = finish - dur(&tasks[i]);
    }
    Ok(tasks
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
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

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
