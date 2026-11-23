<script lang="ts">
  // "Wann?" for one task: pick a day (and a time) and put it into the calendar. For shopping
  // and other tasks "auf Abruf" the chosen day is also when it is due (server, plan.rs).
  import { untrack } from "svelte";
  import type { TaskDto } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { taskStore } from "$lib/tasks.svelte";
  import { addDays, sameDay, startOfDay } from "./views";

  let { task, onclose }: { task: TaskDto; onclose: () => void } = $props();

  const today = startOfDay(Date.now());
  // The sheet starts from the task as it is when opened.
  const first = untrack(() => ({ planned: task.scheduled_start, shop: !!task.list_id }));
  const initial = first.planned ? new Date(first.planned) : null;
  const pad = (n: number) => String(n).padStart(2, "0");
  const isoDay = (t: number) => {
    const d = new Date(t);
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  };

  let day = $state(initial ? startOfDay(initial.getTime()) : first.shop ? addDays(today, 1) : today);
  let time = $state(initial ? `${pad(initial.getHours())}:${pad(initial.getMinutes())}` : first.shop ? "10:00" : "17:00");
  let busy = $state(false);
  let error = $state("");

  const days = Array.from({ length: 8 }, (_, i) => addDays(today, i));
  const wd = new Intl.DateTimeFormat("de-DE", { weekday: "short" });
  const full = new Intl.DateTimeFormat("de-DE", { weekday: "long", day: "numeric", month: "long" });
  const TIMES = [
    ["Morgens", "08:00"],
    ["Vormittag", "10:00"],
    ["Mittag", "12:00"],
    ["Nachmittag", "15:00"],
    ["Abend", "18:00"],
  ] as const;

  const shopping = $derived(!!task.list_id);
  const onCall = $derived(task.on_call);
  const at = $derived.by(() => {
    const [h, m] = time.split(":").map(Number);
    const d = new Date(day);
    d.setHours(h || 0, m || 0, 0, 0);
    return d;
  });

  async function save(start: Date | null) {
    busy = true;
    error = "";
    try {
      await session.request("PUT", `/tasks/${task.id}/schedule`, { scheduled_start: start?.toISOString() ?? null, scheduled_end: null });
      feedback("step");
      taskStore.scheduleReload();
      onclose();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  function pickDate(v: string) {
    const [y, mo, d] = v.split("-").map(Number);
    if (y && mo && d) day = new Date(y, mo - 1, d).getTime();
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="sheet stack" role="dialog" aria-modal="true" aria-labelledby="plan-title">
    <h2 id="plan-title">{shopping ? "Wann wird eingekauft?" : `Wann: ${task.title}?`}</h2>

    <div class="days" role="group" aria-label="Tag">
      {#each days as d (d)}
        <button class="day" class:sel={sameDay(d, day)} aria-pressed={sameDay(d, day)} onclick={() => (day = d)}>
          <span class="wd">{d === today ? "Heute" : d === addDays(today, 1) ? "Morgen" : wd.format(new Date(d))}</span>
          <span class="num">{new Date(d).getDate()}</span>
        </button>
      {/each}
    </div>
    <label class="field other">
      <span>Anderer Tag</span>
      <input type="date" min={isoDay(today)} value={isoDay(day)} onchange={(e) => pickDate(e.currentTarget.value)} />
    </label>

    <div class="times" role="group" aria-label="Uhrzeit">
      {#each TIMES as [label, t] (t)}
        <button class="chip" class:sel={time === t} aria-pressed={time === t} onclick={() => (time = t)}>{label}<small>{t}</small></button>
      {/each}
      <label class="clock"><span class="sr">Uhrzeit</span><input type="time" bind:value={time} step="900" /></label>
    </div>

    <p class="summary">
      <b>{full.format(at)}, {time} Uhr</b>
      {#if onCall}<br /><span class="muted small">{shopping ? "Einkaufen" : `„${task.title}“`} ist dann an diesem Tag fällig.</span>{/if}
    </p>
    {#if error}<p class="err" role="alert">{error}</p>{/if}

    <div class="row actions">
      {#if task.scheduled_start}
        <button class="ghost" disabled={busy} onclick={() => save(null)}>Termin entfernen</button>
      {/if}
      <button disabled={busy} onclick={onclose}>Abbrechen</button>
      <button class="primary" disabled={busy} onclick={() => save(at)}>{task.scheduled_start ? "Verschieben" : "Einplanen"}</button>
    </div>
  </div>
</div>

<style>
  h2 {
    font-size: 1.25rem;
  }

  .days {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
  }

  .day {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    min-height: 60px;
    padding: 6px 0;
    border-radius: 14px;
    background: var(--surface);
    border: 1px solid var(--line);
    font-weight: 400;
  }

  .day .wd {
    font-size: 0.75rem;
    font-weight: 700;
    color: var(--muted);
  }

  .day .num {
    font-family: var(--font-display);
    font-size: 1.25rem;
    font-weight: 800;
  }

  .day.sel {
    background: var(--primary);
    border-color: var(--primary);
    color: var(--primary-fg);
  }

  .day.sel .wd {
    color: inherit;
  }

  .other input {
    font-weight: 400;
  }

  .times {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }

  .chip {
    display: inline-flex;
    align-items: baseline;
    gap: 5px;
    min-height: 38px;
    padding: 0 12px;
    border-radius: 999px;
    background: var(--surface);
    border: 1px solid var(--line);
    font-size: 0.85rem;
  }

  .chip small {
    color: var(--muted);
    font-weight: 400;
  }

  .chip.sel {
    background: var(--primary-soft);
    border-color: var(--primary);
    color: var(--primary);
  }

  .chip.sel small {
    color: inherit;
  }

  .clock input {
    min-height: 38px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: var(--surface);
    font-variant-numeric: tabular-nums;
  }

  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }

  .summary {
    margin: 0;
    padding: var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }

  .actions {
    justify-content: flex-end;
    flex-wrap: wrap;
  }

  .ghost {
    margin-right: auto;
    background: transparent;
    color: var(--over);
  }

  .err {
    color: var(--over);
  }
</style>
