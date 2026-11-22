//! Task kinds and the task status machine.
//!
//! ```text
//! open ──claim──▶ claimed ──done──▶ submitted ──approve──▶ completed
//!   │                │                  └──reject──▶ claimed
//!   └── cancel / expire (from any non-final state) ──▶ cancelled
//! ```

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// Normal household task: contribution + balance.
    Regular,
    /// Popular task with extra points, unlocked as a reward.
    Reward,
    /// Assigned by settlement or admins: contribution, but no balance.
    Penalty,
}

impl TaskKind {
    /// Whether completing a task of this kind credits balance (Guthaben).
    pub fn credits_balance(self) -> bool {
        !matches!(self, TaskKind::Penalty)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Claimed,
    Submitted,
    Completed,
    Cancelled,
}

impl TaskStatus {
    pub fn is_final(self) -> bool {
        matches!(self, TaskStatus::Completed | TaskStatus::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskAction {
    Claim,
    /// Release a claimed task back to open.
    Unclaim,
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
            (S::Claimed, A::Unclaim) => S::Open,
            // Done straight from Open is allowed: "just did it" without claiming first.
            (S::Open | S::Claimed, A::Done) if approval_required => S::Submitted,
            (S::Open | S::Claimed, A::Done) => S::Completed,
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
        for a in [A::Claim, A::Done, A::Approve, A::Reject, A::Cancel, A::Unclaim] {
            assert!(S::Completed.apply(a, true).is_err());
            assert!(S::Cancelled.apply(a, true).is_err());
        }
    }

    #[test]
    fn penalty_gives_no_balance() {
        assert!(!TaskKind::Penalty.credits_balance());
        assert!(TaskKind::Regular.credits_balance());
    }
}
