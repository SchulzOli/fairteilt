// Shopping lists (M8, KONZEPT §10). Anyone writes things down (no XP); the list calls its
// "auf Abruf" task ("Einkaufen"). Changes go through the offline queue, so writing down and
// checking off work in the shop without a connection.
import type { CheckItemResponse, Section, ShoppingDto, ShoppingItemDto, ShoppingListDto, TaskDto } from "$lib/generated";
import { outbox } from "$lib/outbox.svelte";
import { session } from "$lib/session.svelte";
import { taskStore } from "$lib/tasks.svelte";

export const SECTIONS: { id: Section; label: string }[] = [
  { id: "produce", label: "Obst & Gemüse" },
  { id: "bakery", label: "Brot & Backwaren" },
  { id: "chilled", label: "Kühlregal" },
  { id: "frozen", label: "Tiefkühl" },
  { id: "pantry", label: "Vorrat" },
  { id: "drinks", label: "Getränke" },
  { id: "drugstore", label: "Drogerie" },
  { id: "household", label: "Haushalt" },
  { id: "other", label: "Sonstiges" },
];

export const sectionLabel = (s: Section) => SECTIONS.find((x) => x.id === s)?.label ?? "Sonstiges";

/** Same rule as the server (`item_key`): "Milch" and " MILCH! " are one item. */
export const itemKey = (name: string) =>
  name
    .toLowerCase()
    .split("")
    .filter((c) => /[\p{L}\p{N}]/u.test(c))
    .join("");

/** Items grouped by section, in shop order. */
export function bySection(items: ShoppingItemDto[]): { section: Section; label: string; items: ShoppingItemDto[] }[] {
  return SECTIONS.map((s) => ({ section: s.id, label: s.label, items: items.filter((i) => i.section === s.id) })).filter((g) => g.items.length > 0);
}

const SELECTED = "fairteilt.list";

class ShoppingStore {
  data = $state<ShoppingDto | null>(null);
  loaded = $state(false);
  selected = $state<string | null>(null);
  private hid: string | null = null;

  get lists(): ShoppingListDto[] {
    return this.data?.lists ?? [];
  }

  get current(): ShoppingListDto | undefined {
    return this.lists.find((l) => l.id === this.selected) ?? this.lists[0];
  }

  /** Items still to buy on all lists (the badge in the navigation). */
  get openCount(): number {
    return this.lists.reduce((n, l) => n + l.items.filter((i) => !i.checked_at).length, 0);
  }

  listOf(task: TaskDto): ShoppingListDto | undefined {
    return task.list_id ? this.lists.find((l) => l.id === task.list_id) : undefined;
  }

  private get cacheKey() {
    // Under "fairteilt.cache.": signing out removes it with the other offline copies.
    return `fairteilt.cache.shopping.${this.hid}`;
  }

  start(hid: string) {
    if (this.hid === hid) return;
    this.hid = hid;
    this.data = null;
    this.loaded = false;
    try {
      this.selected = localStorage.getItem(SELECTED);
      const cached = localStorage.getItem(this.cacheKey);
      if (cached) {
        this.data = JSON.parse(cached);
        this.loaded = true;
      }
    } catch {
      /* no cache */
    }
    void this.load().catch(() => {});
  }

  stop() {
    this.hid = null;
    this.data = null;
    this.loaded = false;
  }

  async load() {
    const hid = this.hid;
    if (!hid) return;
    const data = await session.request<ShoppingDto>("GET", `/households/${hid}/shopping`);
    if (hid !== this.hid) return;
    // Changes still waiting in the queue stay visible until the server has them.
    if (outbox.items.some((p) => p.path.startsWith("/lists/") || p.path.startsWith("/items/"))) return;
    this.data = data;
    this.loaded = true;
    this.persist();
  }

  private persist() {
    try {
      localStorage.setItem(this.cacheKey, JSON.stringify(this.data));
    } catch {
      /* works without cache */
    }
  }

  select(id: string) {
    this.selected = id;
    try {
      localStorage.setItem(SELECTED, id);
    } catch {
      /* not remembered */
    }
  }

  private edit(listId: string, change: (l: ShoppingListDto) => ShoppingListDto) {
    if (!this.data) return;
    this.data = { ...this.data, lists: this.data.lists.map((l) => (l.id === listId ? change(l) : l)) };
    this.persist();
  }

  /** Writes something down (merged with the same item). `asMember`: wall display. */
  async add(listId: string, name: string, opts: { note?: string; section?: Section; urgent?: boolean; asMember?: string } = {}) {
    const clean = name.trim().replace(/\s+/g, " ");
    if (!clean) return;
    const list = this.lists.find((l) => l.id === listId);
    const key = itemKey(clean);
    const same = list?.items.find((i) => !i.checked_at && itemKey(i.name) === key);
    const suggestion = this.data?.suggestions.find((s) => itemKey(s.name) === key);
    const id = same?.id ?? crypto.randomUUID();
    const author = opts.asMember ?? session.membership?.membership_id ?? null;
    // Shown at once; the server's answer (or the reload) replaces it.
    this.edit(listId, (l) => {
      if (same) {
        const note = opts.note ? (same.note ? `${same.note} · ${opts.note}` : opts.note) : same.note;
        return { ...l, items: l.items.map((i) => (i.id === same.id ? { ...i, note, urgent: i.urgent || !!opts.urgent } : i)) };
      }
      const item: ShoppingItemDto = {
        id,
        list_id: listId,
        name: clean.charAt(0).toUpperCase() + clean.slice(1),
        note: opts.note?.trim() || null,
        section: opts.section ?? suggestion?.section ?? "other",
        urgent: !!opts.urgent,
        added_by: author,
        added_at: new Date().toISOString(),
        checked_by: null,
        checked_at: null,
      };
      return { ...l, items: [...l.items, item] };
    });
    await outbox.send(
      "POST",
      `/lists/${listId}/items`,
      { id: same ? null : id, name: clean, note: opts.note?.trim() || null, section: opts.section ?? null, urgent: !!opts.urgent, as_member: opts.asMember ?? null },
      `„${clean}“ aufgeschrieben`,
    );
    taskStore.scheduleReload();
  }

  /**
   * Checks an item: in the cart while I shop (checking starts the shopping, like a step);
   * otherwise I brought it on the side and it leaves the list.
   */
  async check(item: ShoppingItemDto, checked: boolean): Promise<CheckItemResponse | null> {
    const me = session.membership?.membership_id ?? null;
    const list = this.lists.find((l) => l.id === item.list_id);
    const round = list?.task_id ? taskStore.tasks.find((t) => t.id === list.task_id) : undefined;
    const shopping =
      round && (round.status === "in_progress" || ((round.status === "open" || round.status === "claimed") && (!round.assignee_id || round.assignee_id === me || (!!me && round.helpers.includes(me)))));
    this.edit(item.list_id, (l) => ({
      ...l,
      items:
        !checked || shopping
          ? l.items.map((i) => (i.id === item.id ? { ...i, checked_at: checked ? new Date().toISOString() : null, checked_by: checked ? me : null } : i))
          : l.items.filter((i) => i.id !== item.id),
    }));
    const res = await outbox.send<CheckItemResponse>("POST", `/items/${item.id}/check`, { checked }, `„${item.name}“ ${checked ? "abgehakt" : "zurückgelegt"}`);
    taskStore.scheduleReload();
    return res;
  }

  async setUrgent(item: ShoppingItemDto, urgent: boolean) {
    this.edit(item.list_id, (l) => ({ ...l, items: l.items.map((i) => (i.id === item.id ? { ...i, urgent } : i)) }));
    await outbox.send("PATCH", `/items/${item.id}`, { note: null, section: null, urgent }, `„${item.name}“ ${urgent ? "dringend" : "nicht mehr dringend"}`);
    taskStore.scheduleReload();
  }

  async setSection(item: ShoppingItemDto, section: Section) {
    this.edit(item.list_id, (l) => ({ ...l, items: l.items.map((i) => (i.id === item.id ? { ...i, section } : i)) }));
    await outbox.send("PATCH", `/items/${item.id}`, { note: null, section, urgent: null }, `„${item.name}“ einsortiert`);
  }

  async remove(item: ShoppingItemDto) {
    this.edit(item.list_id, (l) => ({ ...l, items: l.items.filter((i) => i.id !== item.id) }));
    await outbox.send("DELETE", `/items/${item.id}`, undefined, `„${item.name}“ gestrichen`);
    taskStore.scheduleReload();
  }

  async createList(name: string) {
    const id = await session.request<string>("POST", `/households/${this.hid}/lists`, { name, task_title: null });
    await this.load();
    this.select(id);
  }

  async renameList(id: string, name: string) {
    await session.request("PATCH", `/lists/${id}`, { name, task_title: null });
    await this.load();
  }

  async deleteList(id: string) {
    await session.request("DELETE", `/lists/${id}`);
    if (this.selected === id) this.selected = null;
    await this.load();
    taskStore.scheduleReload();
  }
}

export const shopping = new ShoppingStore();
