// Tasks and rooms of the active household, kept current over the live WebSocket (M2).
import type {
  AccountDto,
  PeriodDto,
  CreateTemplateRequest,
  CreateTemplateResponse,
  RoomDto,
  TaskActionResponse,
  TaskCommand,
  TaskDto,
} from "$lib/generated";
import { notices } from "$lib/notice.svelte";
import { outbox } from "$lib/outbox.svelte";
import { session } from "$lib/session.svelte";
import { ICONS, type IconName } from "$lib/ui/Icon.svelte";

const DAY = 86_400_000;

class TaskStore {
  tasks = $state<TaskDto[]>([]);
  rooms = $state<RoomDto[]>([]);
  /** Accounts (G, XP, streak) of all members, in member order. */
  accounts = $state<AccountDto[]>([]);
  /** Unread inbox entries (Postfach). */
  unread = $state(0);
  /** Running period: race against Krümel and the latest settlements. */
  period = $state<PeriodDto | null>(null);
  loaded = $state(false);
  /** Counts reloads: pages with their own data (history) reload when it changes. */
  version = $state(0);
  /** Connected to the live channel. */
  live = $state(false);
  /** Counts changes of the shopping lists (live `list_changed`, reconnects). */
  listVersion = $state(0);
  /** Ticks every second: elapsed time and condition bars move without reloading. */
  now = $state(Date.now());

  private hid: string | null = null;
  private member: string | undefined;
  private ws: WebSocket | null = null;
  private retry = 0;
  private retryTimer: ReturnType<typeof setTimeout> | undefined;
  private reloadTimer: ReturnType<typeof setTimeout> | undefined;
  private clock: ReturnType<typeof setInterval> | undefined;
  /** Reloads wait until animations (confetti) are over, so cards do not vanish mid-celebration. */
  private holdUntil = 0;

  /** `member`: who is looking (a child profile on the tablet has its own inbox and account). */
  start(hid: string, member?: string) {
    if (this.hid === hid) {
      if (member !== this.member) {
        this.member = member;
        this.hydrate();
        this.scheduleReload();
      }
      return;
    }
    this.stop();
    this.hid = hid;
    this.member = member;
    this.hydrate();
    this.clock = setInterval(() => (this.now = Date.now()), 1000);
    // Offline at start: the live channel reconnects and reloads once the server is back.
    void this.load().catch(() => {});
    void session.loadMembers().catch(() => {});
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
    this.accounts = [];
    this.period = null;
    this.loaded = false;
    this.live = false;
  }

  async load() {
    const hid = this.hid;
    if (!hid) return;
    if (Date.now() < this.holdUntil) return this.scheduleReload();
    const [tasks, rooms, accounts, period, inbox] = await Promise.all([
      session.request<TaskDto[]>("GET", `/households/${hid}/tasks`),
      session.request<RoomDto[]>("GET", `/households/${hid}/rooms`),
      session.request<AccountDto[]>("GET", `/households/${hid}/accounts`),
      session.request<PeriodDto>("GET", `/households/${hid}/period`),
      session.request<{ unread: number }>("GET", `/households/${hid}/inbox/unread`),
    ]);
    if (hid !== this.hid) return;
    const before = this.myAccount;
    this.tasks = tasks;
    this.rooms = rooms;
    this.accounts = accounts;
    this.celebrateStreak(before);
    this.celebrateOvertake(this.period, period);
    this.period = period;
    this.unread = inbox.unread;
    this.loaded = true;
    this.version++;
    this.persist();
  }

  // ---------- offline (M7): last state on the device ----------
  private get cacheKey(): string {
    return `fairteilt.cache.${this.hid}.${this.member ?? ""}`;
  }

  /** Show the last known state at once (also without a connection). */
  private hydrate() {
    try {
      const raw = localStorage.getItem(this.cacheKey);
      if (!raw) return;
      const c = JSON.parse(raw);
      this.tasks = c.tasks ?? [];
      this.rooms = c.rooms ?? [];
      this.accounts = c.accounts ?? [];
      this.period = c.period ?? null;
      this.unread = c.unread ?? 0;
      this.loaded = true;
    } catch {
      /* no usable cache */
    }
  }

  private persist() {
    try {
      localStorage.setItem(
        this.cacheKey,
        JSON.stringify({ tasks: this.tasks, rooms: this.rooms, accounts: this.accounts, period: this.period, unread: this.unread }),
      );
    } catch {
      /* storage full or blocked: works without cache */
    }
  }

  /** A queued change, shown as if it went through; the server confirms later. */
  private queuedResult(id: string, change: (t: TaskDto) => TaskDto | null): TaskActionResponse {
    const i = this.tasks.findIndex((t) => t.id === id);
    const before = this.tasks[i];
    const after = before ? change(structuredClone($state.snapshot(before)) as TaskDto) : null;
    if (i >= 0 && after) this.tasks[i] = after;
    if (i >= 0 && !after) {
      // Done: the card celebrates, then leaves the list.
      setTimeout(() => {
        this.tasks = this.tasks.filter((t) => t.id !== id);
        this.persist();
      }, 1600);
    }
    this.persist();
    return { task: after ?? before, xp_awarded: null, bounty_paid: null, submitted: false, next_task_id: null, queued: true } as TaskActionResponse;
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
      let event: LiveEvent;
      try {
        event = JSON.parse(String(e.data));
      } catch {
        return;
      }
      const type = event.type;
      if (type === "hello") {
        this.live = true;
        void outbox.flush();
        // After a reconnect we may have missed events.
        if (this.retry > 0) {
          this.scheduleReload();
          this.listVersion++;
        }
        this.retry = 0;
      } else if (type === "list_changed") {
        this.listVersion++;
      } else if (type === "tasks_changed" || type === "rooms_changed" || type === "points_changed") {
        this.scheduleReload();
      } else if (type === "goal_won") {
        notices.show(`Geschafft! Ihr habt Krümel geschlagen`, "streak", `Familienbelohnung: ${event.reward}`, "goal");
        this.scheduleReload();
      } else if (type === "goal_lost") {
        notices.show("Krümel war diesmal schneller", "info", "Nächste Runde, neues Glück.");
        this.scheduleReload();
      } else if (type === "period_closed") {
        if (session.isAdmin) notices.show("Der Abschluss ist fertig", "info", "Unter Punkte ansehen und freigeben.");
        this.scheduleReload();
      } else if (type === "household_deleted") {
        // Gone for everyone: the app falls back to "Haushalt einrichten".
        void session.loadMe().catch(() => {});
      } else if (type === "settlement_approved" || type === "rewards_changed") {
        this.scheduleReload();
      } else if (event.member) {
        this.personal(event);
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

  /** Something another person did for (or about) me. */
  private personal(e: LiveEvent) {
    const me = session.membership?.membership_id;
    if (e.member !== me || e.by === me) return;
    const who = memberName(e.by ?? null) || "Jemand";
    const note = e.note ? `„${e.note}“` : undefined;
    switch (e.type) {
      case "credited":
        notices.show(`${who} hat „${e.task}“ bestätigt · +${e.xp} XP${e.g && e.g > 0 ? ` · +${e.g} G` : ""}`, "xp", note, "confirm");
        break;
      case "returned":
        notices.show(`„${e.task}“ bitte nochmal anschauen`, "info", note ?? `${who} hat sie zurückgegeben.`);
        break;
      case "received":
        notices.show(`${who} hat dir ${e.g} G geschickt`, "coin", note, "coin");
        break;
      case "transfer_declined":
        notices.show(`Deine ${e.g} G an ${memberName(e.to ?? null)} wurden nicht freigegeben`, "info");
        break;
      case "redemption_approved":
        notices.show(`„${e.task}“ ist freigegeben`, "coin", `${who} hat zugestimmt. Viel Spaß!`, "confirm");
        break;
      case "redemption_rejected":
        notices.show(`„${e.task}“ diesmal nicht`, "info", `Deine ${e.g} G sind wieder da.`);
        break;
      case "reversed":
        notices.show(`${who} hat „${e.task ?? "eine Aufgabe"}“ zurückgenommen`, "info", note);
        break;
    }
  }

  /** The family just passed Krümel (same period): the overtake motif. */
  private celebrateOvertake(before: PeriodDto | null, now: PeriodDto) {
    if (!before || before.id !== now.id || now.goal.status !== "running") return;
    const was = before.goal.family_xp - before.goal.krumel_xp;
    const is = now.goal.family_xp - now.goal.krumel_xp;
    if (was <= 0 && is > 0) setTimeout(() => notices.show("Ihr habt Krümel überholt!", "streak", undefined, "overtake"), 900);
  }

  /** The streak grew (own task done or confirmed): a small celebration after the task's own one. */
  private celebrateStreak(before: AccountDto | undefined) {
    const now = this.myAccount;
    if (!before || !now || before.membership_id !== now.membership_id) return;
    if (now.streak.days > before.streak.days && now.streak.today) {
      const days = now.streak.days;
      setTimeout(() => notices.show(days === 1 ? "Serie gestartet!" : `Serie: ${days} Tage in Folge!`, "streak", undefined, "streak"), 1200);
    }
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
    const title = this.tasks.find((t) => t.id === id)?.title ?? "Aufgabe";
    const labels: Record<string, string> = { done: "erledigt", start: "gestartet", pause: "pausiert", claim: "übernommen", unclaim: "abgegeben", snooze: "verschoben", cancel: "übersprungen" };
    const res = await outbox.send<TaskActionResponse>("POST", `/tasks/${id}/action`, { action, minutes: minutes ?? null }, `„${title}“ ${labels[action] ?? action}`);
    if (!res) {
      const me = session.membership?.membership_id ?? null;
      const now = Date.now();
      return this.queuedResult(id, (t) => {
        if (action === "done" || action === "cancel") return null;
        if (action === "start") {
          const remind = now + (t.duration_min ?? 25) * 60_000;
          return { ...t, status: "in_progress", assignee_id: me, started_at: new Date(now).toISOString(), remind_at: new Date(remind).toISOString() };
        }
        if (action === "pause") return { ...t, status: "claimed", started_at: null, remind_at: null };
        if (action === "claim") return { ...t, status: "claimed", assignee_id: me };
        if (action === "unclaim") return { ...t, status: "open", assignee_id: null };
        if (action === "snooze") return { ...t, remind_at: new Date(now + (minutes ?? 10) * 60_000).toISOString() };
        return t;
      });
    }
    if (res.xp_awarded !== null) this.hold(1500);
    this.patch(res);
    return res;
  }

  async checkStep(taskId: string, stepId: string): Promise<TaskActionResponse> {
    const task = this.tasks.find((t) => t.id === taskId);
    const step = task?.steps.find((s) => s.id === stepId);
    const res = await outbox.send<TaskActionResponse>("POST", `/tasks/${taskId}/steps/${stepId}/check`, undefined, `Schritt „${step?.title ?? ""}“`);
    if (!res) {
      const me = session.membership?.membership_id ?? null;
      const now = new Date().toISOString();
      return this.queuedResult(taskId, (t) => {
        const steps = t.steps.map((s) => (s.id === stepId ? { ...s, done_at: now, done_by: me } : s));
        if (steps.every((s) => s.done_at)) return null;
        return { ...t, steps, status: t.status === "in_progress" ? t.status : "in_progress", started_at: t.started_at ?? now, assignee_id: t.assignee_id ?? me };
      });
    }
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
    if (res.task) this.tasks = [...this.tasks, res.task];
    this.scheduleReload();
    return res;
  }

  async createRoom(name: string, icon: string | null): Promise<RoomDto> {
    const room = await session.request<RoomDto>("POST", `/households/${this.hid}/rooms`, { name, icon });
    this.rooms = [...this.rooms, room];
    return room;
  }

  /** Deletes a room; its tasks stay, without a room. */
  async deleteRoom(id: string): Promise<void> {
    await session.request("DELETE", `/households/${this.hid}/rooms/${id}`);
    this.rooms = this.rooms.filter((r) => r.id !== id);
    this.tasks = this.tasks.map((t) => (t.room_id === id ? { ...t, room_id: null } : t));
    this.scheduleReload();
  }

  /** Moves a task (and its setup) to another room; null = without a room. */
  async moveToRoom(taskId: string, roomId: string | null): Promise<void> {
    this.tasks = this.tasks.map((t) => (t.id === taskId ? { ...t, room_id: roomId } : t));
    await session.request("PUT", `/tasks/${taskId}/room`, { room_id: roomId });
    this.scheduleReload();
  }

  /** Adult confirms (optionally with adjusted XP and a message) or sends the task back. */
  async review(id: string, approve: boolean, points: number | null, note: string | null): Promise<TaskActionResponse> {
    const res = await session.request<TaskActionResponse>("POST", `/tasks/${id}/review`, { approve, points, note });
    if (res.xp_awarded !== null) this.hold(1500);
    this.patch(res);
    return res;
  }

  async pledge(id: string, amount: number): Promise<TaskDto> {
    const task = await session.request<TaskDto>("POST", `/tasks/${id}/bounty`, { amount });
    const i = this.tasks.findIndex((t) => t.id === id);
    if (i >= 0) this.tasks[i] = task;
    this.scheduleReload();
    return task;
  }

  get myAccount(): AccountDto | undefined {
    return this.accounts.find((a) => a.membership_id === session.membership?.membership_id);
  }

  roomOf(t: TaskDto): RoomDto | undefined {
    return this.rooms.find((r) => r.id === t.room_id);
  }
}

/** Messages on the live channel. Personal ones name the `member` they concern. */
interface LiveEvent {
  type: string;
  member?: string;
  by?: string;
  task?: string | null;
  xp?: number;
  g?: number;
  note?: string | null;
  to?: string;
  reward?: string;
}

export const taskStore = new TaskStore();

/** The change was saved on the device and waits for the connection. */
export function isQueued(res: TaskActionResponse): boolean {
  return (res as TaskActionResponse & { queued?: boolean }).queued === true;
}

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
export const TASK_ICONS: IconName[] = ["bath", "kitchen", "dish", "cutlery", "bin", "vacuum", "shirt", "window", "plant", "sofa", "cart", "glass", "star"];

export const stepsDone = (t: TaskDto) => t.steps.filter((s) => s.done_at !== null).length;

export function memberName(id: string | null): string {
  if (!id) return "";
  return session.members.find((m) => m.membership_id === id)?.display_name ?? "";
}
