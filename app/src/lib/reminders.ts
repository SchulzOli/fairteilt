// Local reminders on this device (KONZEPT §8.2, stage 1): planned from the synced tasks, work
// offline and without a push service. Phones schedule them in the system; on desktop they are
// shown while the app runs (the system cannot schedule there).
//
// - "bald fällig": when one of your tasks turns yellow (75 % of its round)
// - "Wie läuft's?": the check-in of a started task, with Erledigt / +10 Min. / Später
import { isTauri } from "@tauri-apps/api/core";
import type { TaskDto } from "$lib/generated";
import { isPhone } from "$lib/session.svelte";

type ActionHandler = (actionId: string, taskId: string) => void;

const STORE_KEY = "fairteilt.reminders";
const CHANNEL = "fairteilt";
const DESKTOP_HORIZON_MS = 24 * 3600 * 1000;

interface Planned {
  id: number;
  at: number;
  title: string;
  body: string;
  taskId: string;
  checkin: boolean;
}

let ready: Promise<boolean> | null = null;
let onActionHandler: ActionHandler | null = null;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

/** What happens when someone taps a reminder or one of its buttons. */
export function setActionHandler(handler: ActionHandler) {
  onActionHandler = handler;
}

/** Stable 31-bit id per task and kind (the system identifies notifications by number). */
function idFor(taskId: string, kind: string): number {
  let h = 0;
  for (const c of `${taskId}:${kind}`) h = (Math.imul(h, 31) + c.charCodeAt(0)) | 0;
  return Math.abs(h) % 2_000_000_000;
}

function readPlanned(): Record<number, number> {
  try {
    return JSON.parse(localStorage.getItem(STORE_KEY) ?? "{}");
  } catch {
    return {};
  }
}

function writePlanned(p: Record<number, number>) {
  try {
    localStorage.setItem(STORE_KEY, JSON.stringify(p));
  } catch {
    /* private mode: plan again next time */
  }
}

async function plugin() {
  return import("@tauri-apps/plugin-notification");
}

/** Permission, channel, buttons and the action listener, once. False outside the app or when denied. */
export function prepare(): Promise<boolean> {
  if (!isTauri()) return Promise.resolve(false);
  ready ??= (async () => {
    const n = await plugin();
    let granted = await n.isPermissionGranted();
    if (!granted) granted = (await n.requestPermission()) === "granted";
    if (!granted) return false;
    await n.createChannel({ id: CHANNEL, name: "Fairteilt", description: "Erinnerungen und Nachfragen", importance: n.Importance.Default }).catch(() => {});
    await n
      .registerActionTypes([
        {
          id: "checkin",
          actions: [
            { id: "done", title: "Erledigt" },
            { id: "snooze", title: "+10 Min." },
            { id: "later", title: "Später" },
          ],
        },
      ])
      .catch(() => {});
    await n.onAction((e) => {
      const extra = (e.notification as { extra?: Record<string, unknown> } | undefined)?.extra;
      const taskId = typeof extra?.taskId === "string" ? extra.taskId : null;
      if (taskId && onActionHandler) onActionHandler(e.actionId, taskId);
    });
    return true;
  })().catch(() => false);
  return ready;
}

function plan(tasks: TaskDto[], me: string, now: number): Planned[] {
  const out: Planned[] = [];
  for (const t of tasks) {
    if (t.assignee_id !== me || ["submitted", "completed", "cancelled"].includes(t.status)) continue;
    if (t.status === "in_progress" && t.remind_at) {
      const at = Date.parse(t.remind_at);
      if (at > now) {
        out.push({ id: idFor(t.id, "checkin"), at, title: `Wie läuft's mit „${t.title}“?`, body: "Kein Stress, sag einfach kurz Bescheid.", taskId: t.id, checkin: true });
      }
      continue;
    }
    if (t.due_at) {
      const start = Date.parse(t.cycle_start);
      const due = Date.parse(t.due_at);
      const at = start + (due - start) * 0.75;
      if (at > now && due > at) {
        out.push({ id: idFor(t.id, "due"), at, title: `„${t.title}“ wird bald fällig`, body: "Der Balken wird gelb.", taskId: t.id, checkin: false });
      }
    }
  }
  return out;
}

/** Brings the device's reminders in line with the tasks (cheap to call after every reload). */
export async function sync(tasks: TaskDto[], me: string | undefined): Promise<void> {
  if (!me || !(await prepare())) return;
  const n = await plugin();
  const wanted = plan(tasks, me, Date.now());
  const before = readPlanned();
  const after: Record<number, number> = {};
  const stale = Object.keys(before).map(Number).filter((id) => !wanted.some((w) => w.id === id));
  if (isPhone()) {
    if (stale.length) await n.cancel(stale).catch(() => {});
    for (const w of wanted) {
      after[w.id] = w.at;
      if (before[w.id] === w.at) continue; // already scheduled for that moment
      // Same id again replaces the earlier schedule.
      n.sendNotification({
        id: w.id,
        channelId: CHANNEL,
        title: w.title,
        body: w.body,
        schedule: n.Schedule.at(new Date(w.at)),
        actionTypeId: w.checkin ? "checkin" : undefined,
        extra: { taskId: w.taskId },
      });
    }
  } else {
    for (const id of stale) {
      clearTimeout(timers.get(id));
      timers.delete(id);
    }
    for (const w of wanted) {
      if (w.at - Date.now() > DESKTOP_HORIZON_MS) continue;
      after[w.id] = w.at;
      if (before[w.id] === w.at && timers.has(w.id)) continue;
      clearTimeout(timers.get(w.id));
      timers.set(
        w.id,
        setTimeout(() => {
          timers.delete(w.id);
          n.sendNotification({ title: w.title, body: w.body, actionTypeId: w.checkin ? "checkin" : undefined, extra: { taskId: w.taskId } });
        }, w.at - Date.now()),
      );
    }
  }
  writePlanned(after);
}

/** Turned off (or signed out): nothing more from this device. */
export async function clear(): Promise<void> {
  for (const t of timers.values()) clearTimeout(t);
  timers.clear();
  const ids = Object.keys(readPlanned()).map(Number);
  writePlanned({});
  if (isTauri() && ids.length) {
    const n = await plugin();
    await n.cancel(ids).catch(() => {});
  }
}
