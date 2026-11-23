<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import ConditionBar from "$lib/ui/ConditionBar.svelte";
  import Board from "$lib/plan/Board.svelte";
  import Calendar from "$lib/plan/Calendar.svelte";
  import Views from "$lib/plan/Views.svelte";
  import TaskCard from "$lib/TaskCard.svelte";
  import TaskSheet from "$lib/TaskSheet.svelte";
  import { session } from "$lib/session.svelte";
  import { TASK_ICONS, conditionOf, iconOf, taskStore } from "$lib/tasks.svelte";
  import type { IconName } from "$lib/ui/Icon.svelte";
  import type { TemplateDto, TriggerResponse } from "$lib/generated";
  import { shopping } from "$lib/shopping.svelte";
  import { goto } from "$app/navigation";

  type View = "rooms" | "calendar" | "board" | "views";
  const VIEW_KEY = "fairteilt.plan";
  let view = $state<View>(readView());

  function readView(): View {
    try {
      const v = localStorage.getItem(VIEW_KEY);
      return v === "calendar" || v === "board" || v === "views" ? v : "rooms";
    } catch {
      return "rooms";
    }
  }

  function show(v: View) {
    view = v;
    try {
      localStorage.setItem(VIEW_KEY, v);
    } catch {
      /* not remembered */
    }
  }
  let openRoom = $state<string | null>(null);
  let sheet = $state<{ roomId?: string } | null>(null);
  let adding = $state(false);
  let roomName = $state("");
  let roomIcon = $state<IconName>("sofa");
  let error = $state("");

  const planner = $derived(session.membership?.role !== "child");

  // "Auf Abruf" (M8): templates that rest until someone says "Ist wieder nötig". Shopping
  // tasks are left out: their list calls them.
  let templates = $state<TemplateDto[]>([]);
  let calling = $state<string | null>(null);
  $effect(() => {
    void taskStore.version;
    const hid = session.membership?.household_id;
    if (hid) session.request<TemplateDto[]>("GET", `/households/${hid}/templates`).then((t) => (templates = t), () => {});
  });
  const resting = $derived(
    templates.filter(
      (t) =>
        t.schedule.type === "on_call" &&
        !shopping.lists.some((l) => l.template_id === t.id) &&
        !taskStore.tasks.some((x) => x.template_id === t.id),
    ),
  );

  async function call(t: TemplateDto) {
    calling = t.id;
    error = "";
    try {
      const res = await session.request<TriggerResponse>("POST", `/templates/${t.id}/trigger`);
      taskStore.scheduleReload();
      void goto(`/aufgabe/${res.task_id}`);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      calling = null;
    }
  }

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
</script>

<header class="stack tight">
  <h1 class="page-title">Plan</h1>
  <div class="seg" role="tablist" aria-label="Ansicht">
    <button role="tab" aria-selected={view === "rooms"} class:sel={view === "rooms"} onclick={() => show("rooms")}>Räume</button>
    <button role="tab" aria-selected={view === "calendar"} class:sel={view === "calendar"} onclick={() => show("calendar")}>Kalender</button>
    <button role="tab" aria-selected={view === "board"} class:sel={view === "board"} onclick={() => show("board")}>Board</button>
    <button role="tab" aria-selected={view === "views"} class:sel={view === "views"} onclick={() => show("views")}>Ansichten</button>
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
  {#if resting.length}
    <h2 class="section-title">Auf Abruf</h2>
    <div class="stack">
      {#each resting as t (t.id)}
        <div class="card resting">
          <span class="ic" aria-hidden="true"><Icon name={iconOf(t.icon, "star")} size={20} /></span>
          <span class="name">
            <strong>{t.title}</strong>
            <span class="muted small">ruht · {t.points.default} XP, wenn es wieder nötig ist</span>
          </span>
          <button class="primary" disabled={calling === t.id} onclick={() => call(t)}>Ist wieder nötig</button>
        </div>
      {/each}
    </div>
    {#if error && !adding}<p class="err" role="alert">{error}</p>{/if}
  {/if}
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
  <Calendar />
{:else if view === "board"}
  <Board />
{:else}
  <Views />
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

  .seg button {
    font-size: 0.85rem;
  }

  .room {
    padding: 0;
    overflow: hidden;
  }

  .resting {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3);
  }

  .resting .ic {
    width: 38px;
    height: 38px;
    border-radius: 13px;
    background: var(--surface-2);
    display: grid;
    place-items: center;
  }

  .resting .name {
    display: flex;
    flex-direction: column;
  }

  .resting button {
    font-size: 0.85rem;
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










</style>
