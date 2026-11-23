// Shared helpers for the plan views (M6): saved-view filtering, grouping, days.
import type { TaskDto, ViewDto, ViewFilter, ViewSort } from "$lib/generated";
import { conditionOf } from "$lib/tasks.svelte";

export const DAY = 86_400_000;
export const WEEKDAYS = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];

/** Local midnight of the day containing `t`. */
export function startOfDay(t: number): number {
  const d = new Date(t);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** Monday 00:00 of the week containing `t`. */
export function startOfWeek(t: number): number {
  const d = new Date(startOfDay(t));
  const back = (d.getDay() + 6) % 7;
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() - back).getTime();
}

export function addDays(t: number, n: number): number {
  const d = new Date(t);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + n, d.getHours(), d.getMinutes()).getTime();
}

export function sameDay(a: number, b: number): boolean {
  return startOfDay(a) === startOfDay(b);
}

export const emptyFilter = (): ViewFilter => ({
  members: [],
  open: false,
  rooms: [],
  statuses: [],
  kinds: [],
  bounty_only: false,
  due_within_days: null,
});

export const defaultSort = (): ViewSort => ({ by: "condition", group: "none" });

/** Tasks matching a saved view's filter, sorted as it asks. */
export function applyView(tasks: TaskDto[], view: Pick<ViewDto, "filter" | "sort">, now: number): TaskDto[] {
  const f = view.filter;
  const limit = f.due_within_days !== null ? now + f.due_within_days * DAY : null;
  const out = tasks.filter((t) => {
    if (f.members.length || f.open) {
      const mine = t.assignee_id !== null && f.members.includes(t.assignee_id);
      const open = t.assignee_id === null && f.open;
      if (!mine && !open) return false;
    }
    if (f.rooms.length && (!t.room_id || !f.rooms.includes(t.room_id))) return false;
    if (f.statuses.length && !f.statuses.includes(t.status)) return false;
    if (f.kinds.length && !f.kinds.includes(t.kind)) return false;
    if (f.bounty_only && t.bounty <= 0) return false;
    if (limit !== null && (!t.due_at || Date.parse(t.due_at) > limit)) return false;
    return true;
  });
  const by = view.sort.by;
  out.sort((a, b) => {
    if (by === "due") return (a.due_at ? Date.parse(a.due_at) : Infinity) - (b.due_at ? Date.parse(b.due_at) : Infinity);
    if (by === "xp") return b.xp - a.xp;
    if (by === "title") return a.title.localeCompare(b.title, "de");
    return conditionOf(b, now) - conditionOf(a, now);
  });
  return out;
}

export const STATUS_LABEL: Record<string, string> = {
  open: "Offen",
  claimed: "Übernommen",
  in_progress: "Im Laden",
  submitted: "Wartet aufs OK",
};

/** Groups for list/kanban: key, label, tasks (in the given order). */
export function groupTasks(
  tasks: TaskDto[],
  group: string,
  names: { member: (id: string | null) => string; room: (id: string | null) => string },
): { key: string; label: string; tasks: TaskDto[] }[] {
  if (group === "none") return [{ key: "all", label: "", tasks }];
  const keyOf = (t: TaskDto): string =>
    group === "member" ? (t.assignee_id ?? "") : group === "room" ? (t.room_id ?? "") : t.status;
  const labelOf = (k: string): string =>
    group === "member" ? names.member(k || null) || "Offen" : group === "room" ? names.room(k || null) || "Ohne Raum" : (STATUS_LABEL[k] ?? k);
  const order = group === "status" ? Object.keys(STATUS_LABEL) : [];
  const map = new Map<string, TaskDto[]>();
  for (const k of order) map.set(k, []);
  for (const t of tasks) {
    const k = keyOf(t);
    if (!map.has(k)) map.set(k, []);
    map.get(k)!.push(t);
  }
  return [...map.entries()].map(([key, ts]) => ({ key, label: labelOf(key), tasks: ts }));
}
