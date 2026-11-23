<script lang="ts">
  // Plan · Kalender (KONZEPT §7.3): day, week, month, agenda. Every entry is a PlanNode in the
  // colour of whoever is on it; coming rounds of recurring tasks are shown dashed.
  // Planning and moving: drag an entry (or one from "Noch ohne Termin") onto a day or an hour;
  // with a finger: press a moment, then drag. Without dragging: tap a task in the tray, then
  // a day. Dropping on "Ohne Uhrzeit" takes it out of the calendar again.
  import { goto } from "$app/navigation";
  import type { CalendarEntry, TaskDto } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { iconOf, taskStore } from "$lib/tasks.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import PlanNode, { type NodeState } from "./PlanNode.svelte";
  import { DAY, WEEKDAYS, addDays, sameDay, startOfDay, startOfWeek } from "./views";

  /** Restrict to these tasks (saved view); null = everything. */
  let { only = null }: { only?: TaskDto[] | null } = $props();

  type Mode = "day" | "week" | "month" | "agenda";
  const MODE_KEY = "fairteilt.calendar";
  let mode = $state<Mode>(readMode());
  let anchor = $state(startOfDay(Date.now()));
  let entries = $state<CalendarEntry[]>([]);
  let picked = $state<string | null>(null);
  let busy = $state<string | null>(null);
  let error = $state("");

  function readMode(): Mode {
    try {
      const m = localStorage.getItem(MODE_KEY);
      return m === "day" || m === "month" || m === "agenda" ? m : "week";
    } catch {
      return "week";
    }
  }

  function setMode(m: Mode) {
    mode = m;
    try {
      localStorage.setItem(MODE_KEY, m);
    } catch {
      /* not remembered */
    }
  }

  const hid = $derived(session.membership?.household_id);
  const me = $derived(session.membership?.membership_id);
  const planner = $derived(session.membership?.role !== "child");
  const today = $derived(startOfDay(taskStore.now));
  const HOURS = Array.from({ length: 17 }, (_, i) => i + 6);

  const range = $derived.by((): [number, number] => {
    if (mode === "day") return [anchor, addDays(anchor, 1)];
    if (mode === "week") return [startOfWeek(anchor), addDays(startOfWeek(anchor), 7)];
    if (mode === "month") {
      const d = new Date(anchor);
      const first = startOfWeek(new Date(d.getFullYear(), d.getMonth(), 1).getTime());
      return [first, addDays(first, 42)];
    }
    return [today, addDays(today, 30)];
  });

  async function load() {
    if (!hid) return;
    const [from, to] = range;
    const q = `from=${encodeURIComponent(new Date(from).toISOString())}&to=${encodeURIComponent(new Date(to).toISOString())}`;
    entries = await session.request<CalendarEntry[]>("GET", `/households/${hid}/calendar?${q}`);
  }

  $effect(() => {
    void range;
    void taskStore.version;
    load().catch((e) => (error = e.message));
  });

  const allowed = $derived(only ? new Set(only.map((t) => t.id)) : null);
  const allowedTemplates = $derived(only ? new Set(only.map((t) => t.template_id).filter(Boolean)) : null);
  const shown = $derived(
    entries.filter((e) => !allowed || (e.task_id && allowed.has(e.task_id)) || (e.template_id && allowedTemplates!.has(e.template_id))),
  );

  /** When an entry sits in the calendar: planned time, done time, or due date. */
  const when = (e: CalendarEntry) => Date.parse(e.scheduled_start ?? e.completed_at ?? e.due_at ?? new Date().toISOString());
  const isOverdue = (e: CalendarEntry) =>
    !e.projected && e.status !== "completed" && !!e.due_at && Date.parse(e.due_at) < taskStore.now && (!e.scheduled_start || Date.parse(e.scheduled_start) < today);
  /** Entries of a day; overdue ones without a plan also show up today. */
  const onDay = (day: number) =>
    shown
      .filter((e) => sameDay(when(e), day) || (day === today && isOverdue(e) && !e.scheduled_start && !sameDay(when(e), day)))
      .sort((a, b) => when(a) - when(b));
  const keyOf = (e: CalendarEntry) => e.task_id ?? `${e.template_id}${e.due_at}`;

  /** Tasks without a time yet (planners: all, others: their own). */
  const plannable = $derived(
    taskStore.tasks.filter(
      (t) => !t.scheduled_start && t.status !== "submitted" && (planner || t.assignee_id === me) && (!allowed || allowed.has(t.id)),
    ),
  );

  const memberOf = (id: string | null) => session.members.find((m) => m.membership_id === id);
  const time = new Intl.DateTimeFormat("de-DE", { hour: "2-digit", minute: "2-digit" });
  const dayName = new Intl.DateTimeFormat("de-DE", { weekday: "long", day: "numeric", month: "long" });
  const monthName = new Intl.DateTimeFormat("de-DE", { month: "long", year: "numeric" });
  const dayMonthShort = new Intl.DateTimeFormat("de-DE", { day: "numeric", month: "short" });
  const weekdayLong = new Intl.DateTimeFormat("de-DE", { weekday: "long" });
  const dayMonth = new Intl.DateTimeFormat("de-DE", { day: "numeric", month: "long" });

  function stateOf(e: CalendarEntry): NodeState {
    if (e.projected) return "projected";
    if (e.status === "completed") return "done";
    if (isOverdue(e)) return "late";
    if (e.status === "in_progress") return "running";
    if (e.status === "submitted") return "submitted";
    return "open";
  }

  function metaOf(e: CalendarEntry, withTime: boolean): string {
    const at = e.scheduled_start && withTime ? time.format(new Date(e.scheduled_start)) : "";
    const s = stateOf(e);
    const word =
      s === "projected" ? "kommt" : s === "done" ? "erledigt" : s === "late" ? "überfällig" : s === "running" ? "läuft" : s === "submitted" ? "wartet aufs OK" : e.scheduled_start ? "" : "fällig";
    return [at, word].filter(Boolean).join(" · ");
  }

  /** Can this entry be moved by me? */
  const movable = (e: CalendarEntry) =>
    !!e.task_id && !e.projected && e.status !== "completed" && e.status !== "submitted" && (planner || e.assignee_id === me);

  // ---------- planning ----------
  /** Drop keys: "d:<day ms>" keeps the time (or 17:00), "h:<ms>" is an exact hour, "loose" unplans. */
  function targetTime(key: string, current: string | null): number | null {
    if (key.startsWith("h:")) return Number(key.slice(2));
    if (key.startsWith("d:")) {
      const at = new Date(Number(key.slice(2)));
      const keep = current ? new Date(current) : null;
      at.setHours(keep?.getHours() ?? 17, keep?.getMinutes() ?? 0, 0, 0);
      return at.getTime();
    }
    return null;
  }

  async function plan(taskId: string, key: string) {
    const current = taskStore.tasks.find((t) => t.id === taskId)?.scheduled_start ?? entries.find((e) => e.task_id === taskId)?.scheduled_start ?? null;
    const at = targetTime(key, current);
    if (at !== null && current && Date.parse(current) === at) return;
    error = "";
    busy = taskId;
    try {
      await session.request("PUT", `/tasks/${taskId}/schedule`, { scheduled_start: at === null ? null : new Date(at).toISOString(), scheduled_end: null });
      feedback("step");
      picked = null;
      taskStore.scheduleReload();
      await load();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = null;
    }
  }

  const dragOf = (taskId: string | null, enabled: boolean) => ({ enabled: enabled && !!taskId, ondrop: (key: string) => taskId && void plan(taskId, key) });

  /** Tap on a day or hour while a task from the tray is picked. */
  function tapTarget(key: string) {
    if (picked) void plan(picked, key);
  }

  function step(n: number) {
    const d = new Date(anchor);
    if (mode === "day") anchor = addDays(anchor, n);
    else if (mode === "week") anchor = addDays(anchor, 7 * n);
    else if (mode === "month") anchor = new Date(d.getFullYear(), d.getMonth() + n, 1).getTime();
  }

  /** "23. – 29. Nov." or "30. Nov. – 6. Dez." */
  function weekTitle(a: number, b: number): string {
    const da = new Date(a);
    const db = new Date(b);
    return da.getMonth() === db.getMonth() ? `${da.getDate()}. – ${dayMonthShort.format(db)}` : `${dayMonthShort.format(da)} – ${dayMonthShort.format(db)}`;
  }

  function openDay(day: number) {
    anchor = day;
    setMode("day");
  }

  const title = $derived(
    mode === "day"
      ? dayName.format(new Date(anchor))
      : mode === "week"
        ? weekTitle(range[0], range[1] - DAY)
        : mode === "month"
          ? monthName.format(new Date(anchor))
          : "Die nächsten 30 Tage",
  );
  const isNow = $derived(
    mode === "day" ? sameDay(anchor, today) : mode === "week" ? sameDay(startOfWeek(anchor), startOfWeek(today)) : mode === "month" ? new Date(anchor).getMonth() === new Date(today).getMonth() && new Date(anchor).getFullYear() === new Date(today).getFullYear() : true,
  );
  const nowLine = $derived.by(() => {
    const d = new Date(taskStore.now);
    const h = d.getHours() + d.getMinutes() / 60;
    return h >= HOURS[0] && h < HOURS[HOURS.length - 1] + 1 ? h : null;
  });
</script>

<div class="cal" class:picking={picked !== null}>
  <div class="seg" role="tablist" aria-label="Zeitraum">
    {#each [["day", "Tag"], ["week", "Woche"], ["month", "Monat"], ["agenda", "Agenda"]] as [m, label] (m)}
      <button role="tab" aria-selected={mode === m} class:sel={mode === m} onclick={() => setMode(m as Mode)}>{label}</button>
    {/each}
  </div>

  <div class="nav">
    {#if mode !== "agenda"}
      <button class="round" aria-label="Zurück" onclick={() => step(-1)}><Icon name="back" size={18} /></button>
    {/if}
    <h2 class="title">{title}</h2>
    {#if mode !== "agenda"}
      <button class="round" aria-label="Weiter" onclick={() => step(1)}><Icon name="chevron" size={18} /></button>
      <button class="today-btn" disabled={isNow} onclick={() => (anchor = today)}>Heute</button>
    {/if}
  </div>

  {#if plannable.length && mode !== "agenda"}
    <section class="tray" aria-label="Noch ohne Termin">
      <div class="tray-head">
        <b>Noch ohne Termin</b>
        <span class="muted small">{picked ? "Jetzt einen Tag antippen" : planner ? "Ziehen oder antippen" : "Deine Aufgaben, ziehen oder antippen"}</span>
        {#if picked}<button class="link" onclick={() => (picked = null)}>Abbrechen</button>{/if}
      </div>
      <div class="tray-list">
        {#each plannable as t (t.id)}
          <div class="tray-item" class:sel={picked === t.id}>
            <PlanNode
              size="s"
              title={t.title}
              icon={iconOf(t.icon)}
              member={memberOf(t.assignee_id)}
              label="{t.title} einplanen"
              drag={dragOf(t.id, true)}
              onopen={() => (picked = picked === t.id ? null : t.id)}
            />
          </div>
        {/each}
      </div>
    </section>
  {/if}
  {#if error}<p class="err small" role="alert">{error}</p>{/if}

  {#snippet node(e: CalendarEntry, withTime: boolean, size: "s" | "m" = "m")}
    <PlanNode
      {size}
      title={e.title}
      icon={iconOf(e.icon)}
      member={memberOf(e.assignee_id) ?? undefined}
      xp={e.xp}
      state={stateOf(e)}
      meta={metaOf(e, withTime)}
      drag={dragOf(e.task_id, movable(e) && busy !== e.task_id)}
      onopen={e.task_id ? () => goto(`/aufgabe/${e.task_id}`) : undefined}
      label={e.projected ? `${e.title}, kommende Runde` : e.title}
    />
  {/snippet}

  {#if mode === "day"}
    {@const list = onDay(anchor)}
    {@const loose = list.filter((e) => !e.scheduled_start || !sameDay(Date.parse(e.scheduled_start), anchor))}
    <section class="loose drop" data-drop="loose" aria-label="Ohne Uhrzeit">
      <span class="label">Ohne Uhrzeit</span>
      {#each loose as e (keyOf(e))}{@render node(e, false)}{:else}<span class="muted small hint">Hierher ziehen nimmt es aus dem Kalender.</span>{/each}
    </section>
    <div class="hours">
      {#each HOURS as h (h)}
        {@const at = new Date(new Date(anchor).setHours(h, 0, 0, 0)).getTime()}
        {@const here = list.filter((e) => e.scheduled_start && sameDay(Date.parse(e.scheduled_start), anchor) && new Date(e.scheduled_start).getHours() === h)}
        <div class="hour drop" data-drop="h:{at}" role="button" tabindex="-1" aria-label="{h}:00 Uhr" onclick={() => tapTarget(`h:${at}`)} onkeydown={() => {}}>
          <span class="h">{String(h).padStart(2, "0")}:00</span>
          <div class="slot">{#each here as e (keyOf(e))}{@render node(e, true)}{/each}</div>
        </div>
      {/each}
      {#if sameDay(anchor, today) && nowLine !== null}
        <span class="now" style="--at: {nowLine - HOURS[0]}" aria-hidden="true"></span>
      {/if}
    </div>
  {:else if mode === "week"}
    <div class="week">
      {#each Array.from({ length: 7 }, (_, i) => addDays(range[0], i)) as day (day)}
        {@const list = onDay(day)}
        {@const past = day < today}
        <section class="day drop" class:today={sameDay(day, today)} class:past data-drop="d:{day}" aria-label={dayName.format(new Date(day))}>
          <button class="badge" onclick={() => (picked ? tapTarget(`d:${day}`) : openDay(day))} aria-label={picked ? `Auf ${dayName.format(new Date(day))} legen` : `${dayName.format(new Date(day))} öffnen`}>
            <span class="wd">{WEEKDAYS[(new Date(day).getDay() + 6) % 7]}</span>
            <span class="num">{new Date(day).getDate()}</span>
          </button>
          <div class="items" role="presentation" onclick={(ev) => ev.target === ev.currentTarget && tapTarget(`d:${day}`)}>
            {#each list as e (keyOf(e))}{@render node(e, true)}{:else}<span class="free">{picked ? "hier einplanen" : "frei"}</span>{/each}
          </div>
        </section>
      {/each}
    </div>
  {:else if mode === "month"}
    <div class="month">
      {#each WEEKDAYS as d (d)}<span class="mwd">{d}</span>{/each}
      {#each Array.from({ length: 42 }, (_, i) => addDays(range[0], i)) as day (day)}
        {@const list = shown.filter((e) => sameDay(when(e), day))}
        <button
          class="cell drop"
          class:other={new Date(day).getMonth() !== new Date(anchor).getMonth()}
          class:today={sameDay(day, today)}
          data-drop="d:{day}"
          aria-label="{dayName.format(new Date(day))}, {list.length} Einträge"
          onclick={() => (picked ? tapTarget(`d:${day}`) : openDay(day))}
        >
          <span class="n">{new Date(day).getDate()}</span>
          <span class="bars">
            {#each list.slice(0, 3) as e, i (i)}
              <i class={stateOf(e)} style="--c: {memberOf(e.assignee_id)?.color ?? 'var(--muted)'}"><span>{e.title}</span></i>
            {/each}
            {#if list.length > 3}<small>+{list.length - 3}</small>{/if}
          </span>
        </button>
      {/each}
    </div>
  {:else}
    <div class="agenda">
      {#each Array.from({ length: 30 }, (_, i) => addDays(range[0], i)) as day (day)}
        {@const list = onDay(day)}
        {#if list.length}
          <section class="aday drop" data-drop="d:{day}">
            <h3>
              <span class="aname">{sameDay(day, today) ? "Heute" : sameDay(day, addDays(today, 1)) ? "Morgen" : weekdayLong.format(new Date(day))}</span>
              <span class="muted">{dayMonth.format(new Date(day))}</span>
            </h3>
            <div class="items">{#each list as e (keyOf(e))}{@render node(e, true)}{/each}</div>
          </section>
        {/if}
      {:else}
        <p class="muted">Nichts geplant.</p>
      {/each}
    </div>
  {/if}

  <div class="legend" aria-label="Legende">
    <span><i class="lg projected"></i>kommt noch</span>
    <span><i class="lg late"></i>überfällig</span>
    <span><i class="lg done"></i>erledigt</span>
    <span class="muted">Farbe = wer dran ist</span>
  </div>
</div>

<style>
  .cal {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .seg button {
    font-size: 0.85rem;
  }

  /* ---- header ---- */
  .nav {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .title {
    flex: 1;
    min-width: 0;
    text-align: center;
    font-size: 1.15rem;
    font-weight: 800;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .round {
    flex: none;
    width: 40px;
    min-height: 40px;
    padding: 0;
    border-radius: 50%;
    display: grid;
    place-items: center;
  }

  .today-btn {
    flex: none;
    min-height: 40px;
    padding: 0 14px;
    border-radius: 999px;
    background: var(--primary-soft);
    color: var(--primary);
    font-size: 0.85rem;
  }

  .today-btn:disabled {
    opacity: 1;
    background: var(--surface-2);
    color: var(--muted);
  }

  .link {
    min-height: 30px;
    padding: 0 6px;
    background: transparent;
    color: var(--primary);
    font-size: 0.85rem;
  }

  /* ---- tray ---- */
  .tray {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border-radius: var(--radius);
    background: var(--surface-2);
  }

  .tray-head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px var(--space-2);
  }

  .tray-head b {
    font-family: var(--font-display);
  }

  .tray-head .link {
    margin-left: auto;
  }

  .tray-list {
    display: flex;
    gap: var(--space-2);
    overflow-x: auto;
    padding-bottom: 2px;
    scrollbar-width: thin;
  }

  .tray-item {
    flex: none;
    width: 190px;
    border-radius: 13px;
  }

  .tray-item.sel {
    outline: 3px solid var(--primary);
    outline-offset: 1px;
  }

  /* ---- drop targets ---- */
  .drop {
    transition: background 0.15s, border-color 0.15s, box-shadow 0.15s;
  }

  :global(body.is-dragging) .drop,
  .picking .drop {
    box-shadow: inset 0 0 0 2px color-mix(in srgb, var(--primary) 25%, transparent);
  }

  .drop:global([data-over]) {
    background: var(--primary-soft) !important;
    box-shadow: inset 0 0 0 2px var(--primary) !important;
  }

  /* ---- week ---- */
  .week {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .day {
    display: grid;
    grid-template-columns: 52px 1fr;
    gap: var(--space-3);
    padding: var(--space-2);
    border-radius: var(--radius);
    border: 1px solid var(--line);
    background: var(--surface);
  }

  .day.past {
    background: color-mix(in srgb, var(--surface) 55%, var(--bg));
  }

  .day.today {
    border-color: color-mix(in srgb, var(--primary) 50%, var(--line));
  }

  .badge {
    align-self: start;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    min-height: 0;
    padding: 6px 0;
    background: transparent;
    border-radius: 12px;
  }

  .badge .wd {
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .badge .num {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-family: var(--font-display);
    font-size: 1.2rem;
    font-weight: 800;
  }

  .day.today .badge .num {
    background: var(--primary);
    color: var(--primary-fg);
  }

  .day.today .badge .wd {
    color: var(--primary);
  }

  .items {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    min-height: 48px;
    justify-content: center;
  }

  .free {
    display: grid;
    place-items: center;
    min-height: 44px;
    border-radius: 12px;
    border: 1.5px dashed var(--line);
    color: var(--muted);
    font-size: 0.85rem;
  }

  /* ---- day ---- */
  .loose {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: var(--space-3);
    border-radius: var(--radius);
    background: var(--surface-2);
  }

  .label {
    font-size: 0.75rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .hint {
    padding: 4px 0;
  }

  .hours {
    position: relative;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    overflow: hidden;
    background: var(--surface);
    --row: 52px;
  }

  .hour {
    display: grid;
    grid-template-columns: 52px 1fr;
    min-height: var(--row);
    border-top: 1px solid var(--line);
  }

  .hour:first-child {
    border-top: none;
  }

  .h {
    padding: 6px 0 0 10px;
    font-size: 0.75rem;
    font-weight: 700;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .slot {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 4px 6px 4px 0;
    min-width: 0;
  }

  .now {
    position: absolute;
    left: 46px;
    right: 0;
    top: calc(var(--at) * var(--row));
    height: 2px;
    background: var(--over);
    pointer-events: none;
  }

  .now::before {
    content: "";
    position: absolute;
    left: -5px;
    top: -4px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--over);
  }

  /* ---- month ---- */
  .month {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    gap: 4px;
  }

  .mwd {
    text-align: center;
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .cell {
    min-width: 0;
    min-height: 76px;
    padding: 5px 3px 4px;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 3px;
    border-radius: 12px;
    background: var(--surface);
    border: 1px solid var(--line);
    font-weight: 400;
  }

  .cell.other {
    background: transparent;
    opacity: 0.5;
  }

  .n {
    align-self: center;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-size: 0.82rem;
    font-weight: 800;
    font-variant-numeric: tabular-nums;
  }

  .cell.today .n {
    background: var(--primary);
    color: var(--primary-fg);
  }

  .bars {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .bars i {
    display: block;
    min-width: 0;
    height: 15px;
    padding: 0 4px;
    border-radius: 5px;
    background: color-mix(in srgb, var(--c) 20%, var(--surface));
    border-left: 3px solid var(--c);
    font-style: normal;
    font-size: 0.66rem;
    font-weight: 700;
    line-height: 15px;
    text-align: left;
    overflow: hidden;
  }

  .bars i span {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bars i.projected {
    background: transparent;
    border: 1px dashed color-mix(in srgb, var(--c) 50%, var(--line));
    color: var(--muted);
  }

  .bars i.late {
    background: var(--over-soft);
    border-left-color: var(--over);
    color: var(--over);
  }

  .bars i.done span {
    text-decoration: line-through;
    color: var(--muted);
  }

  .bars small {
    font-size: 0.66rem;
    font-weight: 700;
    color: var(--muted);
    text-align: center;
  }

  @media (max-width: 420px) {
    .cell {
      min-height: 62px;
    }

    .bars i {
      height: 7px;
      padding: 0;
      border-left-width: 0;
      background: var(--c);
    }

    .bars i span {
      display: none;
    }

    .bars i.projected {
      background: transparent;
    }
  }

  /* ---- agenda ---- */
  .agenda {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .aday {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: var(--space-2) 0;
    border-radius: var(--radius-sm);
  }

  .aday h3 {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    font-size: 1rem;
  }

  .aname {
    font-weight: 800;
  }

  .aday h3 .muted {
    font-family: var(--font-body);
    font-size: 0.85rem;
    font-weight: 400;
  }

  /* ---- legend ---- */
  .legend {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 4px 14px;
    font-size: 0.8rem;
  }

  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .lg {
    display: inline-block;
    width: 16px;
    height: 12px;
    border-radius: 4px;
  }

  .lg.projected {
    border: 1.5px dashed var(--muted);
  }

  .lg.late {
    background: var(--over-soft);
    border: 1.5px solid var(--over);
  }

  .lg.done {
    background: var(--ok-soft);
    border: 1.5px solid var(--ok);
  }

  .err {
    color: var(--over);
  }
</style>
