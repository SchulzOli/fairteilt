//! `punkte-core`: pure domain logic for the household points system.
//!
//! No IO, no database, no clock. Everything here is deterministic and unit-tested,
//! so the server (authoritative) and the Tauri app (offline preview / validation)
//! compute exactly the same numbers.
//!
//! Concepts (see `docs/KONZEPT.md`):
//! - **Leistungspunkte (contribution)**: measure work done, drive the periodic settlement,
//!   never transferable.
//! - **Guthaben (balance)**: currency, transferable, spendable on rewards and bounties.

pub mod assignment;
pub mod condition;
pub mod goal;
pub mod ledger;
pub mod points;
pub mod schedule;
pub mod settlement;
pub mod steps;
pub mod streak;
pub mod task;

/// Point amounts. Signed because ledger deltas can be negative.
pub type Points = i64;

pub use assignment::Assignment;
pub use goal::{GoalPolicy, RaceStatus, decide as decide_race, krumel_position, overdue_boost, period_bounds, suggest_target};
pub use condition::{ConditionState, condition, state as condition_state};
pub use ledger::{LedgerDelta, LedgerKind};
pub use schedule::{Cycle, Schedule, ScheduleError};
pub use steps::{StepOrder, all_done, can_check};
pub use points::{ApprovalRule, Escalation, PointPolicy, PointRange, PointsError, TemplateApproval};
pub use streak::{Streak, streak};
pub use settlement::{
    DeficitAction, MemberPeriodInput, Outcome, PeriodLength, PeriodMode, PeriodPolicy, Settlement,
    SettlementLine, SurplusAction,
};
pub use task::{TaskAction, TaskKind, TaskStatus, TransitionError};
