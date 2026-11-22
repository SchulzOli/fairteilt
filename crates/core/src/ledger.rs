//! Ledger entry kinds and the deltas they produce.
//!
//! The ledger is append-only. Balances are sums over entries; corrections are
//! counter-entries, never edits.

use serde::{Deserialize, Serialize};

use crate::Points;
use crate::task::TaskKind;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerDelta {
    pub kind: LedgerKind,
    /// Change of Leistungspunkte (contribution).
    pub contribution: Points,
    /// Change of Guthaben (balance).
    pub balance: Points,
}

impl LedgerDelta {
    /// Credit for completing a task. Penalty tasks give contribution but no balance.
    /// Bounties are paid out as separate `BountyPayout` entries.
    pub fn task_reward(kind: TaskKind, points: Points) -> Self {
        Self {
            kind: LedgerKind::TaskReward,
            contribution: points,
            balance: if kind.credits_balance() { points } else { 0 },
        }
    }

    /// Both sides of a balance transfer between members: (sender, receiver).
    pub fn transfer(amount: Points) -> (Self, Self) {
        (
            Self { kind: LedgerKind::TransferOut, contribution: 0, balance: -amount },
            Self { kind: LedgerKind::TransferIn, contribution: 0, balance: amount },
        )
    }

    pub fn bounty_hold(amount: Points) -> Self {
        Self { kind: LedgerKind::BountyHold, contribution: 0, balance: -amount }
    }

    pub fn bounty_payout(amount: Points) -> Self {
        Self { kind: LedgerKind::BountyPayout, contribution: 0, balance: amount }
    }

    pub fn bounty_refund(amount: Points) -> Self {
        Self { kind: LedgerKind::BountyRefund, contribution: 0, balance: amount }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_is_zero_sum_and_keeps_contribution() {
        let (out, inn) = LedgerDelta::transfer(15);
        assert_eq!(out.balance + inn.balance, 0);
        assert_eq!(out.contribution + inn.contribution, 0);
    }

    #[test]
    fn bounty_roundtrip_is_zero_sum() {
        assert_eq!(LedgerDelta::bounty_hold(20).balance + LedgerDelta::bounty_refund(20).balance, 0);
    }

    #[test]
    fn as_str_matches_serde() {
        let json = serde_json::to_string(&LedgerKind::SettlementZeroSum).unwrap();
        assert_eq!(json, format!("\"{}\"", LedgerKind::SettlementZeroSum.as_str()));
    }
}
