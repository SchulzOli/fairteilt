// Demo data for the UI shell until the API is wired up (M1–M3).

export interface DemoMember {
  id: string;
  name: string;
  role: "admin" | "member" | "child";
  weight: number;
  color: string;
}

export const members: DemoMember[] = [
  { id: "00000000-0000-0000-0000-000000000001", name: "Alex", role: "admin", weight: 1, color: "#3d6bff" },
  { id: "00000000-0000-0000-0000-000000000002", name: "Sam", role: "admin", weight: 1, color: "#1f9d61" },
  { id: "00000000-0000-0000-0000-000000000003", name: "Mia", role: "child", weight: 0.3, color: "#e0752d" },
];

export interface DemoTask {
  id: string;
  title: string;
  room: string;
  icon: string;
  due: string;
  overdue?: boolean;
  points: number;
  pointsMax: number;
  bounty?: number;
  assignee?: string;
  kind: "regular" | "reward" | "penalty";
}

export const tasks: DemoTask[] = [
  { id: "t1", title: "Spülmaschine ausräumen", room: "Küche", icon: "🍽️", due: "heute 18:00", points: 8, pointsMax: 12, assignee: "Alex", kind: "regular" },
  { id: "t2", title: "Bad putzen", room: "Bad", icon: "🛁", due: "seit gestern", overdue: true, points: 24, pointsMax: 30, bounty: 15, kind: "regular" },
  { id: "t3", title: "Müll rausbringen", room: "Flur", icon: "🗑️", due: "heute 20:00", points: 5, pointsMax: 5, assignee: "Alex", kind: "regular" },
  { id: "t4", title: "Fenster Wohnzimmer", room: "Wohnzimmer", icon: "🪟", due: "Do", points: 20, pointsMax: 25, assignee: "Alex", kind: "penalty" },
  { id: "t5", title: "Kuchen backen", room: "Küche", icon: "🎂", due: "Sa", points: 15, pointsMax: 15, kind: "reward" },
];

export const me = members[0];
