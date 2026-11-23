<script lang="ts">
  // Plan · Ansichten: saved views as chips; the selected one shows its tasks as list, kanban,
  // calendar or canvas. The last choice is remembered on this device.
  import type { ViewDto } from "$lib/generated";
  import TaskCard from "$lib/TaskCard.svelte";
  import { session } from "$lib/session.svelte";
  import { memberName, taskStore } from "$lib/tasks.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Board from "./Board.svelte";
  import Calendar from "./Calendar.svelte";
  import ViewSheet from "./ViewSheet.svelte";
  import { applyView, groupTasks } from "./views";

  const KEY = "fairteilt.view";
  let views = $state<ViewDto[]>([]);
  let selected = $state<string | null>(readSelected());
  let editing = $state<{ view: ViewDto | null } | null>(null);
  let error = $state("");

  function readSelected(): string | null {
    try {
      return localStorage.getItem(KEY);
    } catch {
      return null;
    }
  }

  function select(id: string | null) {
    selected = id;
    try {
      if (id) localStorage.setItem(KEY, id);
      else localStorage.removeItem(KEY);
    } catch {
      /* not remembered */
    }
  }

  const hid = $derived(session.membership?.household_id);

  async function load() {
    if (!hid) return;
    views = await session.request<ViewDto[]>("GET", `/households/${hid}/views`);
    if (selected && !views.some((v) => v.id === selected)) select(views[0]?.id ?? null);
    if (!selected && views.length) select(views[0].id);
  }

  $effect(() => {
    void taskStore.version;
    load().catch((e) => (error = e.message));
  });

  const view = $derived(views.find((v) => v.id === selected) ?? null);
  const tasks = $derived(view ? applyView(taskStore.tasks, view, taskStore.now) : []);
  const roomName = (id: string | null) => taskStore.rooms.find((r) => r.id === id)?.name ?? "";
  const groups = $derived(view ? groupTasks(tasks, view.layout === "kanban" ? "status" : view.sort.group, { member: memberName, room: roomName }) : []);
</script>

<div class="views">
  <div class="chips">
    {#each views as v (v.id)}
      <button class="chip" class:sel={v.id === selected} onclick={() => select(v.id)}>
        {v.name}{#if v.shared}<Icon name="family" size={14} />{/if}
      </button>
    {/each}
    <button class="chip add" onclick={() => (editing = { view: null })}><Icon name="plus" size={16} />Neue Ansicht</button>
  </div>
  {#if error}<p class="err small">{error}</p>{/if}

  {#if !view}
    <p class="muted">Eigene Ansichten merken sich Filter und Darstellung, zum Beispiel „Mias Aufgaben diese Woche“ als Liste oder „Alles mit Bonus“ als Kanban.</p>
  {:else}
    <div class="row between">
      <span class="muted small">{tasks.length} {tasks.length === 1 ? "Aufgabe" : "Aufgaben"}{view.shared ? " · für alle" : ""}</span>
      <button class="link" onclick={() => (editing = { view })}>{view.owner_id === session.membership?.membership_id ? "Bearbeiten" : "Ansehen"}</button>
    </div>

    {#if view.layout === "calendar"}
      <Calendar only={tasks} />
    {:else if view.layout === "canvas"}
      <Board only={tasks} />
    {:else if view.layout === "kanban"}
      <div class="kanban">
        {#each groups as g (g.key)}
          <section class="col">
            <h3>{g.label} <span class="muted small">{g.tasks.length}</span></h3>
            <div class="stack">{#each g.tasks as task (task.id)}<TaskCard {task} />{:else}<p class="muted small">–</p>{/each}</div>
          </section>
        {/each}
      </div>
    {:else}
      {#each groups as g (g.key)}
        {#if g.label}<h3 class="glabel">{g.label} <span class="muted small">{g.tasks.length}</span></h3>{/if}
        <div class="stack">{#each g.tasks as task (task.id)}<TaskCard {task} />{/each}</div>
      {/each}
      {#if tasks.length === 0}<p class="muted">Keine Aufgabe passt gerade.</p>{/if}
    {/if}
  {/if}
</div>

{#if editing}
  <ViewSheet
    view={editing.view}
    onclose={() => (editing = null)}
    onsaved={(v) => {
      editing = null;
      void load().then(() => select(v?.id ?? views[0]?.id ?? null));
    }}
  />
{/if}

<style>
  .views {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
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
    gap: 6px;
    min-height: 38px;
    padding: 0 14px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--fg);
  }

  .chip.sel {
    background: var(--primary-soft);
    border-color: var(--primary);
    color: var(--primary);
    font-weight: 700;
  }

  .chip.add {
    border-style: dashed;
    color: var(--muted);
  }

  .between {
    justify-content: space-between;
  }

  .link {
    background: transparent;
    border: none;
    color: var(--primary);
    font-weight: 700;
    min-height: 32px;
  }

  .kanban {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: minmax(260px, 1fr);
    gap: var(--space-3);
    overflow-x: auto;
    padding-bottom: var(--space-2);
  }

  .col {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2);
    border-radius: var(--radius);
    background: var(--surface-2);
  }

  .col h3,
  .glabel {
    font-size: 0.95rem;
  }

  .glabel {
    margin-top: var(--space-2);
  }

  .err {
    color: var(--over);
  }
</style>
