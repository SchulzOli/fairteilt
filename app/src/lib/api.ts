// Typed wrappers around the Tauri commands in src-tauri/src/lib.rs.
// Domain types come from punkte-core via ts-rs (src/lib/generated), never by hand.
import { invoke } from "@tauri-apps/api/core";
import type { MemberPeriodInput, PeriodPolicy, PointPolicy, Settlement } from "./generated";

export type * from "./generated";

export const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

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
