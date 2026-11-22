//! Steps within a task (KONZEPT §7.1): free order or strictly one after another.
//! XP are awarded only when the last step is done.

use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepOrder {
    /// Any order (e.g. cleaning the bathroom).
    #[default]
    Free,
    /// One after another, only the current step can be checked (e.g. dishwasher: load, then unload).
    Sequential,
}

impl StepOrder {
    pub fn as_str(self) -> &'static str {
        match self {
            StepOrder::Free => "free",
            StepOrder::Sequential => "sequential",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "free" => Some(StepOrder::Free),
            "sequential" => Some(StepOrder::Sequential),
            _ => None,
        }
    }
}

/// Whether step `index` may be checked now. `done[i]` tells which steps are done.
pub fn can_check(order: StepOrder, done: &[bool], index: usize) -> bool {
    match done.get(index) {
        None | Some(true) => false,
        Some(false) => match order {
            StepOrder::Free => true,
            StepOrder::Sequential => done[..index].iter().all(|d| *d),
        },
    }
}

/// All steps done (a task without steps counts as done).
pub fn all_done(done: &[bool]) -> bool {
    done.iter().all(|d| *d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_only_current_step() {
        let done = [true, false, false];
        assert!(!can_check(StepOrder::Sequential, &done, 0), "already done");
        assert!(can_check(StepOrder::Sequential, &done, 1));
        assert!(!can_check(StepOrder::Sequential, &done, 2), "must wait for step 2");
        assert!(!can_check(StepOrder::Sequential, &done, 3), "out of range");
    }

    #[test]
    fn free_any_open_step() {
        let done = [false, true, false];
        assert!(can_check(StepOrder::Free, &done, 0));
        assert!(!can_check(StepOrder::Free, &done, 1));
        assert!(can_check(StepOrder::Free, &done, 2));
    }

    #[test]
    fn completion() {
        assert!(all_done(&[]));
        assert!(all_done(&[true, true]));
        assert!(!all_done(&[true, false]));
        assert_eq!(StepOrder::parse(StepOrder::Sequential.as_str()), Some(StepOrder::Sequential));
    }
}
