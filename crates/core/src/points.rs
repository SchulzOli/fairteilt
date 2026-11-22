//! Point ranges per task, household-wide limits and escalation of overdue tasks.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Points;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PointsError {
    #[error("Punktebereich ungültig: es muss min ≤ default ≤ max gelten (min {min}, default {default}, max {max})")]
    InvalidRange { min: Points, default: Points, max: Points },
    #[error("Punkte müssen positiv sein")]
    NotPositive,
    #[error("Bereich {min}…{max} liegt außerhalb der Haushaltsgrenzen {policy_min}…{policy_max}")]
    OutsidePolicy { min: Points, max: Points, policy_min: Points, policy_max: Points },
    #[error("Anpassung {value} liegt außerhalb von {min}…{max}")]
    AdjustmentOutOfRange { value: Points, min: Points, max: Points },
}

/// `min ≤ default ≤ max` for a single task or template.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PointRange {
    pub min: Points,
    pub default: Points,
    pub max: Points,
}

impl PointRange {
    pub fn new(min: Points, default: Points, max: Points) -> Result<Self, PointsError> {
        if min <= 0 {
            return Err(PointsError::NotPositive);
        }
        if !(min <= default && default <= max) {
            return Err(PointsError::InvalidRange { min, default, max });
        }
        Ok(Self { min, default, max })
    }

    /// A fixed value (min = default = max).
    pub fn fixed(points: Points) -> Result<Self, PointsError> {
        Self::new(points, points, points)
    }

    pub fn clamp(&self, value: Points) -> Points {
        value.clamp(self.min, self.max)
    }
}

/// Overdue / unclaimed tasks gain value over time, capped at the task's max.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Escalation {
    /// Points added per full interval.
    pub step_points: Points,
    /// Interval length in seconds (e.g. 86_400 for "per day").
    pub step_interval_secs: i64,
}

impl Escalation {
    /// Bonus for a task that has been lying around for `age_secs`.
    /// Negative ages (not yet due) yield 0.
    pub fn bonus(&self, age_secs: i64) -> Points {
        if age_secs <= 0 || self.step_interval_secs <= 0 || self.step_points <= 0 {
            return 0;
        }
        (age_secs / self.step_interval_secs).saturating_mul(self.step_points)
    }
}

/// Who needs another member's approval before a completed task is credited.
/// Templates can override this per task (always / never / with photo).
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalRule {
    /// Every completion is approved by someone else.
    Everyone,
    /// Only tasks completed by members with the child role (household default).
    ChildrenOnly,
    /// Completed = credited; admins can still revert afterwards.
    Nobody,
}

impl ApprovalRule {
    pub fn required_for(self, completer_is_child: bool) -> bool {
        match self {
            ApprovalRule::Everyone => true,
            ApprovalRule::ChildrenOnly => completer_is_child,
            ApprovalRule::Nobody => false,
        }
    }
}

/// Household-wide point rules. Stored as JSON on the household.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointPolicy {
    /// Hard lower bound for any task.
    pub task_min: Points,
    /// Hard upper bound for any task.
    pub task_max: Points,
    /// Suggested value for new tasks.
    pub default_points: Points,
    pub escalation: Option<Escalation>,
    pub bounty_enabled: bool,
    /// Max balance a member can pledge on one task in one go.
    pub bounty_max_per_pledge: Option<Points>,
    pub transfer_enabled: bool,
    /// Max balance a member can transfer to others per period.
    pub transfer_max_per_period: Option<Points>,
    pub child_transfer_needs_approval: bool,
    /// Who needs approval before completed tasks are credited.
    pub approval: ApprovalRule,
    /// Approver may adjust the points within the task's min…max.
    pub quality_adjust: bool,
    /// Allow `penalty_deduction` to push a balance below zero.
    pub allow_negative_balance: bool,
}

impl Default for PointPolicy {
    fn default() -> Self {
        Self {
            task_min: 1,
            task_max: 100,
            default_points: 10,
            escalation: Some(Escalation { step_points: 2, step_interval_secs: 86_400 }),
            bounty_enabled: true,
            bounty_max_per_pledge: Some(50),
            transfer_enabled: true,
            transfer_max_per_period: Some(100),
            child_transfer_needs_approval: true,
            approval: ApprovalRule::ChildrenOnly,
            quality_adjust: true,
            allow_negative_balance: false,
        }
    }
}

impl PointPolicy {
    /// Checks that a task's range lies within the household limits.
    pub fn validate_range(&self, range: &PointRange) -> Result<(), PointsError> {
        if range.min < self.task_min || range.max > self.task_max {
            return Err(PointsError::OutsidePolicy {
                min: range.min,
                max: range.max,
                policy_min: self.task_min,
                policy_max: self.task_max,
            });
        }
        Ok(())
    }

    /// Base points (= contribution) a task is worth right now, before approver adjustment.
    pub fn current_points(&self, range: &PointRange, age_secs: i64) -> Points {
        let bonus = self.escalation.map(|e| e.bonus(age_secs)).unwrap_or(0);
        range.clamp(range.default.saturating_add(bonus))
    }

    /// Final base points on approval. `adjusted` is the approver's choice (if allowed).
    pub fn completion_points(
        &self,
        range: &PointRange,
        age_secs: i64,
        adjusted: Option<Points>,
    ) -> Result<Points, PointsError> {
        match adjusted {
            Some(value) if self.quality_adjust => {
                if value < range.min || value > range.max {
                    return Err(PointsError::AdjustmentOutOfRange {
                        value,
                        min: range.min,
                        max: range.max,
                    });
                }
                Ok(value)
            }
            _ => Ok(self.current_points(range, age_secs)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    #[test]
    fn range_validation() {
        assert!(PointRange::new(5, 10, 20).is_ok());
        assert_eq!(PointRange::new(0, 1, 2), Err(PointsError::NotPositive));
        assert!(matches!(PointRange::new(5, 30, 20), Err(PointsError::InvalidRange { .. })));
        assert!(matches!(PointRange::new(10, 5, 20), Err(PointsError::InvalidRange { .. })));
    }

    #[test]
    fn policy_bounds() {
        let p = PointPolicy::default();
        assert!(p.validate_range(&PointRange::new(1, 10, 100).unwrap()).is_ok());
        assert!(p.validate_range(&PointRange::new(1, 10, 101).unwrap()).is_err());
    }

    #[test]
    fn escalation_grows_until_max() {
        let p = PointPolicy::default(); // +2 per day
        let r = PointRange::new(5, 10, 16).unwrap();
        assert_eq!(p.current_points(&r, -DAY), 10, "not due yet");
        assert_eq!(p.current_points(&r, DAY - 1), 10, "less than one full day");
        assert_eq!(p.current_points(&r, DAY), 12);
        assert_eq!(p.current_points(&r, 2 * DAY), 14);
        assert_eq!(p.current_points(&r, 30 * DAY), 16, "capped at max");
    }

    #[test]
    fn no_escalation() {
        let p = PointPolicy { escalation: None, ..Default::default() };
        let r = PointRange::new(5, 10, 16).unwrap();
        assert_eq!(p.current_points(&r, 30 * DAY), 10);
    }

    #[test]
    fn approval_rule() {
        assert!(ApprovalRule::ChildrenOnly.required_for(true));
        assert!(!ApprovalRule::ChildrenOnly.required_for(false));
        assert!(ApprovalRule::Everyone.required_for(false));
        assert!(!ApprovalRule::Nobody.required_for(true));
        assert_eq!(PointPolicy::default().approval, ApprovalRule::ChildrenOnly);
    }

    #[test]
    fn approver_adjustment() {
        let p = PointPolicy::default();
        let r = PointRange::new(5, 10, 16).unwrap();
        assert_eq!(p.completion_points(&r, 0, Some(15)), Ok(15));
        assert!(p.completion_points(&r, 0, Some(17)).is_err());
        let strict = PointPolicy { quality_adjust: false, ..Default::default() };
        assert_eq!(strict.completion_points(&r, 0, Some(15)), Ok(10), "adjustment ignored");
    }
}
