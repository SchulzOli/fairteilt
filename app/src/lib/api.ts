// Typed wrappers around the Tauri commands in src-tauri/src/lib.rs.
// Types mirror punkte-core (crates/core). Keep them in sync until type generation
// (ts-rs/specta) is added.
import { invoke } from "@tauri-apps/api/core";

export const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export type DeficitAction =
  | { type: "none" }
  | { type: "penalty_tasks" }
  | { type: "balance_deduction"; percent: number }
  | { type: "zero_sum" }
  | { type: "carry_over"; percent: number };

export type SurplusAction =
  | { type: "none" }
  | { type: "bonus"; percent: number }
  | { type: "carry_over"; percent: number };

export type PeriodLength =
  | { type: "weekly"; start_weekday: number }
  | { type: "monthly"; start_day: number }
  | { type: "custom_days"; days: number };

export interface PeriodPolicy {
  length: PeriodLength;
  mode: "fair_share" | "fixed_quota" | "both";
  tolerance_percent: number;
  deficit_action: DeficitAction;
  surplus_action: SurplusAction;
  auto_approve_after_hours: number | null;
  warn_before_end_hours: number | null;
}

export interface PointPolicy {
  task_min: number;
  task_max: number;
  default_points: number;
  escalation: { step_points: number; step_interval_secs: number } | null;
  bounty_enabled: boolean;
  bounty_max_per_pledge: number | null;
  transfer_enabled: boolean;
  transfer_max_per_period: number | null;
  child_transfer_needs_approval: boolean;
  approval: "everyone" | "children_only" | "nobody";
  quality_adjust: boolean;
  allow_negative_balance: boolean;
}

export interface MemberPeriodInput {
  member_id: string;
  weight: number;
  availability: number;
  contribution: number;
  /** deficit > 0 raises the target, lead < 0 lowers it */
  carry_in: number;
  quota: number | null;
}

export type Outcome = "within_tolerance" | "deficit" | "surplus";

export interface SettlementLine {
  member_id: string;
  target: number;
  contribution: number;
  deviation: number;
  deviation_ratio: number | null;
  outcome: Outcome;
  balance_delta: number;
  penalty_points_due: number;
  carry_next: number;
}

export interface Settlement {
  total_contribution: number;
  lines: SettlementLine[];
}

export interface ServerHealth {
  status: string;
  database: boolean;
  version: string;
}

export const api = {
  serverHealth: (baseUrl: string) => invoke<ServerHealth>("server_health", { baseUrl }),
  defaultPolicies: () =>
    invoke<{ point_policy: PointPolicy; period_policy: PeriodPolicy }>("default_policies"),
  previewSettlement: (policy: PeriodPolicy, members: MemberPeriodInput[]) =>
    invoke<Settlement>("preview_settlement", { policy, members }),
};

/** Tauri command errors arrive as { message }. */
export function errorMessage(err: unknown): string {
  if (err && typeof err === "object" && "message" in err) return String((err as { message: unknown }).message);
  return String(err);
}
