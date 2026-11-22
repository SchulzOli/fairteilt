// Tasks and rooms of the active household, kept current over the live WebSocket (M2).
import type {
  CreateTemplateRequest,
  CreateTemplateResponse,
  RoomDto,
  TaskActionResponse,
  TaskCommand,
  TaskDto,
} from "$lib/generated";
import { session } from "$lib/session.svelte";
import { ICONS, type IconName } from "$lib/ui/Icon.svelte";

const DAY = 86_400_000;

class TaskStore {
  tasks = $state<TaskDto[]>([]);
  rooms = $state<RoomDto[]>([]);
  loaded = $state(false);
  /** Connected to the live channel. */
  live = $state(false);
  /** Ticks every second: elapsed time and condition bars move without reloading. */
  now = $state(Date.now());

  private hid: string | null = null;
  private ws: WebSocket | null = null;
  private retry = 0;
  private retryTimer: ReturnType<typeof setTimeout> | undefined;
  private reloadTimer: ReturnType<typeof setTimeout> | undefined;
  private clock: ReturnType<typeof setInterval> | undefined;
  /** Reloads wait until animations (confetti) are over, so cards do not vanish mid-celebration. */
  private holdUntil = 0;

  start(hid: string) {
    if (this.hid === hid) return;
    this.stop();
    this.hid = hid;
    this.clock = setInterval(() => (this.now = Date.now()), 1000);
    void this.load();
    void session.loadMembers();
    void this.connect();
  }

  stop() {
    this.hid = null;
    clearInterval(this.clock);
    clearTimeout(this.retryTimer);
    clearTimeout(this.reloadTimer);
    const ws = this.ws;
    this.ws = null;
    ws?.close();
    this.tasks = [];
    this.rooms = [];
    this.loaded = false;
    this.live = false;
  }

  async load() {
    const hid = this.hid;
    if (!hid) return;
    if (Date.now() < this.holdUntil) return this.scheduleReload();
    const [tasks, rooms] = await Promise.all([
      session.request<TaskDto[]>("GET", `/households/${hid}/tasks`),
      session.request<RoomDto[]>("GET", `/households/${hid}/rooms`),
    ]);
    if (hid !== this.hid) return;
    this.tasks = tasks;
    this.rooms = rooms;
    this.loaded = true;
  }

  /** Coalesces bursts of live events into one reload. */
  scheduleReload() {
    clearTimeout(this.reloadTimer);
    const wait = Math.max(120, this.holdUntil - Date.now());
    this.reloadTimer = setTimeout(() => void this.load().catch(() => {}), wait);
  }

  /** Keep what is on screen for `ms` (celebrations); reloads catch up afterwards. */
  hold(ms: number) {
    this.holdUntil = Date.now() + ms;
  }

  private async connect() {
    const hid = this.hid;
    const token = await session.liveToken();
    if (!token || hid !== this.hid) return;
    const url = `${session.serverUrl.replace(/^http/, "ws")}/api/v1/households/${hid}/live?token=${encodeURIComponent(token)}`;
    const ws = new WebSocket(url);
    this.ws = ws;
    ws.onmessage = (e) => {
      let type = "";
      try {
        type = JSON.parse(String(e.data)).type;
      } catch {
        return;
      }
      if (type === "hello") {
        this.live = true;
        // After a reconnect we may have missed events.
        if (this.retry > 0) this.scheduleReload();
        this.retry = 0;
      } else if (type === "tasks_changed" || type === "rooms_changed") {
        this.scheduleReload();
      }
    };
    ws.onclose = () => {
      if (this.ws !== ws) return;
      this.ws = null;
      this.live = false;
      const delay = Math.min(30_000, 1000 * 2 ** this.retry++);
      this.retryTimer = setTimeout(() => void this.connect(), delay);
    };
  }

  /** Back in the foreground: refresh at once (phones pause sockets in the background). */
  wake() {
    if (!this.hid) return;
    this.scheduleReload();
    if (!this.ws) {
      clearTimeout(this.retryTimer);
      void this.connect();
    }
  }

  // ---------- actions ----------
  async action(id: string, action: TaskCommand, minutes?: number): Promise<TaskActionResponse> {
    const res = await session.request<TaskActionResponse>("POST", `/tasks/${id}/action`, { action, minutes: minutes ?? null });
    if (res.xp_awarded !== null) this.hold(1500);
    this.patch(res);
    return res;
  }

  async checkStep(taskId: string, stepId: string): Promise<TaskActionResponse> {
    const res = await session.request<TaskActionResponse>("POST", `/tasks/${taskId}/steps/${stepId}/check`);
    if (res.xp_awarded !== null) this.hold(1800);
    this.patch(res);
    return res;
  }

  /** Shows the server's answer right away; the full list follows via reload. */
  private patch(res: TaskActionResponse) {
    const i = this.tasks.findIndex((t) => t.id === res.task.id);
    // A finished task keeps its place (and its card) until the held reload removes it;
    // otherwise it would jump sections mid-celebration.
    const finished = res.task.status === "completed" || res.task.status === "cancelled";
    if (i >= 0 && !finished) this.tasks[i] = res.task;
    this.scheduleReload();
  }

  async createTemplate(req: CreateTemplateRequest): Promise<CreateTemplateResponse> {
    const res = await session.request<CreateTemplateResponse>("POST", `/households/${this.hid}/templates`, req);
    this.tasks = [...this.tasks, res.task];
    this.scheduleReload();
    return res;
  }

  async createRoom(name: string, icon: string | null): Promise<RoomDto> {
    const room = await session.request<RoomDto>("POST", `/households/${this.hid}/rooms`, { name, icon });
    this.rooms = [...this.rooms, room];
    return room;
  }

  roomOf(t: TaskDto): RoomDto | undefined {
    return this.rooms.find((r) => r.id === t.room_id);
  }
}

export const taskStore = new TaskStore();

// ---------- presentation helpers ----------

/** Share of the round that has passed (like the server, but live). */
export function conditionOf(t: TaskDto, now: number): number {
  if (!t.due_at) return 0;
  const start = Date.parse(t.cycle_start);
  const due = Date.parse(t.due_at);
  if (due <= start) return now >= due ? 1 : 0;
  return Math.max(0, (now - start) / (due - start));
}

export type ConditionName = "fresh" | "due" | "overdue";
export const conditionName = (c: number): ConditionName => (c >= 1 ? "overdue" : c >= 0.75 ? "due" : "fresh");

function localDay(ms: number): number {
  const d = new Date(ms);
  return Math.floor(new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime() / DAY + 0.5);
}

/** "in 3 Tagen fällig", "heute fällig", "seit 2 Tagen überfällig". */
export function dueText(t: TaskDto, now: number): string {
  if (!t.due_at) return "ohne Termin";
  const due = Date.parse(t.due_at);
  const days = localDay(due) - localDay(now);
  if (due < now) {
    if (days >= 0) return "überfällig";
    return days === -1 ? "seit gestern überfällig" : `seit ${-days} Tagen überfällig`;
  }
  if (days <= 0) return "heute fällig";
  if (days === 1) return "morgen fällig";
  return `in ${days} Tagen fällig`;
}

/** Elapsed time, counting up: "4:07" or "1:02:09". */
export function clock(fromIso: string, now: number): string {
  const s = Math.max(0, Math.floor((now - Date.parse(fromIso)) / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

export function minutesSince(fromIso: string, now: number): number {
  return Math.max(0, Math.floor((now - Date.parse(fromIso)) / 60_000));
}

export function iconOf(name: string | null | undefined, fallback: IconName = "star"): IconName {
  return name && name in ICONS ? (name as IconName) : fallback;
}

/** Icons offered for tasks and rooms. */
export const TASK_ICONS: IconName[] = ["bath", "kitchen", "dish", "cutlery", "bin", "vacuum", "shirt", "window", "plant", "sofa", "star"];

export const stepsDone = (t: TaskDto) => t.steps.filter((s) => s.done_at !== null).length;

export function memberName(id: string | null): string {
  if (!id) return "";
  return session.members.find((m) => m.membership_id === id)?.display_name ?? "";
}
