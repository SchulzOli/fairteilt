<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import ConditionBar from "$lib/ui/ConditionBar.svelte";
  import { members, rooms, tasks } from "$lib/demo";

  type View = "rooms" | "calendar" | "board";
  let view = $state<View>("rooms");

  // Rooms: condition of the most urgent task, XP sum.
  const roomRows = $derived(
    rooms.map((r) => {
      const ts = tasks.filter((t) => t.room === r.id);
      const worst = ts.reduce((m, t) => Math.max(m, t.condition), 0);
      const overdue = ts.filter((t) => t.condition >= 1).length;
      const dueSoon = ts.filter((t) => t.condition >= 0.75 && t.condition < 1).length;
      const hint = overdue ? `${overdue} überfällig` : dueSoon ? `${dueSoon} bald fällig` : "alles im grünen Bereich";
      return { ...r, count: ts.length, worst, xp: ts.reduce((s, t) => s + t.xp, 0), hint };
    }),
  );

  // Calendar placeholder (M6): week strip and the day's tasks.
  const days = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
  const todayIndex = (new Date().getDay() + 6) % 7;
  let selectedDay = $state(todayIndex);

  // Board placeholder (M6): zones per member, tap card then zone to assign.
  let assignment = $state<Record<string, string | undefined>>(Object.fromEntries(tasks.map((t) => [t.id, t.assignee])));
  let picked = $state<string | null>(null);
  const zones = [
    { name: undefined as string | undefined, label: "Offen", color: "var(--muted)" },
    ...members.map((m) => ({ name: m.name as string | undefined, label: m.name, color: m.color })),
  ];
  function drop(zone: string | undefined) {
    if (!picked) return;
    assignment[picked] = zone;
    picked = null;
  }
  const memberOf = (name?: string) => members.find((m) => m.name === name);
</script>

<header class="stack tight">
  <h1 class="page-title">Plan</h1>
  <div class="seg" role="tablist" aria-label="Ansicht">
    <button role="tab" aria-selected={view === "rooms"} class:sel={view === "rooms"} onclick={() => (view = "rooms")}>Räume</button>
    <button role="tab" aria-selected={view === "calendar"} class:sel={view === "calendar"} onclick={() => (view = "calendar")}>Kalender</button>
    <button role="tab" aria-selected={view === "board"} class:sel={view === "board"} onclick={() => (view = "board")}>Board</button>
  </div>
</header>

{#if view === "rooms"}
  <div class="stack">
    {#each roomRows as r (r.id)}
      <article class="card room">
        <span class="ic" aria-hidden="true"><Icon name={r.icon} size={22} /></span>
        <div>
          <h2>{r.name}</h2>
          <div class="muted small">{r.count} Aufgaben · {r.hint}</div>
        </div>
        <span class="xp-chip">{r.xp} XP</span>
        <div class="cond"><ConditionBar value={r.worst} /></div>
      </article>
    {/each}
  </div>
  <p class="muted small hint">„Bad putzen“ läuft alle 7 Tage ab dem letzten Mal. Wer früher putzt, setzt den Balken zurück.</p>
{:else if view === "calendar"}
  <div class="week" role="tablist" aria-label="Wochentag">
    {#each days as d, i (d)}
      <button role="tab" aria-selected={selectedDay === i} class:sel={selectedDay === i} class:today={i === todayIndex} onclick={() => (selectedDay = i)}>{d}</button>
    {/each}
  </div>
  <div class="stack">
    {#each tasks.slice(0, 5) as t, i (t.id)}
      {@const m = memberOf(t.assignee)}
      <div class="slot">
        <span class="time muted">{8 + i * 3}:00</span>
        <div class="event" style="border-left-color: {m?.color ?? 'var(--muted)'}">
          <strong>{t.title}</strong>
          <span class="muted small">{t.assignee ?? "offen"} · {t.xp} XP</span>
        </div>
      </div>
    {/each}
  </div>
  <p class="muted small hint">Vorschau. Tag, Woche, Monat und Ziehen zum Einplanen folgen in M6.</p>
{:else}
  <p class="muted small">Karte antippen, dann eine Zone antippen = zuweisen. Freies Canvas mit Zoom folgt in M6.</p>
  <div class="zones">
    {#each zones as z (z.label)}
      {@const m = memberOf(z.name)}
      <div class="zone" class:target={picked !== null} style="--zone: {z.color}" onclick={() => drop(z.name)}
        onkeydown={(e) => e.key === "Enter" && drop(z.name)} role="button" tabindex="0" aria-label="Zone {z.label}">
        <h2 class="row">{#if m}<Avatar animal={m.animal} color={m.color} size={24} />{/if}{z.label}</h2>
        <div class="cards">
          {#each tasks.filter((t) => assignment[t.id] === z.name) as t (t.id)}
            <button class="mini" class:picked={picked === t.id} onclick={(e) => { e.stopPropagation(); picked = picked === t.id ? null : t.id; }}>
              <span>{t.title}</span><span class="muted">{t.xp}</span>
            </button>
          {/each}
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .tight {
    gap: var(--space-3);
    margin-bottom: var(--space-4);
  }

  .small {
    font-size: 0.85rem;
  }

  .hint {
    margin-top: var(--space-4);
    text-align: center;
  }

  .seg,
  .week {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 4px;
    padding: 4px;
    border-radius: 14px;
    background: var(--surface-2);
  }

  .week {
    grid-template-columns: repeat(7, 1fr);
    margin-bottom: var(--space-4);
  }

  .seg button,
  .week button {
    min-height: 36px;
    padding: 0;
    background: transparent;
    color: var(--muted);
    border-radius: 10px;
  }

  .seg button.sel,
  .week button.sel {
    background: var(--surface);
    color: var(--fg);
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.12);
  }

  .week button.today {
    color: var(--primary);
  }

  .room {
    display: grid;
    grid-template-columns: 44px 1fr auto;
    gap: var(--space-2) var(--space-3);
    align-items: center;
  }

  .room h2 {
    font-size: 1.05rem;
  }

  .ic {
    width: 44px;
    height: 44px;
    border-radius: 15px;
    background: var(--surface-2);
    display: grid;
    place-items: center;
  }

  .cond {
    grid-column: 2 / -1;
  }

  .slot {
    display: grid;
    grid-template-columns: 52px 1fr;
    gap: var(--space-3);
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
    border: 1px solid var(--line);
    border-left: 4px solid;
    border-radius: var(--radius-sm);
  }

  .zones {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: var(--space-3);
    margin-top: var(--space-3);
  }

  .zone {
    min-height: 140px;
    padding: var(--space-3);
    border: 2px dashed color-mix(in srgb, var(--zone) 50%, var(--line));
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--zone) 6%, var(--surface));
  }

  .zone.target {
    border-style: solid;
  }

  .zone h2 {
    font-size: 0.95rem;
    color: var(--zone);
    margin-bottom: var(--space-2);
    gap: var(--space-2);
  }

  .cards {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .mini {
    display: flex;
    justify-content: space-between;
    gap: var(--space-2);
    text-align: left;
    font-size: 0.85rem;
    background: var(--surface);
    border: 1px solid var(--line);
    padding: var(--space-2);
    min-height: 40px;
  }

  .mini.picked {
    outline: 2px solid var(--primary);
  }
</style>
