<script lang="ts">
  // Plan · Board (KONZEPT §7.3): a free canvas with cards and zones. A card dropped into a
  // person's zone is assigned, into a day of this week is planned (17:00), anywhere else it just
  // stays there. Pan with one finger / the mouse, zoom with two fingers / the wheel.
  // Narrow screens get a list: tap a card, then a zone.
  import type { BoardItemDto, TaskDto } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { iconOf, taskStore } from "$lib/tasks.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import type { AnimalName } from "$lib/ui/Avatar.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import { WEEKDAYS, addDays, sameDay, startOfWeek } from "./views";

  let { only = null }: { only?: TaskDto[] | null } = $props();

  const CARD_W = 160;
  const CARD_H = 58;
  const PZ_W = 250;
  const PZ_H = 330;
  const DZ_W = 180;
  const DZ_H = 210;
  const DAY_Y = PZ_H + 60;

  let items = $state<BoardItemDto[]>([]);
  let width = $state(800);
  let view = $state({ x: 20, y: 20, scale: 0.8 });
  let drag = $state<{ id: string; dx: number; dy: number; x: number; y: number } | null>(null);
  let picked = $state<string | null>(null);
  let error = $state("");

  const hid = $derived(session.membership?.household_id);
  const planner = $derived(session.membership?.role !== "child");
  const tasks = $derived((only ?? taskStore.tasks).filter((t) => t.status !== "submitted"));
  const week = $derived(startOfWeek(taskStore.now));

  type Zone = { key: string; kind: "person" | "day"; label: string; x: number; y: number; w: number; h: number; member?: string | null; day?: number };
  const zones = $derived.by((): Zone[] => {
    const people: Zone[] = [
      ...session.members.map((m, i) => ({ key: m.membership_id, kind: "person" as const, label: m.display_name, member: m.membership_id, x: i * (PZ_W + 20), y: 0, w: PZ_W, h: PZ_H })),
    ];
    people.push({ key: "open", kind: "person", label: "Offen", member: null, x: people.length * (PZ_W + 20), y: 0, w: PZ_W, h: PZ_H });
    const days: Zone[] = WEEKDAYS.map((d, i) => ({ key: `d${i}`, kind: "day", label: d, day: addDays(week, i), x: i * (DZ_W + 14), y: DAY_Y, w: DZ_W, h: DZ_H }));
    return [...people, ...days];
  });

  async function load() {
    if (!hid) return;
    items = await session.request<BoardItemDto[]>("GET", `/households/${hid}/board`);
  }

  $effect(() => {
    void taskStore.version;
    load().catch((e) => (error = e.message));
  });

  const zoneAt = (x: number, y: number) => zones.find((z) => x >= z.x && x <= z.x + z.w && y >= z.y && y <= z.y + z.h);

  /** Does a position agree with the task (right person, right day)? */
  function consistent(t: TaskDto, x: number, y: number): boolean {
    const z = zoneAt(x + CARD_W / 2, y + CARD_H / 2);
    if (!z) return true;
    if (z.kind === "person") return z.member === t.assignee_id;
    return !!t.scheduled_start && sameDay(Date.parse(t.scheduled_start), z.day!);
  }

  /** Where each card is: its saved place if it still fits, else stacked in its zone. */
  const placed = $derived.by(() => {
    const counts = new Map<string, number>();
    return tasks.map((t) => {
      const saved = items.find((i) => (t.template_id ? i.template_id === t.template_id : i.task_id === t.id));
      if (drag?.id === t.id) return { t, x: drag.x, y: drag.y };
      if (saved && consistent(t, saved.x, saved.y)) return { t, x: saved.x, y: saved.y };
      const thisWeek = t.scheduled_start && zones.find((z) => z.kind === "day" && sameDay(Date.parse(t.scheduled_start!), z.day!));
      const z = thisWeek || zones.find((z) => z.kind === "person" && z.member === t.assignee_id)!;
      const n = counts.get(z.key) ?? 0;
      counts.set(z.key, n + 1);
      return { t, x: z.x + 10, y: z.y + 44 + n * (CARD_H + 8) };
    });
  });

  async function dropped(t: TaskDto, x: number, y: number) {
    error = "";
    const z = zoneAt(x + CARD_W / 2, y + CARD_H / 2);
    try {
      if (z?.kind === "person" && z.member !== t.assignee_id) {
        await session.request("PUT", `/tasks/${t.id}/assign`, { assignee_id: z.member });
        feedback("step");
      } else if (z?.kind === "day" && !(t.scheduled_start && sameDay(Date.parse(t.scheduled_start), z.day!))) {
        const keep = t.scheduled_start ? new Date(t.scheduled_start) : null;
        const at = new Date(z.day!);
        at.setHours(keep?.getHours() ?? 17, keep?.getMinutes() ?? 0, 0, 0);
        await session.request("PUT", `/tasks/${t.id}/schedule`, { scheduled_start: at.toISOString(), scheduled_end: null });
        feedback("step");
      }
      await session.request("PUT", `/households/${hid}/board`, { task_id: t.id, x, y });
      const key = t.template_id ? { template_id: t.template_id, task_id: t.id } : { template_id: null, task_id: t.id };
      items = [...items.filter((i) => (t.template_id ? i.template_id !== t.template_id : i.task_id !== t.id)), { ...key, x, y }];
      taskStore.scheduleReload();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  // ---------- pointer: card drag, pan, pinch ----------
  const pointers = new Map<number, { x: number; y: number }>();
  let pan: { x: number; y: number; vx: number; vy: number } | null = null;
  let pinch: { d: number; scale: number } | null = null;

  function cardDown(ev: PointerEvent, t: TaskDto, x: number, y: number) {
    if (!planner) return;
    ev.stopPropagation();
    (ev.currentTarget as HTMLElement).setPointerCapture(ev.pointerId);
    drag = { id: t.id, dx: ev.clientX / view.scale - x, dy: ev.clientY / view.scale - y, x, y };
  }

  function cardMove(ev: PointerEvent) {
    if (!drag) return;
    drag = { ...drag, x: ev.clientX / view.scale - drag.dx, y: ev.clientY / view.scale - drag.dy };
  }

  function cardUp(t: TaskDto) {
    if (!drag) return;
    const { x, y } = drag;
    drag = null;
    void dropped(t, Math.round(x), Math.round(y));
  }

  function down(ev: PointerEvent) {
    pointers.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
    (ev.currentTarget as HTMLElement).setPointerCapture(ev.pointerId);
    if (pointers.size === 1) pan = { x: ev.clientX, y: ev.clientY, vx: view.x, vy: view.y };
    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      pinch = { d: Math.hypot(a.x - b.x, a.y - b.y), scale: view.scale };
      pan = null;
    }
  }

  function move(ev: PointerEvent) {
    if (!pointers.has(ev.pointerId)) return;
    pointers.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
    if (pinch && pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      view = { ...view, scale: Math.min(1.6, Math.max(0.35, (pinch.scale * Math.hypot(a.x - b.x, a.y - b.y)) / pinch.d)) };
    } else if (pan) {
      view = { ...view, x: pan.vx + ev.clientX - pan.x, y: pan.vy + ev.clientY - pan.y };
    }
  }

  function up(ev: PointerEvent) {
    pointers.delete(ev.pointerId);
    if (pointers.size < 2) pinch = null;
    if (pointers.size === 0) pan = null;
  }

  function wheel(ev: WheelEvent) {
    ev.preventDefault();
    view = { ...view, scale: Math.min(1.6, Math.max(0.35, view.scale * (ev.deltaY < 0 ? 1.1 : 0.9))) };
  }

  // ---------- list fallback ----------
  async function tapZone(z: Zone) {
    const t = tasks.find((x) => x.id === picked);
    if (!t) return;
    picked = null;
    await dropped(t, z.x + 10, z.y + 44);
  }

  const memberOf = (id: string | null | undefined) => session.members.find((m) => m.membership_id === id);
  const dayFmt = new Intl.DateTimeFormat("de-DE", { day: "numeric", month: "numeric" });
</script>

<div class="board-wrap" bind:clientWidth={width}>
  {#if error}<p class="err small" role="alert">{error}</p>{/if}
  {#if width >= 600}
    <p class="muted small">{planner ? "Karten in eine Person ziehen = zuweisen, in einen Tag = einplanen. Fläche schieben, mit zwei Fingern oder dem Mausrad zoomen." : "Das Board zeigt, wer was macht. Ändern können Erwachsene."}</p>
    <div class="canvas" role="application" aria-label="Board" onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up} onwheel={wheel}>
      <div class="world" style="transform: translate({view.x}px, {view.y}px) scale({view.scale})">
        {#each zones as z (z.key)}
          {@const m = memberOf(z.member)}
          <div class="zone {z.kind}" class:today={z.kind === "day" && sameDay(z.day!, taskStore.now)} style="left:{z.x}px;top:{z.y}px;width:{z.w}px;height:{z.h}px;--zc:{m?.color ?? 'var(--muted)'}">
            <span class="zlabel">
              {#if m}<Avatar animal={m.animal as AnimalName} color={m.color} size={24} />{/if}
              {z.label}{#if z.kind === "day"} <span class="muted">{dayFmt.format(new Date(z.day!))}</span>{/if}
            </span>
          </div>
        {/each}
        {#each placed as p (p.t.id)}
          <div
            class="card-item"
            class:dragging={drag?.id === p.t.id}
            class:movable={planner}
            role="button"
            tabindex="0"
            aria-label={p.t.title}
            style="left:{p.x}px;top:{p.y}px;width:{CARD_W}px;height:{CARD_H}px;--c:{memberOf(p.t.assignee_id)?.color ?? 'var(--muted)'}"
            onpointerdown={(ev) => cardDown(ev, p.t, p.x, p.y)}
            onpointermove={cardMove}
            onpointerup={() => cardUp(p.t)}
          >
            <Icon name={iconOf(p.t.icon)} size={16} />
            <span class="ctitle">{p.t.title}</span>
            <span class="xp-chip small">{p.t.xp}</span>
          </div>
        {/each}
      </div>
    </div>
  {:else}
    <p class="muted small">{planner ? (picked ? "Jetzt eine Person oder einen Tag antippen." : "Karte antippen, dann Person oder Tag antippen. Im Querformat gibt es das Board als Fläche.") : "Wer macht was diese Woche."}</p>
    <div class="lists">
      {#each zones as z (z.key)}
        {@const m = memberOf(z.member)}
        {@const here = placed.filter((p) => zoneAt(p.x + CARD_W / 2, p.y + CARD_H / 2)?.key === z.key)}
        <section class="lzone" class:target={picked !== null} style="--zc:{m?.color ?? 'var(--muted)'}">
          <button class="lhead" disabled={!picked} onclick={() => tapZone(z)}>
            {#if m}<Avatar animal={m.animal as AnimalName} color={m.color} size={22} />{/if}
            <b>{z.kind === "day" ? `${z.label} ${dayFmt.format(new Date(z.day!))}` : z.label}</b>
            <span class="muted small">{here.length}</span>
          </button>
          {#each here as p (p.t.id)}
            <button class="litem" class:sel={picked === p.t.id} disabled={!planner} onclick={() => (picked = picked === p.t.id ? null : p.t.id)}>
              <Icon name={iconOf(p.t.icon)} size={14} />{p.t.title}<span class="muted small">{p.t.xp} XP</span>
            </button>
          {/each}
        </section>
      {/each}
    </div>
  {/if}
</div>

<style>
  .board-wrap {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .canvas {
    position: relative;
    height: min(70vh, 640px);
    overflow: hidden;
    border-radius: var(--radius);
    border: 1px solid var(--line);
    background:
      radial-gradient(circle, color-mix(in srgb, var(--muted) 25%, transparent) 1px, transparent 1px) 0 0 / 22px 22px,
      var(--surface-2);
    touch-action: none;
    cursor: grab;
  }

  .world {
    position: absolute;
    left: 0;
    top: 0;
    transform-origin: 0 0;
  }

  .zone {
    position: absolute;
    border-radius: 18px;
    border: 2px dashed color-mix(in srgb, var(--zc) 55%, var(--line));
    background: color-mix(in srgb, var(--zc) 6%, transparent);
  }

  .zone.day {
    border-style: solid;
    border-width: 1px;
  }

  .zone.today {
    border-color: var(--primary);
  }

  .zlabel {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    font-weight: 800;
    color: var(--fg);
  }

  .card-item {
    position: absolute;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border-radius: 12px;
    border: 1px solid var(--line);
    border-left: 5px solid var(--c);
    background: var(--surface);
    box-shadow: 0 2px 6px rgb(0 0 0 / 0.12);
    font-size: 0.85rem;
    font-weight: 700;
    user-select: none;
    touch-action: none;
  }

  .card-item.movable {
    cursor: grab;
  }

  .card-item.dragging {
    z-index: 5;
    cursor: grabbing;
    box-shadow: 0 10px 24px rgb(0 0 0 / 0.25);
    transform: rotate(-2deg);
  }

  .ctitle {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .lists {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .lzone {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--line);
    border-left: 4px solid var(--zc);
    background: var(--surface);
  }

  .lzone.target .lhead:not(:disabled) {
    background: var(--primary-soft);
  }

  .lhead {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    background: transparent;
    border: none;
    color: var(--fg);
    text-align: left;
  }

  .lhead:disabled {
    opacity: 1;
  }

  .litem {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 38px;
    font-size: 0.9rem;
    text-align: left;
  }

  .litem:disabled {
    opacity: 1;
  }

  .litem.sel {
    background: var(--primary-soft);
    border-color: var(--primary);
    color: var(--primary);
  }

  .err {
    color: var(--over);
  }
</style>
