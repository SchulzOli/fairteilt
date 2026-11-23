//! Ledger entry kinds and the deltas they produce.
//!
//! The ledger is append-only. Balances are sums over entries; corrections are
//! counter-entries, never edits.

use serde::{Deserialize, Serialize};

use crate::Points;
use crate::task::TaskKind;

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerKind {
    TaskReward,
    BountyHold,
    BountyPayout,
    BountyRefund,
    TransferOut,
    TransferIn,
    RewardRedeem,
    PenaltyDeduction,
    SettlementBonus,
    SettlementZeroSum,
    ManualAdjust,
}

impl LedgerKind {
    /// Database / wire representation (matches the serde name).
    pub fn as_str(self) -> &'static str {
        match self {
            LedgerKind::TaskReward => "task_reward",
            LedgerKind::BountyHold => "bounty_hold",
            LedgerKind::BountyPayout => "bounty_payout",
            LedgerKind::BountyRefund => "bounty_refund",
            LedgerKind::TransferOut => "transfer_out",
            LedgerKind::TransferIn => "transfer_in",
            LedgerKind::RewardRedeem => "reward_redeem",
            LedgerKind::PenaltyDeduction => "penalty_deduction",
            LedgerKind::SettlementBonus => "settlement_bonus",
            LedgerKind::SettlementZeroSum => "settlement_zero_sum",
            LedgerKind::ManualAdjust => "manual_adjust",
        }
    }
}

/// What one ledger entry does to a member's two accounts.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerDelta {
    pub kind: LedgerKind,
    /// Change of Leistungspunkte (contribution).
    pub contribution: Points,
    /// Change of Guthaben (balance).
    pub balance: Points,
}

impl LedgerDelta {
    /// Credit for completing a task: XP only. Guthaben is no longer earned (since 23.11.2026,
    /// the app shows XP only); `kind` stays for older callers.
    pub fn task_reward(_kind: TaskKind, points: Points) -> Self {
        Self { kind: LedgerKind::TaskReward, contribution: points, balance: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_matches_serde() {
        let json = serde_json::to_string(&LedgerKind::SettlementZeroSum).unwrap();
        assert_eq!(json, format!("\"{}\"", LedgerKind::SettlementZeroSum.as_str()));
    }
}
