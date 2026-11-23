<script lang="ts">
  // Plan · Board (KONZEPT §7.3): a free canvas with entries and zones. An entry dropped into a
  // person's zone is assigned, into a day of this week is planned (keeps its time, else 17:00),
  // anywhere else it just stays there. Pan with one finger / the mouse, zoom with two fingers,
  // the wheel or the buttons. Narrow screens get the zones as a list: drag an entry onto a zone
  // (press a moment with a finger), or tap it, then the zone.
  import { goto } from "$app/navigation";
  import type { BoardItemDto, TaskDto } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { STATUS_LABEL, conditionOf, iconOf, statusOf, taskStore } from "$lib/tasks.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import type { AnimalName } from "$lib/ui/Avatar.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import PlanNode, { type NodeState } from "./PlanNode.svelte";
  import { WEEKDAYS, addDays, sameDay, startOfWeek } from "./views";

  let { only = null }: { only?: TaskDto[] | null } = $props();

  const CARD_W = 236;
  const CARD_H = 50;
  const GAP = 8;
  const HEAD = 56;
  const PZ_W = 268;
  const PZ_H = 340;
  const DZ_W = 260;
  const DZ_H = 230;
  const DAY_Y = PZ_H + 70;
  const MIN = 0.35;
  const MAX = 1.6;

  let items = $state<BoardItemDto[]>([]);
  let width = $state(800);
  let canvasEl = $state<HTMLElement | null>(null);
  // Starts readable at the people; "Alles zeigen" (the % button) fits the whole board.
  let view = $state({ x: 16, y: 40, scale: 0.8 });
  let drag = $state<{ id: string; dx: number; dy: number; x: number; y: number; moved: boolean } | null>(null);
  let overZone = $state<string | null>(null);
  let picked = $state<string | null>(null);
  let error = $state("");

  const hid = $derived(session.membership?.household_id);
  const planner = $derived(session.membership?.role !== "child");
  const tasks = $derived((only ?? taskStore.tasks).filter((t) => t.status !== "submitted"));
  const week = $derived(startOfWeek(taskStore.now));

  type Zone = { key: string; kind: "person" | "day"; label: string; x: number; y: number; w: number; h: number; member?: string | null; day?: number };
  const zones = $derived.by((): Zone[] => {
    const people: Zone[] = session.members.map((m, i) => ({
      key: m.membership_id,
      kind: "person" as const,
      label: m.display_name,
      member: m.membership_id,
      x: i * (PZ_W + 24),
      y: 0,
      w: PZ_W,
      h: PZ_H,
    }));
    people.push({ key: "open", kind: "person", label: "Offen", member: null, x: people.length * (PZ_W + 24), y: 0, w: PZ_W, h: PZ_H });
    const days: Zone[] = WEEKDAYS.map((d, i) => ({ key: `d${i}`, kind: "day", label: d, day: addDays(week, i), x: i * (DZ_W + 16), y: DAY_Y, w: DZ_W, h: DZ_H }));
    return [...people, ...days];
  });
  const world = $derived({
    w: Math.max(...zones.map((z) => z.x + z.w)),
    h: Math.max(...zones.map((z) => z.y + z.h)),
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

  /** Where each entry is: its saved place if it still fits, else stacked in its zone. */
  const placed = $derived.by(() => {
    const counts = new Map<string, number>();
    return tasks.map((t) => {
      const saved = items.find((i) => (t.template_id ? i.template_id === t.template_id : i.task_id === t.id));
      if (drag?.id === t.id) return { t, x: drag.x, y: drag.y };
      if (saved && consistent(t, saved.x, saved.y)) return { t, x: saved.x, y: saved.y };
      const thisWeek = t.scheduled_start && zones.find((z) => z.kind === "day" && sameDay(Date.parse(t.scheduled_start!), z.day!));
      const z = thisWeek || zones.find((z) => z.kind === "person" && z.member === t.assignee_id) || zones.find((z) => z.key === "open")!;
      const n = counts.get(z.key) ?? 0;
      counts.set(z.key, n + 1);
      return { t, x: z.x + (z.w - CARD_W) / 2, y: z.y + HEAD + n * (CARD_H + GAP) };
    });
  });
  const inZone = (z: Zone) => placed.filter((p) => zoneAt(p.x + CARD_W / 2, p.y + CARD_H / 2)?.key === z.key);

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

  function stateOf(t: TaskDto): NodeState {
    if (t.status === "in_progress") return "running";
    return conditionOf(t, taskStore.now) >= 1 ? "late" : "open";
  }
  const metaOf = (t: TaskDto) => (t.list_id && t.status === "in_progress" ? "kauft ein" : t.due_at ? STATUS_LABEL[statusOf(conditionOf(t, taskStore.now))] : "kein Termin");
  const memberOf = (id: string | null | undefined) => session.members.find((m) => m.membership_id === id);
  const dayFmt = new Intl.DateTimeFormat("de-DE", { day: "numeric", month: "numeric" });
  const canMove = (t: TaskDto) => planner && ["open", "claimed"].includes(t.status);

  // ---------- canvas: entry drag, pan, pinch, zoom ----------
  const pointers = new Map<number, { x: number; y: number }>();
  let pan: { x: number; y: number; vx: number; vy: number } | null = null;
  let pinch: { d: number; scale: number; cx: number; cy: number; vx: number; vy: number } | null = null;

  function cardDown(ev: PointerEvent, t: TaskDto, x: number, y: number) {
    ev.stopPropagation();
    if (!canMove(t)) {
      drag = { id: "", dx: ev.clientX, dy: ev.clientY, x, y, moved: false };
      return;
    }
    (ev.currentTarget as HTMLElement).setPointerCapture(ev.pointerId);
    drag = { id: t.id, dx: ev.clientX / view.scale - x, dy: ev.clientY / view.scale - y, x, y, moved: false };
  }

  function cardMove(ev: PointerEvent) {
    if (!drag?.id) return;
    const x = ev.clientX / view.scale - drag.dx;
    const y = ev.clientY / view.scale - drag.dy;
    const moved = drag.moved || Math.hypot(x - drag.x, y - drag.y) * view.scale > 4;
    drag = { ...drag, x, y, moved };
    overZone = zoneAt(x + CARD_W / 2, y + CARD_H / 2)?.key ?? null;
  }

  function cardUp(t: TaskDto) {
    const d = drag;
    drag = null;
    overZone = null;
    if (!d) return;
    if (!d.moved) return void goto(`/aufgabe/${t.id}`);
    void dropped(t, Math.round(d.x), Math.round(d.y));
  }

  function down(ev: PointerEvent) {
    pointers.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
    (ev.currentTarget as HTMLElement).setPointerCapture(ev.pointerId);
    if (pointers.size === 1) pan = { x: ev.clientX, y: ev.clientY, vx: view.x, vy: view.y };
    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      const r = canvasEl!.getBoundingClientRect();
      pinch = { d: Math.hypot(a.x - b.x, a.y - b.y), scale: view.scale, cx: (a.x + b.x) / 2 - r.left, cy: (a.y + b.y) / 2 - r.top, vx: view.x, vy: view.y };
      pan = null;
    }
  }

  function move(ev: PointerEvent) {
    if (!pointers.has(ev.pointerId)) return;
    pointers.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
    if (pinch && pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      const scale = clamp((pinch.scale * Math.hypot(a.x - b.x, a.y - b.y)) / pinch.d);
      // Zoom around the point between the fingers.
      const k = scale / pinch.scale;
      view = { scale, x: pinch.cx - (pinch.cx - pinch.vx) * k, y: pinch.cy - (pinch.cy - pinch.vy) * k };
    } else if (pan) {
      view = { ...view, x: pan.vx + ev.clientX - pan.x, y: pan.vy + ev.clientY - pan.y };
    }
  }

  function up(ev: PointerEvent) {
    pointers.delete(ev.pointerId);
    if (pointers.size < 2) pinch = null;
    if (pointers.size === 0) pan = null;
  }

  const clamp = (s: number) => Math.min(MAX, Math.max(MIN, s));

  /** Zoom by a factor around a point of the canvas (default: its centre). */
  function zoom(f: number, cx?: number, cy?: number) {
    const r = canvasEl?.getBoundingClientRect();
    const px = cx ?? (r ? r.width / 2 : 0);
    const py = cy ?? (r ? r.height / 2 : 0);
    const scale = clamp(view.scale * f);
    const k = scale / view.scale;
    view = { scale, x: px - (px - view.x) * k, y: py - (py - view.y) * k };
  }

  function wheel(ev: WheelEvent) {
    ev.preventDefault();
    const r = canvasEl!.getBoundingClientRect();
    zoom(ev.deltaY < 0 ? 1.1 : 0.9, ev.clientX - r.left, ev.clientY - r.top);
  }

  /** Everything in view. */
  function fit() {
    const r = canvasEl?.getBoundingClientRect();
    if (!r) return;
    const scale = clamp(Math.min((r.width - 32) / world.w, (r.height - 32) / world.h));
    view = { scale, x: (r.width - world.w * scale) / 2, y: 16 + 30 * scale };
  }


  // ---------- list (narrow screens) ----------
  async function toZone(t: TaskDto, z: Zone) {
    picked = null;
    await dropped(t, z.x + (z.w - CARD_W) / 2, z.y + HEAD);
  }

  function tapZone(z: Zone) {
    const t = tasks.find((x) => x.id === picked);
    if (t) void toZone(t, z);
  }

  const listDrag = (t: TaskDto) => ({
    enabled: canMove(t),
    ondrop: (key: string) => {
      const z = zones.find((x) => x.key === key);
      if (z) void toZone(t, z);
    },
  });
</script>

<div class="board-wrap" bind:clientWidth={width}>
  {#if error}<p class="err small" role="alert">{error}</p>{/if}
  {#if width >= 600}
    <p class="muted small">
      {planner ? "Ziehen: in eine Person = zuweisen, in einen Tag = einplanen. Fläche schieben, zoomen mit zwei Fingern, Mausrad oder den Knöpfen." : "Das Board zeigt, wer was macht. Ändern können Erwachsene."}
    </p>
    <div
      class="canvas"
      bind:this={canvasEl}
      role="application"
      aria-label="Board"
      onpointerdown={down}
      onpointermove={move}
      onpointerup={up}
      onpointercancel={up}
      onwheel={wheel}
    >
      <div class="world" style="transform: translate({view.x}px, {view.y}px) scale({view.scale})">
        <span class="row-label" style="top: -2px">Wer</span>
        <span class="row-label" style="top: {DAY_Y - 34}px">Diese Woche</span>
        {#each zones as z (z.key)}
          {@const m = memberOf(z.member)}
          {@const count = inZone(z).length}
          <div
            class="zone {z.kind}"
            class:open-zone={z.key === "open"}
            class:today={z.kind === "day" && sameDay(z.day!, taskStore.now)}
            class:over={overZone === z.key}
            style="left:{z.x}px;top:{z.y}px;width:{z.w}px;height:{z.h}px;--zc:{m?.color ?? 'var(--muted)'}"
          >
            <span class="zhead">
              {#if m}
                <Avatar animal={m.animal as AnimalName} color={m.color} size={32} />
              {:else if z.kind === "person"}
                <span class="zopen" aria-hidden="true"><Icon name="family" size={18} /></span>
              {:else}
                <span class="zday" aria-hidden="true">{new Date(z.day!).getDate()}</span>
              {/if}
              <span class="zname">{z.kind === "day" ? `${z.label}, ${dayFmt.format(new Date(z.day!))}` : z.label}</span>
              <span class="zcount">{count}</span>
            </span>
          </div>
        {/each}
        {#each placed as p (p.t.id)}
          <div
            class="entry"
            class:dragging={drag?.id === p.t.id && drag.moved}
            class:movable={canMove(p.t)}
            role="button"
            tabindex="0"
            aria-label={p.t.title}
            style="left:{p.x}px;top:{p.y}px;width:{CARD_W}px;height:{CARD_H}px"
            onpointerdown={(ev) => cardDown(ev, p.t, p.x, p.y)}
            onpointermove={cardMove}
            onpointerup={() => cardUp(p.t)}
            onkeydown={(ev) => ev.key === "Enter" && goto(`/aufgabe/${p.t.id}`)}
          >
            <PlanNode title={p.t.title} icon={iconOf(p.t.icon)} member={memberOf(p.t.assignee_id)} xp={p.t.xp} state={stateOf(p.t)} meta={metaOf(p.t)} />
          </div>
        {/each}
      </div>
      <div class="zoom" role="group" aria-label="Zoom">
        <button aria-label="Verkleinern" onpointerdown={(ev) => ev.stopPropagation()} onclick={() => zoom(0.8)}>−</button>
        <button aria-label="Alles zeigen" onpointerdown={(ev) => ev.stopPropagation()} onclick={fit}>{Math.round(view.scale * 100)} %</button>
        <button aria-label="Vergrößern" onpointerdown={(ev) => ev.stopPropagation()} onclick={() => zoom(1.25)}>+</button>
      </div>
    </div>
  {:else}
    <p class="muted small">
      {planner
        ? picked
          ? "Jetzt eine Person oder einen Tag antippen."
          : "Eintrag auf eine Person oder einen Tag ziehen (kurz drücken, dann ziehen) oder antippen und dann das Ziel antippen. Quer gehalten gibt es das Board als Fläche."
        : "Wer macht was diese Woche."}
    </p>
    <div class="lists">
      {#each zones as z (z.key)}
        {@const m = memberOf(z.member)}
        {@const here = inZone(z)}
        {#if z.kind === "day" && z.key === "d0"}<h3 class="section-title">Diese Woche</h3>{/if}
        <section class="lzone drop {z.kind}" class:today={z.kind === "day" && sameDay(z.day!, taskStore.now)} data-drop={z.key} style="--zc:{m?.color ?? 'var(--muted)'}">
          <button class="lhead" disabled={!picked} onclick={() => tapZone(z)}>
            {#if m}
              <Avatar animal={m.animal as AnimalName} color={m.color} size={28} />
            {:else if z.kind === "person"}
              <span class="zopen" aria-hidden="true"><Icon name="family" size={16} /></span>
            {:else}
              <span class="zday" aria-hidden="true">{new Date(z.day!).getDate()}</span>
            {/if}
            <b>{z.kind === "day" ? `${z.label}, ${dayFmt.format(new Date(z.day!))}` : z.label}</b>
            <span class="zcount">{here.length}</span>
          </button>
          {#each here as p (p.t.id)}
            <div class="litem" class:sel={picked === p.t.id}>
              <PlanNode
                title={p.t.title}
                icon={iconOf(p.t.icon)}
                member={memberOf(p.t.assignee_id)}
                xp={p.t.xp}
                state={stateOf(p.t)}
                meta={metaOf(p.t)}
                drag={listDrag(p.t)}
                onopen={() => (planner && canMove(p.t) ? (picked = picked === p.t.id ? null : p.t.id) : goto(`/aufgabe/${p.t.id}`))}
              />
            </div>
          {:else}
            <span class="lempty">{picked ? "hierher" : "–"}</span>
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

  /* ---- canvas ---- */
  .canvas {
    position: relative;
    height: min(72vh, 660px);
    overflow: hidden;
    border-radius: var(--radius);
    border: 1px solid var(--line);
    background:
      radial-gradient(circle, color-mix(in srgb, var(--muted) 22%, transparent) 1.2px, transparent 1.2px) 0 0 / 24px 24px,
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

  .row-label {
    position: absolute;
    left: 4px;
    font-size: 0.85rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
    white-space: nowrap;
    transform: translateY(-100%);
  }

  .zone {
    position: absolute;
    border-radius: 22px;
    border: 2px solid color-mix(in srgb, var(--zc) 35%, var(--line));
    background: color-mix(in srgb, var(--zc) 7%, var(--surface));
    transition: border-color 0.15s, background 0.15s, box-shadow 0.15s;
  }

  .zone.open-zone {
    border-style: dashed;
    border-color: var(--line);
    background: color-mix(in srgb, var(--surface) 60%, transparent);
  }

  .zone.day {
    border-width: 1px;
    border-color: var(--line);
    background: var(--surface);
  }

  .zone.today {
    border: 2px solid var(--primary);
  }

  .zone.over {
    border-color: var(--primary);
    background: var(--primary-soft);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--primary) 18%, transparent);
  }

  .zhead {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 48px;
    padding: 8px 12px 0;
  }

  .zname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-display);
    font-size: 1.05rem;
    font-weight: 800;
  }

  .zcount {
    min-width: 24px;
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--surface-2);
    font-size: 0.78rem;
    font-weight: 700;
    text-align: center;
    color: var(--muted);
  }

  .zopen,
  .zday {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    flex: none;
  }

  .zopen {
    border: 2px dashed var(--line);
    color: var(--muted);
  }

  .zday {
    background: var(--surface-2);
    font-family: var(--font-display);
    font-weight: 800;
  }

  .zone.today .zday,
  .lzone.today .zday {
    background: var(--primary);
    color: var(--primary-fg);
  }

  .entry {
    position: absolute;
    border-radius: 14px;
    touch-action: none;
    cursor: pointer;
  }

  .entry.movable {
    cursor: grab;
  }

  .entry :global(.node) {
    height: 100%;
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.08);
  }

  .entry.dragging {
    z-index: 5;
    cursor: grabbing;
  }

  .entry.dragging :global(.node) {
    border-color: var(--primary);
    box-shadow: 0 14px 30px rgb(0 0 0 / 0.25);
    transform: rotate(-2deg);
  }

  .zoom {
    position: absolute;
    right: 10px;
    bottom: 10px;
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 14px;
    background: var(--surface);
    border: 1px solid var(--line);
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.1);
  }

  .zoom button {
    min-width: 38px;
    min-height: 36px;
    padding: 0 8px;
    border-radius: 11px;
    background: transparent;
    font-size: 1rem;
    font-variant-numeric: tabular-nums;
  }

  .zoom button:nth-child(2) {
    font-size: 0.8rem;
    color: var(--muted);
  }

  /* ---- list ---- */
  .lists {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .lists .section-title {
    margin: var(--space-3) 0 0;
  }

  .lzone {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: var(--space-2);
    border-radius: var(--radius);
    border: 1px solid color-mix(in srgb, var(--zc) 30%, var(--line));
    background: color-mix(in srgb, var(--zc) 6%, var(--surface));
    transition: background 0.15s, box-shadow 0.15s;
  }

  .lzone.day {
    border-color: var(--line);
    background: var(--surface);
  }

  .lzone.today {
    border-color: color-mix(in srgb, var(--primary) 50%, var(--line));
  }

  :global(body.is-dragging) .lzone {
    box-shadow: inset 0 0 0 2px color-mix(in srgb, var(--primary) 25%, transparent);
  }

  .lzone:global([data-over]) {
    background: var(--primary-soft);
    box-shadow: inset 0 0 0 2px var(--primary);
  }

  .lhead {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 40px;
    padding: 0 4px;
    background: transparent;
    color: var(--fg);
    text-align: left;
  }

  .lhead b {
    flex: 1;
    font-family: var(--font-display);
    font-size: 1rem;
  }

  .lhead:disabled {
    opacity: 1;
    cursor: default;
  }

  .lhead:not(:disabled) {
    background: var(--primary-soft);
  }

  .litem {
    border-radius: 15px;
  }

  .litem.sel {
    outline: 3px solid var(--primary);
    outline-offset: 1px;
  }

  .lempty {
    padding: 2px 6px 4px;
    color: var(--muted);
    font-size: 0.85rem;
  }

  .err {
    color: var(--over);
  }
</style>
