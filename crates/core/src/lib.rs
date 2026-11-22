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

pub mod ledger;
pub mod points;
pub mod settlement;
pub mod task;

/// Point amounts. Signed because ledger deltas can be negative.
pub type Points = i64;

pub use ledger::{LedgerDelta, LedgerKind};
pub use points::{ApprovalRule, Escalation, PointPolicy, PointRange, PointsError};
pub use settlement::{
    DeficitAction, MemberPeriodInput, Outcome, PeriodLength, PeriodMode, PeriodPolicy, Settlement,
    SettlementLine, SurplusAction,
};
pub use task::{TaskAction, TaskKind, TaskStatus, TransitionError};
