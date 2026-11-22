//! Periodic settlement ("Gleichstellung").
//!
//! At the end of each period every member's contribution (Leistungspunkte) is compared
//! with a target derived from their weight (capacity) and availability, and/or a fixed
//! quota. Members outside the tolerance band get consequences: penalty tasks, balance
//! deduction, zero-sum payment to over-achievers or a carried-over deficit; members above
//! the band may receive a balance bonus or carry their lead into the next period.
//!
//! ```text
//! capacity_m = weight_m · availability_m
//! fair_m     = T · capacity_m / Σ capacity          (T = Σ contribution)
//! target_m   = max(0, fair_m | quota_m · availability_m | max(both)  + carry_in_m)
//! deviation  = contribution_m − target_m
//! ```
//!
//! `carry_in` is signed: a carried deficit (> 0) raises the next target, a carried lead
//! (< 0) lowers it.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Points;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PeriodLength {
    /// `start_weekday`: 1 = Monday … 7 = Sunday (ISO 8601).
    Weekly { start_weekday: u8 },
    /// `start_day`: day of month 1…28.
    Monthly { start_day: u8 },
    CustomDays { days: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodMode {
    /// Target relative to weight and availability.
    FairShare,
    /// Fixed minimum points per member (scaled by availability).
    FixedQuota,
    /// The higher of both.
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeficitAction {
    None,
    /// Assign penalty tasks worth roughly the deficit.
    PenaltyTasks,
    /// Deduct `percent` % of the deficit from the balance.
    BalanceDeduction { percent: u32 },
    /// Pay balance to the members above target, proportional to their surplus.
    ZeroSum,
    /// Add `percent` % of the deficit to next period's target.
    CarryOver { percent: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SurplusAction {
    None,
    /// Credit `percent` % of the surplus as balance.
    Bonus { percent: u32 },
    /// Lower next period's target by `percent` % of the surplus (the lead carries over).
    CarryOver { percent: u32 },
}

/// Household-wide settlement rules. Stored as JSON on the household.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeriodPolicy {
    pub length: PeriodLength,
    pub mode: PeriodMode,
    /// Allowed relative deviation without consequences, in percent.
    pub tolerance_percent: u32,
    pub deficit_action: DeficitAction,
    pub surplus_action: SurplusAction,
    /// Draft settlements are approved automatically after this many hours (None = manual only).
    pub auto_approve_after_hours: Option<u32>,
    /// Warn members below target this many hours before the period ends.
    pub warn_before_end_hours: Option<u32>,
}

impl Default for PeriodPolicy {
    fn default() -> Self {
        Self {
            // Household default (decided 22.11.2026): monthly, 20 % tolerance,
            // deficit and lead both carry over into the next month.
            length: PeriodLength::Monthly { start_day: 1 },
            mode: PeriodMode::FairShare,
            tolerance_percent: 20,
            deficit_action: DeficitAction::CarryOver { percent: 100 },
            surplus_action: SurplusAction::CarryOver { percent: 100 },
            auto_approve_after_hours: Some(24),
            warn_before_end_hours: Some(48),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberPeriodInput {
    pub member_id: Uuid,
    /// Capacity, e.g. adult 1.0, teenager 0.6, child 0.3.
    pub weight: f64,
    /// Share of the period the member was available (not on holiday / sick), 0…1.
    pub availability: f64,
    /// Leistungspunkte earned in this period.
    pub contribution: Points,
    /// Carried over from the previous period: deficit > 0 raises the target,
    /// lead < 0 lowers it. This is the previous line's `carry_next`.
    pub carry_in: Points,
    /// Fixed quota for `FixedQuota` / `Both` (full availability).
    pub quota: Option<Points>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    WithinTolerance,
    Deficit,
    Surplus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettlementLine {
    pub member_id: Uuid,
    pub target: Points,
    pub contribution: Points,
    /// contribution − target
    pub deviation: Points,
    /// deviation / target. `None` when the target is 0.
    pub deviation_ratio: Option<f64>,
    pub outcome: Outcome,
    /// Net balance change from all consequences (bonus, deduction, zero-sum).
    pub balance_delta: Points,
    /// Points worth of penalty tasks to assign.
    pub penalty_points_due: Points,
    /// To feed into next period's `carry_in`: deficit > 0, lead < 0.
    pub carry_next: Points,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settlement {
    pub total_contribution: Points,
    pub lines: Vec<SettlementLine>,
}

fn percent_of(value: Points, percent: u32) -> Points {
    ((value as f64) * f64::from(percent) / 100.0).round() as Points
}

/// Computes the settlement for one period. Pure: the caller persists the result,
/// creates ledger entries and penalty tasks, and caps deductions by the actual balance
/// if the household forbids negative balances.
pub fn settle(policy: &PeriodPolicy, members: &[MemberPeriodInput]) -> Settlement {
    let total: Points = members.iter().map(|m| m.contribution).sum();
    let capacity = |m: &MemberPeriodInput| m.weight.max(0.0) * m.availability.clamp(0.0, 1.0);
    let total_capacity: f64 = members.iter().map(capacity).sum();
    let tolerance = f64::from(policy.tolerance_percent) / 100.0;

    let mut lines: Vec<SettlementLine> = members
        .iter()
        .map(|m| {
            let fair = if total_capacity > 0.0 {
                total as f64 * capacity(m) / total_capacity
            } else {
                0.0
            };
            let quota = m.quota.unwrap_or(0) as f64 * m.availability.clamp(0.0, 1.0);
            let base = match policy.mode {
                PeriodMode::FairShare => fair,
                PeriodMode::FixedQuota => quota,
                PeriodMode::Both => fair.max(quota),
            };
            let target = (base.round() as Points + m.carry_in).max(0);
            let deviation = m.contribution - target;
            let deviation_ratio = (target > 0).then(|| deviation as f64 / target as f64);
            let outcome = match deviation_ratio {
                Some(r) if r < -tolerance => Outcome::Deficit,
                Some(r) if r > tolerance => Outcome::Surplus,
                Some(_) => Outcome::WithinTolerance,
                None if deviation > 0 => Outcome::Surplus,
                None => Outcome::WithinTolerance,
            };
            SettlementLine {
                member_id: m.member_id,
                target,
                contribution: m.contribution,
                deviation,
                deviation_ratio,
                outcome,
                balance_delta: 0,
                penalty_points_due: 0,
                carry_next: 0,
            }
        })
        .collect();

    // Individual consequences.
    for line in &mut lines {
        match line.outcome {
            Outcome::Deficit => {
                let deficit = -line.deviation;
                match policy.deficit_action {
                    DeficitAction::None | DeficitAction::ZeroSum => {}
                    DeficitAction::PenaltyTasks => line.penalty_points_due = deficit,
                    DeficitAction::BalanceDeduction { percent } => {
                        line.balance_delta -= percent_of(deficit, percent)
                    }
                    DeficitAction::CarryOver { percent } => {
                        line.carry_next = percent_of(deficit, percent)
                    }
                }
            }
            Outcome::Surplus => match policy.surplus_action {
                SurplusAction::None => {}
                SurplusAction::Bonus { percent } => {
                    line.balance_delta += percent_of(line.deviation, percent)
                }
                SurplusAction::CarryOver { percent } => {
                    line.carry_next = -percent_of(line.deviation, percent)
                }
            },
            Outcome::WithinTolerance => {}
        }
    }

    // Zero-sum: members below target pay those above, proportionally.
    if policy.deficit_action == DeficitAction::ZeroSum {
        let deficits: Vec<Points> = lines
            .iter()
            .map(|l| if l.outcome == Outcome::Deficit { -l.deviation } else { 0 })
            .collect();
        let surpluses: Vec<Points> = lines
            .iter()
            .map(|l| if l.outcome == Outcome::Surplus { l.deviation } else { 0 })
            .collect();
        let pool = deficits.iter().sum::<Points>().min(surpluses.iter().sum());
        if pool > 0 {
            let pay = allocate(pool, &deficits);
            let receive = allocate(pool, &surpluses);
            for (i, line) in lines.iter_mut().enumerate() {
                line.balance_delta += receive[i] - pay[i];
            }
        }
    }

    Settlement { total_contribution: total, lines }
}

/// Splits `total` into integer parts proportional to `weights` (largest remainder method).
/// The parts always sum to `total` (if any weight is positive).
pub fn allocate(total: Points, weights: &[Points]) -> Vec<Points> {
    let sum: Points = weights.iter().map(|w| (*w).max(0)).sum();
    if sum <= 0 || total <= 0 {
        return vec![0; weights.len()];
    }
    let mut parts: Vec<(usize, Points, Points)> = weights
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let exact = total as i128 * (*w).max(0) as i128;
            let q = (exact / sum as i128) as Points;
            let r = (exact % sum as i128) as Points;
            (i, q, r)
        })
        .collect();
    let mut rest = total - parts.iter().map(|p| p.1).sum::<Points>();
    // Largest remainder first; ties go to the larger weight, then the lower index (stable).
    let mut order: Vec<usize> = (0..parts.len()).collect();
    order.sort_by(|&a, &b| parts[b].2.cmp(&parts[a].2).then(weights[b].cmp(&weights[a])));
    for i in order {
        if rest == 0 {
            break;
        }
        if weights[i] > 0 {
            parts[i].1 += 1;
            rest -= 1;
        }
    }
    parts.into_iter().map(|p| p.1).collect()
}

/// Picks penalty tasks from `options` (id, points) until `due` points are covered.
/// Greedy: the largest task that still fits, repeated as needed; if nothing fits the rest,
/// the smallest task that covers it. At most `max_tasks` tasks.
pub fn pick_penalty_tasks<T: Copy>(due: Points, options: &[(T, Points)], max_tasks: usize) -> Vec<T> {
    let mut valid: Vec<(T, Points)> = options.iter().copied().filter(|(_, p)| *p > 0).collect();
    valid.sort_by_key(|o| std::cmp::Reverse(o.1));
    let mut picked = Vec::new();
    let mut remaining = due;
    while remaining > 0 && picked.len() < max_tasks {
        if let Some(&(id, p)) = valid.iter().find(|(_, p)| *p <= remaining) {
            picked.push(id);
            remaining -= p;
        } else if let Some(&(id, _)) = valid.last() {
            picked.push(id);
            break;
        } else {
            break;
        }
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(n: u128, weight: f64, contribution: Points) -> MemberPeriodInput {
        MemberPeriodInput {
            member_id: Uuid::from_u128(n),
            weight,
            availability: 1.0,
            contribution,
            carry_in: 0,
            quota: None,
        }
    }

    fn policy(deficit: DeficitAction, surplus: SurplusAction) -> PeriodPolicy {
        PeriodPolicy {
            length: PeriodLength::Weekly { start_weekday: 1 },
            tolerance_percent: 10,
            deficit_action: deficit,
            surplus_action: surplus,
            ..Default::default()
        }
    }

    fn balance_sum(s: &Settlement) -> Points {
        s.lines.iter().map(|l| l.balance_delta).sum()
    }

    #[test]
    fn equal_weights_penalty_and_bonus() {
        let p = policy(DeficitAction::PenaltyTasks, SurplusAction::Bonus { percent: 50 });
        let s = settle(&p, &[member(1, 1.0, 60), member(2, 1.0, 40)]);
        assert_eq!(s.total_contribution, 100);
        let (a, b) = (&s.lines[0], &s.lines[1]);
        assert_eq!((a.target, b.target), (50, 50));
        assert_eq!(a.outcome, Outcome::Surplus);
        assert_eq!(a.balance_delta, 5);
        assert_eq!(b.outcome, Outcome::Deficit);
        assert_eq!(b.penalty_points_due, 10);
        assert_eq!(b.balance_delta, 0);
    }

    #[test]
    fn weights_make_unequal_split_fair() {
        // Adult 1.0, child 0.25: 80/20 is exactly fair.
        let s = settle(&PeriodPolicy::default(), &[member(1, 1.0, 80), member(2, 0.25, 20)]);
        assert!(s.lines.iter().all(|l| l.outcome == Outcome::WithinTolerance));
        assert_eq!((s.lines[0].target, s.lines[1].target), (80, 20));
    }

    #[test]
    fn availability_scales_target() {
        let mut b = member(2, 1.0, 30);
        b.availability = 0.5; // on holiday half the week
        let s = settle(&PeriodPolicy::default(), &[member(1, 1.0, 60), b]);
        assert_eq!((s.lines[0].target, s.lines[1].target), (60, 30));
        assert!(s.lines.iter().all(|l| l.outcome == Outcome::WithinTolerance));
    }

    #[test]
    fn tolerance_boundary_is_inclusive() {
        // 55 vs 45 → targets 50, ±10 % exactly → within.
        let p = policy(DeficitAction::PenaltyTasks, SurplusAction::None);
        let s = settle(&p, &[member(1, 1.0, 55), member(2, 1.0, 45)]);
        assert!(s.lines.iter().all(|l| l.outcome == Outcome::WithinTolerance));
    }

    #[test]
    fn zero_sum_moves_balance_without_creating_any() {
        let p = policy(DeficitAction::ZeroSum, SurplusAction::None);
        let s = settle(&p, &[member(1, 1.0, 90), member(2, 1.0, 30), member(3, 1.0, 30)]);
        let d: Vec<Points> = s.lines.iter().map(|l| l.balance_delta).collect();
        assert_eq!(d, vec![40, -20, -20]);
        assert_eq!(balance_sum(&s), 0);
    }

    #[test]
    fn zero_sum_pool_limited_by_smaller_side() {
        // C is within tolerance (+2 on 50 = 4 %): it neither pays nor receives.
        let p = policy(DeficitAction::ZeroSum, SurplusAction::None);
        let s = settle(&p, &[member(1, 1.0, 70), member(2, 1.0, 28), member(3, 1.0, 52)]);
        assert_eq!(s.lines[2].outcome, Outcome::WithinTolerance);
        assert_eq!(s.lines[2].balance_delta, 0);
        assert_eq!(s.lines[0].balance_delta, 20); // surplus 20
        assert_eq!(s.lines[1].balance_delta, -20); // deficit 22, but only 20 to receive
        assert_eq!(balance_sum(&s), 0);
    }

    #[test]
    fn balance_deduction_and_carry_over() {
        let p = policy(DeficitAction::BalanceDeduction { percent: 50 }, SurplusAction::None);
        let s = settle(&p, &[member(1, 1.0, 60), member(2, 1.0, 40)]);
        assert_eq!(s.lines[1].balance_delta, -5);

        let p = policy(DeficitAction::CarryOver { percent: 100 }, SurplusAction::None);
        let s = settle(&p, &[member(1, 1.0, 60), member(2, 1.0, 40)]);
        assert_eq!(s.lines[1].carry_next, 10);

        // Next period: B's carried deficit raises the target.
        let mut b = member(2, 1.0, 50);
        b.carry_in = 10;
        let s = settle(&PeriodPolicy::default(), &[member(1, 1.0, 50), b]);
        assert_eq!(s.lines[1].target, 60);
    }

    #[test]
    fn fixed_quota_and_nobody_worked() {
        // Fair share with zero total: nobody is behind anybody.
        let s = settle(&PeriodPolicy::default(), &[member(1, 1.0, 0), member(2, 1.0, 0)]);
        assert!(s.lines.iter().all(|l| l.outcome == Outcome::WithinTolerance));

        // With a quota, doing nothing is a deficit.
        let mut p = PeriodPolicy { mode: PeriodMode::Both, ..Default::default() };
        p.deficit_action = DeficitAction::PenaltyTasks;
        let mut a = member(1, 1.0, 0);
        a.quota = Some(30);
        let s = settle(&p, &[a]);
        assert_eq!(s.lines[0].target, 30);
        assert_eq!(s.lines[0].penalty_points_due, 30);
    }

    #[test]
    fn zero_weight_member_who_worked_is_surplus() {
        let s = settle(&PeriodPolicy::default(), &[member(1, 1.0, 50), member(2, 0.0, 10)]);
        assert_eq!(s.lines[1].target, 0);
        assert_eq!(s.lines[1].deviation_ratio, None);
        assert_eq!(s.lines[1].outcome, Outcome::Surplus);
    }

    #[test]
    fn default_carries_deficit_and_lead() {
        // Monthly, 20 % tolerance: 70 vs 30 → targets 50, ±40 % → both carry.
        let s = settle(&PeriodPolicy::default(), &[member(1, 1.0, 70), member(2, 1.0, 30)]);
        assert_eq!(s.lines[0].carry_next, -20, "lead lowers next target");
        assert_eq!(s.lines[1].carry_next, 20, "deficit raises next target");
        assert!(s.lines.iter().all(|l| l.balance_delta == 0 && l.penalty_points_due == 0));

        // Next month both do 50: targets become 30 (lead) and 70 (deficit).
        let mut a = member(1, 1.0, 50);
        a.carry_in = s.lines[0].carry_next;
        let mut b = member(2, 1.0, 50);
        b.carry_in = s.lines[1].carry_next;
        let next = settle(&PeriodPolicy::default(), &[a, b]);
        assert_eq!((next.lines[0].target, next.lines[1].target), (30, 70));
    }

    #[test]
    fn within_tolerance_carries_nothing() {
        let s = settle(&PeriodPolicy::default(), &[member(1, 1.0, 55), member(2, 1.0, 45)]);
        assert!(s.lines.iter().all(|l| l.carry_next == 0));
    }

    #[test]
    fn large_lead_cannot_make_target_negative() {
        let mut a = member(1, 1.0, 10);
        a.carry_in = -500;
        let s = settle(&PeriodPolicy::default(), &[a, member(2, 1.0, 10)]);
        assert_eq!(s.lines[0].target, 0);
    }

    #[test]
    fn allocate_sums_exactly() {
        assert_eq!(allocate(10, &[1, 1, 1]), vec![4, 3, 3]);
        assert_eq!(allocate(7, &[0, 5, 0]), vec![0, 7, 0]);
        assert_eq!(allocate(5, &[0, 0]), vec![0, 0]);
        let parts = allocate(101, &[13, 29, 7, 51]);
        assert_eq!(parts.iter().sum::<Points>(), 101);
    }

    #[test]
    fn penalty_picker() {
        let opts = [('A', 10), ('B', 5), ('C', 30)];
        assert_eq!(pick_penalty_tasks(25, &opts, 10), vec!['A', 'A', 'B']);
        assert_eq!(pick_penalty_tasks(3, &opts, 10), vec!['B'], "smallest that covers");
        assert_eq!(pick_penalty_tasks(100, &opts, 2), vec!['C', 'C'], "capped");
        assert!(pick_penalty_tasks(10, &[('X', 0)], 5).is_empty());
    }

    #[test]
    fn policy_json_roundtrip() {
        let p = PeriodPolicy::default();
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"type\":\"carry_over\""));
        assert_eq!(serde_json::from_str::<PeriodPolicy>(&json).unwrap(), p);
    }
}
