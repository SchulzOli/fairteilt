// Offline queue (KONZEPT §9.2, M7): changes made without a connection wait here (also across
// app restarts) and are sent in order once the server is reachable again. Every entry carries
// its own Idempotency-Key, so a change whose answer got lost is never booked twice. When the
// server refuses one (rules, rights, someone was faster), it is dropped and the person told.
import { notices } from "$lib/notice.svelte";
import { ApiError, session } from "$lib/session.svelte";

const KEY = "fairteilt.outbox";

export interface Pending {
  id: string;
  method: string;
  path: string;
  body: unknown;
  /** Shown to the person: "„Bad putzen“ erledigt". */
  label: string;
  created: number;
}

function load(): Pending[] {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "[]");
  } catch {
    return [];
  }
}

class Outbox {
  items = $state<Pending[]>(load());
  private flushing = false;
  /** Called after the queue was sent (the stores reload). */
  onFlushed: (() => void) | null = null;

  private save() {
    try {
      localStorage.setItem(KEY, JSON.stringify(this.items));
    } catch {
      /* kept in memory until the app closes */
    }
  }

  /**
   * Sends a write now, or queues it when there is no connection. Returns the server's answer,
   * or null when queued (the caller shows the change optimistically).
   */
  async send<T>(method: string, path: string, body: unknown, label: string): Promise<T | null> {
    const entry: Pending = { id: crypto.randomUUID(), method, path, body, label, created: Date.now() };
    if (this.items.length === 0) {
      try {
        return await session.request<T>(method, path, body, { "idempotency-key": entry.id });
      } catch (e) {
        if (!(e instanceof ApiError) || e.status !== 0) throw e;
      }
    }
    this.items = [...this.items, entry];
    this.save();
    return null;
  }

  /** Sends what is waiting, oldest first. Stops at the first missing connection. */
  async flush(): Promise<void> {
    if (this.flushing || this.items.length === 0) return;
    this.flushing = true;
    let sent = 0;
    try {
      while (this.items.length) {
        const next = this.items[0];
        try {
          await session.request(next.method, next.path, next.body, { "idempotency-key": next.id });
          sent++;
        } catch (e) {
          if (e instanceof ApiError && e.status === 0) break;
          const why = e instanceof Error ? e.message : String(e);
          notices.show(`${next.label} ging nicht durch`, "info", why);
        }
        this.items = this.items.slice(1);
        this.save();
      }
    } finally {
      this.flushing = false;
    }
    if (sent > 0) {
      notices.show(sent === 1 ? "Deine Änderung ist übertragen" : `${sent} Änderungen sind übertragen`, "xp");
      this.onFlushed?.();
    }
  }

  clear() {
    this.items = [];
    this.save();
  }
}

export const outbox = new Outbox();
