<script lang="ts">
  // Plan · Kalender (KONZEPT §7.3): day, week, month, agenda. Colour = member. Coming rounds of
  // recurring tasks are shown faded. Planning: drag a task onto a slot, or (touch) tap the task,
  // then the slot.
  import { goto } from "$app/navigation";
  import type { CalendarEntry, TaskDto } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { iconOf, taskStore } from "$lib/tasks.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import { DAY, WEEKDAYS, addDays, sameDay, startOfDay, startOfWeek } from "./views";

  /** Restrict to these tasks (saved view); null = everything. */
  let { only = null }: { only?: TaskDto[] | null } = $props();

  type Mode = "day" | "week" | "month" | "agenda";
  let mode = $state<Mode>("week");
  let anchor = $state(startOfDay(Date.now()));
  let entries = $state<CalendarEntry[]>([]);
  let picked = $state<string | null>(null);
  let error = $state("");

  const hid = $derived(session.membership?.household_id);
  const me = $derived(session.membership?.membership_id);
  const planner = $derived(session.membership?.role !== "child");
  const HOURS = Array.from({ length: 17 }, (_, i) => i + 6);

  const range = $derived.by((): [number, number] => {
    if (mode === "day") return [anchor, addDays(anchor, 1)];
    if (mode === "week") return [startOfWeek(anchor), addDays(startOfWeek(anchor), 7)];
    if (mode === "month") {
      const d = new Date(anchor);
      const first = startOfWeek(new Date(d.getFullYear(), d.getMonth(), 1).getTime());
      return [first, addDays(first, 42)];
    }
    return [startOfDay(Date.now()), addDays(startOfDay(Date.now()), 30)];
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
  const onDay = (day: number) => shown.filter((e) => sameDay(when(e), day) || (day === startOfDay(Date.now()) && isOverdue(e)));
  const isOverdue = (e: CalendarEntry) =>
    !e.projected && e.status !== "completed" && !e.scheduled_start && !!e.due_at && Date.parse(e.due_at) < startOfDay(Date.now());

  /** Tasks that can still be planned: current rounds without a time (planners: all, others: own). */
  const plannable = $derived(
    taskStore.tasks.filter(
      (t) => !t.scheduled_start && !["submitted"].includes(t.status) && (planner || t.assignee_id === me) && (!allowed || allowed.has(t.id)),
    ),
  );

  const colorOf = (id: string | null) => session.members.find((m) => m.membership_id === id)?.color ?? "var(--muted)";
  const time = new Intl.DateTimeFormat("de-DE", { hour: "2-digit", minute: "2-digit" });
  const dayName = new Intl.DateTimeFormat("de-DE", { weekday: "long", day: "numeric", month: "long" });
  const monthName = new Intl.DateTimeFormat("de-DE", { month: "long", year: "numeric" });
  const shortDay = new Intl.DateTimeFormat("de-DE", { weekday: "short", day: "numeric", month: "short" });

  async function plan(taskId: string, at: number) {
    error = "";
    try {
      await session.request("PUT", `/tasks/${taskId}/schedule`, { scheduled_start: new Date(at).toISOString(), scheduled_end: null });
      feedback("step");
      picked = null;
      taskStore.scheduleReload();
      await load();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function unplan(taskId: string) {
    await session.request("PUT", `/tasks/${taskId}/schedule`, { scheduled_start: null, scheduled_end: null }).catch(() => {});
    taskStore.scheduleReload();
    await load();
  }

  /** Slot target: drop or tap with a picked task. Week/month rows plan at 17:00. */
  function slot(at: number) {
    return {
      ondragover: (ev: DragEvent) => ev.preventDefault(),
      ondrop: (ev: DragEvent) => {
        ev.preventDefault();
        const id = ev.dataTransfer?.getData("text/task");
        if (id) void plan(id, at);
      },
      onclick: () => picked && void plan(picked, at),
    };
  }

  function step(n: number) {
    const d = new Date(anchor);
    if (mode === "day") anchor = addDays(anchor, n);
    else if (mode === "week") anchor = addDays(anchor, 7 * n);
    else if (mode === "month") anchor = new Date(d.getFullYear(), d.getMonth() + n, 1).getTime();
  }

  const title = $derived(
    mode === "day"
      ? dayName.format(new Date(anchor))
      : mode === "week"
        ? `${shortDay.format(new Date(range[0]))} – ${shortDay.format(new Date(range[1] - DAY))}`
        : mode === "month"
          ? monthName.format(new Date(anchor))
          : "Die nächsten 30 Tage",
  );
</script>

<div class="cal">
  <div class="seg modes" role="tablist" aria-label="Zeitraum">
    {#each [["day", "Tag"], ["week", "Woche"], ["month", "Monat"], ["agenda", "Agenda"]] as [m, label] (m)}
      <button role="tab" aria-selected={mode === m} class:sel={mode === m} onclick={() => (mode = m as Mode)}>{label}</button>
    {/each}
  </div>

  <div class="row nav">
    {#if mode !== "agenda"}<button class="icon-btn" aria-label="Zurück" onclick={() => step(-1)}><Icon name="back" size={18} /></button>{/if}
    <strong class="title">{title}</strong>
    {#if mode !== "agenda"}
      <button class="icon-btn" aria-label="Weiter" onclick={() => step(1)}><Icon name="chevron" size={18} /></button>
      <button class="today" onclick={() => (anchor = startOfDay(Date.now()))}>Heute</button>
    {/if}
  </div>

  {#if plannable.length && mode !== "agenda" && mode !== "month"}
    <div class="tray" aria-label="Noch nicht eingeplant">
      <span class="muted small">Noch ohne Zeit · {picked ? "jetzt einen Platz antippen" : "ziehen oder antippen"}</span>
      <div class="chips">
        {#each plannable as t (t.id)}
          <button
            class="chip"
            class:sel={picked === t.id}
            draggable="true"
            style="--c: {colorOf(t.assignee_id)}"
            ondragstart={(ev) => ev.dataTransfer?.setData("text/task", t.id)}
            onclick={() => (picked = picked === t.id ? null : t.id)}
          >
            <Icon name={iconOf(t.icon)} size={14} />{t.title}
          </button>
        {/each}
      </div>
    </div>
  {/if}
  {#if error}<p class="err small" role="alert">{error}</p>{/if}

  {#snippet item(e: CalendarEntry, withTime: boolean)}
    <button
      class="entry"
      class:projected={e.projected}
      class:done={e.status === "completed"}
      class:late={isOverdue(e)}
      style="--c: {colorOf(e.assignee_id)}"
      disabled={!e.task_id}
      onclick={() => e.task_id && goto(`/aufgabe/${e.task_id}`)}
    >
      <Icon name={e.status === "completed" ? "check" : iconOf(e.icon)} size={14} />
      <span class="name">{e.title}</span>
      {#if withTime && e.scheduled_start}<span class="muted small">{time.format(new Date(e.scheduled_start))}</span>{/if}
      {#if e.projected}<span class="muted small">geplant</span>{:else if isOverdue(e)}<span class="small">überfällig</span>{/if}
    </button>
  {/snippet}

  {#if mode === "day"}
    {@const list = onDay(anchor)}
    {@const loose = list.filter((e) => !e.scheduled_start)}
    {#if loose.length}
      <div class="loose">{#each loose as e (e.task_id ?? `${e.template_id}${e.due_at}`)}{@render item(e, false)}{/each}</div>
    {/if}
    <div class="hours">
      {#each HOURS as h (h)}
        {@const at = new Date(new Date(anchor).setHours(h, 0, 0, 0)).getTime()}
        {@const here = list.filter((e) => e.scheduled_start && new Date(e.scheduled_start).getHours() === h)}
        <div class="hour" class:target={picked !== null} role="button" tabindex="-1" aria-label="{h}:00 Uhr" {...slot(at)}>
          <span class="h muted">{h}:00</span>
          <div class="slot-items">
            {#each here as e (e.task_id)}
              <span class="planned">
                {@render item(e, true)}
                {#if e.task_id && (planner || e.assignee_id === me)}<button class="x" aria-label="Aus dem Kalender nehmen" onclick={(ev) => (ev.stopPropagation(), unplan(e.task_id!))}><Icon name="close" size={12} /></button>{/if}
              </span>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {:else if mode === "week"}
    <div class="week">
      {#each Array.from({ length: 7 }, (_, i) => addDays(range[0], i)) as day (day)}
        {@const at = new Date(new Date(day).setHours(17, 0, 0, 0)).getTime()}
        <div class="weekday" class:today={sameDay(day, Date.now())} class:target={picked !== null} role="button" tabindex="-1" aria-label={shortDay.format(new Date(day))} {...slot(at)}>
          <button class="dlabel" onclick={(ev) => (ev.stopPropagation(), (anchor = day), (mode = "day"))}>
            <b>{WEEKDAYS[(new Date(day).getDay() + 6) % 7]}</b><span>{new Date(day).getDate()}.</span>
          </button>
          <div class="items">
            {#each onDay(day) as e (e.task_id ?? `${e.template_id}${e.due_at}`)}{@render item(e, true)}{:else}<span class="muted small free">frei</span>{/each}
          </div>
        </div>
      {/each}
    </div>
  {:else if mode === "month"}
    <div class="month">
      {#each WEEKDAYS as d (d)}<span class="wd muted small">{d}</span>{/each}
      {#each Array.from({ length: 42 }, (_, i) => addDays(range[0], i)) as day (day)}
        {@const list = shown.filter((e) => sameDay(when(e), day))}
        <button class="cell" class:other={new Date(day).getMonth() !== new Date(anchor).getMonth()} class:today={sameDay(day, Date.now())}
          onclick={() => ((anchor = day), (mode = "day"))}>
          <span class="n">{new Date(day).getDate()}</span>
          <span class="dots">
            {#each list.slice(0, 4) as e, i (i)}<i class:projected={e.projected} style="--c: {colorOf(e.assignee_id)}"></i>{/each}
            {#if list.length > 4}<small>+{list.length - 4}</small>{/if}
          </span>
        </button>
      {/each}
    </div>
  {:else}
    <div class="agenda">
      {#each Array.from({ length: 30 }, (_, i) => addDays(range[0], i)) as day (day)}
        {@const list = onDay(day)}
        {#if list.length}
          <h3 class="aday">{sameDay(day, Date.now()) ? "Heute" : shortDay.format(new Date(day))}</h3>
          <div class="items">{#each list as e (e.task_id ?? `${e.template_id}${e.due_at}`)}{@render item(e, true)}{/each}</div>
        {/if}
      {:else}
        <p class="muted">Nichts geplant.</p>
      {/each}
    </div>
  {/if}
  <p class="muted small legend"><span class="lg projected"></span> kommende Runden, aus dem Rhythmus berechnet · Farbe = wer dran ist</p>
</div>

<style>
  .cal {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .modes button {
    font-size: 0.85rem;
  }

  .nav {
    gap: var(--space-2);
  }

  .title {
    flex: 1;
    text-align: center;
  }

  .icon-btn {
    width: 36px;
    min-height: 36px;
    padding: 0;
    display: grid;
    place-items: center;
  }

  .today {
    min-height: 36px;
    padding: 0 10px;
    font-size: 0.85rem;
  }

  .tray {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .chips {
    display: flex;
    gap: 6px;
    overflow-x: auto;
    padding-bottom: 4px;
  }

  .chip {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-height: 34px;
    padding: 0 10px;
    border-radius: 999px;
    border: 1px solid var(--line);
    border-left: 4px solid var(--c);
    background: var(--surface);
    font-size: 0.85rem;
    cursor: grab;
  }

  .chip.sel {
    background: var(--primary-soft);
    border-color: var(--primary);
    color: var(--primary);
  }

  .entry {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    min-height: 34px;
    padding: 4px 8px;
    border: 1px solid var(--line);
    border-left: 4px solid var(--c);
    border-radius: 10px;
    background: var(--surface);
    color: var(--fg);
    text-align: left;
    font-size: 0.85rem;
    font-weight: 600;
  }

  .entry:disabled {
    opacity: 1;
    cursor: default;
  }

  .entry .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .entry.projected {
    border-style: dashed;
    border-left-style: solid;
    opacity: 0.6;
    font-weight: 400;
  }

  .entry.done .name {
    text-decoration: line-through;
    color: var(--muted);
  }

  .entry.late {
    border-color: color-mix(in srgb, var(--over) 50%, var(--line));
    color: var(--over);
  }

  .loose {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .hours {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    overflow: hidden;
    background: var(--surface);
  }

  .hour {
    display: grid;
    grid-template-columns: 48px 1fr;
    min-height: 44px;
    border-top: 1px solid var(--line);
  }

  .hour:first-child {
    border-top: none;
  }

  .hour.target,
  .weekday.target {
    background: color-mix(in srgb, var(--primary) 5%, transparent);
    cursor: copy;
  }

  .h {
    padding: 6px;
    font-size: 0.75rem;
    font-variant-numeric: tabular-nums;
  }

  .slot-items {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 4px 6px 4px 0;
  }

  .planned {
    display: flex;
    gap: 4px;
  }

  .x {
    width: 30px;
    min-height: 30px;
    padding: 0;
    display: grid;
    place-items: center;
    border-radius: 50%;
  }

  .week {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .weekday {
    display: grid;
    grid-template-columns: 52px 1fr;
    gap: var(--space-2);
    padding: 6px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    border: 1px solid var(--line);
  }

  .weekday.today {
    border-color: var(--primary);
  }

  .dlabel {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 44px;
    padding: 0;
    background: transparent;
    border: none;
    color: var(--fg);
  }

  .items {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .free {
    align-self: center;
    padding-top: 10px;
  }

  .month {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 3px;
  }

  .wd {
    text-align: center;
  }

  .cell {
    min-height: 54px;
    padding: 4px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    border-radius: 10px;
    background: var(--surface);
    border: 1px solid var(--line);
    color: var(--fg);
  }

  .cell.other {
    opacity: 0.45;
  }

  .cell.today {
    border-color: var(--primary);
    color: var(--primary);
  }

  .dots {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 2px;
    align-items: center;
  }

  .dots i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--c);
  }

  .dots i.projected {
    opacity: 0.4;
  }

  .dots small {
    font-size: 0.65rem;
  }

  .agenda {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .aday {
    font-size: 0.9rem;
    margin-top: var(--space-2);
  }

  .legend {
    text-align: center;
  }

  .lg {
    display: inline-block;
    width: 18px;
    height: 10px;
    border: 1px dashed var(--muted);
    border-radius: 3px;
    vertical-align: middle;
  }

  .err {
    color: var(--over);
  }
</style>
