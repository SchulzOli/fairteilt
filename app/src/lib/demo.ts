// Example family for the settlement preview (Regeln). Names and numbers are examples.
import type { AnimalName } from "$lib/ui/Avatar.svelte";

export interface DemoMember {
  id: string;
  name: string;
  role: "admin" | "member" | "child";
  weight: number;
  color: string;
  animal: AnimalName;
}

export const members: DemoMember[] = [
  { id: "00000000-0000-0000-0000-000000000001", name: "Alex", role: "admin", weight: 1, color: "#2563eb", animal: "fox" },
  { id: "00000000-0000-0000-0000-000000000002", name: "Sam", role: "admin", weight: 1, color: "#d63384", animal: "owl" },
  { id: "00000000-0000-0000-0000-000000000003", name: "Mia", role: "child", weight: 0.3, color: "#e8660f", animal: "otter" },
];
