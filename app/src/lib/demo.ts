// Demo data for the UI shell until the API is wired up (M1–M3). Names and numbers are examples.
import type { IconName } from "$lib/ui/Icon.svelte";
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

export const me = members[0];

export interface DemoRoom {
  id: string;
  name: string;
  icon: IconName;
}

export const rooms: DemoRoom[] = [
  { id: "kitchen", name: "Küche", icon: "kitchen" },
  { id: "bath", name: "Bad", icon: "bath" },
  { id: "living", name: "Wohnzimmer", icon: "sofa" },
  { id: "laundry", name: "Wäsche", icon: "shirt" },
  { id: "hall", name: "Flur", icon: "bin" },
];

export interface DemoTask {
  id: string;
  title: string;
  room: string;
  icon: IconName;
  /** Share of the interval that has passed: < 0.75 fresh, < 1 due, ≥ 1 overdue. */
  condition: number;
  xp: number;
  bounty?: number;
  assignee?: string;
  kind: "regular" | "reward" | "penalty";
}

export const tasks: DemoTask[] = [
  { id: "t1", title: "Spülmaschine ausräumen", room: "kitchen", icon: "dish", condition: 0.8, xp: 8, assignee: "Alex", kind: "regular" },
  { id: "t2", title: "Müll rausbringen", room: "hall", icon: "bin", condition: 0.9, xp: 5, assignee: "Alex", kind: "regular" },
  { id: "t3", title: "Fenster Wohnzimmer", room: "living", icon: "window", condition: 0.5, xp: 20, assignee: "Alex", kind: "penalty" },
  { id: "t4", title: "Bad putzen", room: "bath", icon: "bath", condition: 1.15, xp: 24, bounty: 15, kind: "regular" },
  { id: "t5", title: "Blumen gießen", room: "living", icon: "plant", condition: 0.3, xp: 4, kind: "reward" },
  { id: "t6", title: "Wäsche aufhängen", room: "laundry", icon: "shirt", condition: 0.6, xp: 12, assignee: "Sam", kind: "regular" },
  { id: "t7", title: "Tisch decken", room: "kitchen", icon: "cutlery", condition: 0.4, xp: 4, assignee: "Mia", kind: "regular" },
];

/** Family goal race vs. Krümel (KONZEPT §6). */
export const familyGoal = { xp: 268, goal: 400, krumelXp: 292, reward: "Ausflug", daysLeft: 8 };

export const myStreakDays = 5;
