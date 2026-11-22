<script lang="ts">
  import { members, tasks } from "$lib/demo";

  // Placeholder week strip; the real calendar (day/week/month/agenda, drag to schedule) comes in M6.
  const days = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
  const todayIndex = (new Date().getDay() + 6) % 7;
  let selected = $state(todayIndex);

  const colorOf = (name?: string) => members.find((m) => m.name === name)?.color ?? "var(--text-muted)";
</script>

<header>
  <h1>Kalender</h1>
</header>

<div class="week" role="tablist" aria-label="Wochentag">
  {#each days as d, i (d)}
    <button role="tab" aria-selected={selected === i} class:sel={selected === i} class:today={i === todayIndex}
      onclick={() => (selected = i)}>
      {d}
    </button>
  {/each}
</div>

<div class="timeline">
  {#each tasks.slice(0, 4) as t, i (t.id)}
    <div class="slot">
      <span class="time muted">{8 + i * 3}:00</span>
      <div class="event" style="border-left-color: {colorOf(t.assignee)}">
        <strong>{t.icon} {t.title}</strong>
        <span class="muted small">{t.assignee ?? "offen"} · {t.points} P</span>
      </div>
    </div>
  {/each}
</div>

<p class="muted small hint">Vorschau. Tag-, Wochen- und Monatsansicht mit Ziehen zum Einplanen folgen in M6.</p>

<style>
  header {
    margin-bottom: var(--space-4);
  }

  h1 {
    font-size: 1.9rem;
  }

  .week {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: var(--space-1);
    margin-bottom: var(--space-4);
  }

  .week button {
    padding: 0;
    background: var(--surface);
    border: 1px solid var(--border);
    font-weight: 600;
  }

  .week button.today {
    color: var(--accent);
  }

  .week button.sel {
    background: var(--accent);
    color: var(--accent-text);
    border-color: var(--accent);
  }

  .timeline {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .slot {
    display: grid;
    grid-template-columns: 52px 1fr;
    gap: var(--space-3);
    align-items: start;
  }

  .time {
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
    padding-top: var(--space-2);
  }

  .event {
    display: flex;
    flex-direction: column;
    padding: var(--space-3);
    background: var(--surface);
    border: 1px solid var(--border);
    border-left: 4px solid;
    border-radius: var(--radius-sm);
  }

  .small {
    font-size: 0.85rem;
  }

  .hint {
    margin-top: var(--space-5);
    text-align: center;
  }
</style>
