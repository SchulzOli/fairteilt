<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import ConditionBar from "$lib/ui/ConditionBar.svelte";
  import { members, tasks } from "$lib/demo";
  import TaskCard from "$lib/TaskCard.svelte";
  import TaskSheet from "$lib/TaskSheet.svelte";
  import { session } from "$lib/session.svelte";
  import { TASK_ICONS, conditionOf, iconOf, taskStore } from "$lib/tasks.svelte";
  import type { IconName } from "$lib/ui/Icon.svelte";

  type View = "rooms" | "calendar" | "board";
  let view = $state<View>("rooms");
  let openRoom = $state<string | null>(null);
  let sheet = $state<{ roomId?: string } | null>(null);
  let adding = $state(false);
  let roomName = $state("");
  let roomIcon = $state<IconName>("sofa");
  let error = $state("");

  const planner = $derived(session.membership?.role !== "child");

  // Rooms: condition of the most urgent task (like Tody), XP sum, tasks by urgency.
  const roomRows = $derived.by(() => {
    const now = taskStore.now;
    const groups = [...taskStore.rooms.map((r) => ({ id: r.id, name: r.name, icon: iconOf(r.icon, "sofa") }))];
    if (taskStore.tasks.some((t) => !t.room_id || !taskStore.rooms.some((r) => r.id === t.room_id))) {
      groups.push({ id: "", name: "Ohne Raum", icon: "star" });
    }
    return groups.map((r) => {
      const ts = taskStore.tasks
        .filter((t) => (r.id ? t.room_id === r.id : !t.room_id || !taskStore.rooms.some((x) => x.id === t.room_id)))
        .sort((a, b) => conditionOf(b, now) - conditionOf(a, now));
      const worst = ts.reduce((m, t) => Math.max(m, conditionOf(t, now)), 0);
      const overdue = ts.filter((t) => conditionOf(t, now) >= 1).length;
      const dueSoon = ts.filter((t) => conditionOf(t, now) >= 0.75 && conditionOf(t, now) < 1).length;
      const hint = !ts.length ? "noch keine Aufgaben" : overdue ? `${overdue} überfällig` : dueSoon ? `${dueSoon} bald fällig` : "alles im grünen Bereich";
      return { ...r, tasks: ts, worst, xp: ts.reduce((s, t) => s + t.xp, 0), hint };
    });
  });

  async function addRoom(e: SubmitEvent) {
    e.preventDefault();
    error = "";
    try {
      await taskStore.createRoom(roomName.trim(), roomIcon);
      roomName = "";
      adding = false;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

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
      <article class="card room" class:expanded={openRoom === r.id}>
        <button class="room-head" aria-expanded={openRoom === r.id} onclick={() => (openRoom = openRoom === r.id ? null : r.id)}>
          <span class="ic" aria-hidden="true"><Icon name={r.icon} size={22} /></span>
          <span class="name">
            <strong class="rname">{r.name}</strong>
            <span class="muted small">{r.tasks.length} {r.tasks.length === 1 ? "Aufgabe" : "Aufgaben"} · {r.hint}</span>
          </span>
          <span class="xp-chip">{r.xp} XP</span>
          <span class="cond"><ConditionBar value={r.worst} /></span>
        </button>
        {#if openRoom === r.id}
          <div class="stack inner">
            {#each r.tasks as task (task.id)}<TaskCard {task} />{/each}
            {#if planner}
              <button class="add-task" onclick={() => (sheet = { roomId: r.id || undefined })}><Icon name="plus" size={18} />Aufgabe in {r.name}</button>
            {/if}
          </div>
        {/if}
      </article>
    {/each}
  </div>
  {#if planner}
    {#if adding}
      <form class="card stack add-room" onsubmit={addRoom}>
        <label class="field"><span>Neuer Raum</span><input id="room-name" type="text" bind:value={roomName} maxlength="40" required placeholder="z. B. Kinderzimmer" /></label>
        <div class="icons">
          {#each TASK_ICONS as i (i)}
            <button type="button" class:sel={roomIcon === i} aria-label="Symbol {i}" onclick={() => (roomIcon = i)}><Icon name={i} size={18} /></button>
          {/each}
        </div>
        {#if error}<p class="err" role="alert">{error}</p>{/if}
        <div class="row">
          <button type="button" onclick={() => (adding = false)}>Abbrechen</button>
          <button class="primary" type="submit" disabled={!roomName.trim()}>Raum anlegen</button>
        </div>
      </form>
    {:else}
      <button class="add-room-btn" onclick={() => (adding = true)}><Icon name="plus" size={18} />Raum hinzufügen</button>
    {/if}
  {/if}
  <p class="muted small hint">Der Balken eines Raums zeigt seine dringendste Aufgabe. Wer früher erledigt, setzt den Balken zurück.</p>
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

{#if planner && view === "rooms"}
  <button class="fab primary" aria-label="Aufgabe anlegen" onclick={() => (sheet = {})}><Icon name="plus" size={24} /></button>
{/if}

{#if sheet}
  <TaskSheet roomId={sheet.roomId} onclose={() => (sheet = null)} />
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
    padding: 0;
    overflow: hidden;
  }

  .room-head {
    width: 100%;
    display: grid;
    grid-template-columns: 44px 1fr auto;
    gap: var(--space-2) var(--space-3);
    align-items: center;
    padding: var(--space-4);
    background: transparent;
    border: none;
    border-radius: 0;
    color: inherit;
    text-align: left;
  }

  .name {
    display: flex;
    flex-direction: column;
  }

  .inner {
    padding: 0 var(--space-3) var(--space-3);
  }

  .add-task,
  .add-room-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    background: transparent;
    border: 1px dashed var(--line);
    color: var(--muted);
  }

  .add-room-btn {
    width: 100%;
    margin-top: var(--space-3);
  }

  .add-room {
    margin-top: var(--space-3);
  }

  .icons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .icons button {
    width: 40px;
    min-height: 40px;
    padding: 0;
    display: grid;
    place-items: center;
    border-radius: 50%;
  }

  .icons button.sel {
    background: var(--primary-soft);
    border-color: var(--primary);
    color: var(--primary);
  }

  .err {
    color: var(--over);
  }

  .rname {
    font-family: var(--font-display);
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
