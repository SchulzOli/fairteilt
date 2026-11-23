//! Task kinds and the task status machine.
//!
//! ```text
//! open ──claim──▶ claimed ──start──▶ in_progress ──done──▶ submitted ──approve──▶ completed
//!   │               ▲  │                │                     └──reject──▶ claimed
//!   │               └──┼──── pause ─────┘
//!   └── start ─────────┘ (starting an open task claims it)
//!   cancel / expire (from any non-final state) ──▶ cancelled
//! ```

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// Normal household task.
    Regular,
    /// Legacy "Belohnungsaufgabe"; no longer offered (since 23.11.2026), kept so older
    /// tasks and saved view filters still read.
    Reward,
    /// Catch-up task, assigned by settlement or admins ("Aufholaufgabe").
    Penalty,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Claimed,
    /// Started: someone is working on it right now (time counts up, no countdown).
    InProgress,
    Submitted,
    Completed,
    Cancelled,
}

impl TaskStatus {
    pub fn is_final(self) -> bool {
        matches!(self, TaskStatus::Completed | TaskStatus::Cancelled)
    }
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskAction {
    Claim,
    /// Release a claimed task back to open.
    Unclaim,
    /// Start working on it (claims it if needed).
    Start,
    /// Stop working for now; stays with the same person.
    Pause,
    Done,
    Approve,
    Reject,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("Aktion {action:?} ist im Status {from:?} nicht möglich")]
pub struct TransitionError {
    pub from: TaskStatus,
    pub action: TaskAction,
}

impl TaskStatus {
    /// Next status for `action`. `approval_required` comes from the household/template.
    /// Who may perform an action (e.g. "nobody approves their own task") is checked by the caller.
    pub fn apply(self, action: TaskAction, approval_required: bool) -> Result<TaskStatus, TransitionError> {
        use TaskAction as A;
        use TaskStatus as S;
        let next = match (self, action) {
            (S::Open, A::Claim) => S::Claimed,
            (S::Claimed | S::InProgress, A::Unclaim) => S::Open,
            (S::Open | S::Claimed, A::Start) => S::InProgress,
            (S::InProgress, A::Pause) => S::Claimed,
            // Done straight from Open is allowed: "just did it" without claiming first.
            (S::Open | S::Claimed | S::InProgress, A::Done) if approval_required => S::Submitted,
            (S::Open | S::Claimed | S::InProgress, A::Done) => S::Completed,
            (S::Submitted, A::Approve) => S::Completed,
            (S::Submitted, A::Reject) => S::Claimed,
            (s, A::Cancel) if !s.is_final() => S::Cancelled,
            (from, action) => return Err(TransitionError { from, action }),
        };
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TaskAction as A;
    use TaskStatus as S;

    #[test]
    fn happy_path_with_approval() {
        let s = S::Open.apply(A::Claim, true).unwrap();
        let s = s.apply(A::Done, true).unwrap();
        assert_eq!(s, S::Submitted);
        assert_eq!(s.apply(A::Approve, true).unwrap(), S::Completed);
    }

    #[test]
    fn without_approval_done_completes() {
        assert_eq!(S::Open.apply(A::Done, false).unwrap(), S::Completed);
    }

    #[test]
    fn reject_goes_back_to_claimed() {
        assert_eq!(S::Submitted.apply(A::Reject, true).unwrap(), S::Claimed);
    }

    #[test]
    fn final_states_are_final() {
        for a in [A::Claim, A::Done, A::Approve, A::Reject, A::Cancel, A::Unclaim, A::Start, A::Pause] {
            assert!(S::Completed.apply(a, true).is_err());
            assert!(S::Cancelled.apply(a, true).is_err());
        }
    }

    #[test]
    fn start_pause_and_done_while_running() {
        let s = S::Open.apply(A::Start, false).unwrap();
        assert_eq!(s, S::InProgress, "starting an open task claims it");
        assert_eq!(s.apply(A::Pause, false).unwrap(), S::Claimed);
        assert_eq!(s.apply(A::Done, false).unwrap(), S::Completed);
        assert_eq!(s.apply(A::Done, true).unwrap(), S::Submitted);
        assert!(S::Submitted.apply(A::Start, false).is_err());
        assert!(S::Claimed.apply(A::Pause, false).is_err());
    }
}
