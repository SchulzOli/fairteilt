<script lang="ts">
  // A task on Heute (and in lists). Cards never finish a task (since 23.11.2026): "+" takes an
  // open task or joins someone else's (the XP is shared), tapping opens the task page, where
  // it is done. Everyone on it shows as an animal; tasks of others fade into the background.
  import { goto } from "$app/navigation";
  import type { TaskDto } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { conditionOf, iconOf, isOn, isQueued, memberName, peopleOn, stepsDone, taskStore } from "$lib/tasks.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import type { AnimalName } from "$lib/ui/Avatar.svelte";
  import ConditionBar from "$lib/ui/ConditionBar.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import ReviewSheet from "$lib/ReviewSheet.svelte";
  import { shopping } from "$lib/shopping.svelte";

  let { task, onedit }: { task: TaskDto; /** Planners: pencil to change the setup. */ onedit?: () => void } = $props();

  let taking = $state(false);
  let taken = $state(false);
  let reviewing = $state(false);
  let error = $state("");

  const me = $derived(session.membership?.membership_id);
  const condition = $derived(conditionOf(task, taskStore.now));
  const room = $derived(taskStore.roomOf(task));
  const done = $derived(stepsDone(task));
  const hasSteps = $derived(task.steps.length > 0);
  // Shopping (M8): finished on the task page, where the cart is checked off.
  const list = $derived(shopping.listOf(task));
  const toBuy = $derived(list ? list.items.filter((i) => !i.checked_at).length : 0);
  const people = $derived(peopleOn(task).map((id) => session.members.find((m) => m.membership_id === id)).filter((m) => !!m));
  const mine = $derived(isOn(task, me));
  const submitted = $derived(task.status === "submitted");
  const canReview = $derived(submitted && session.membership?.role !== "child" && task.completed_by !== me);
  const canTake = $derived(!submitted && !mine && (task.status === "open" || task.status === "claimed" || task.status === "in_progress"));
  /** Someone else's task: in the background, "+" still lets me join. */
  const faded = $derived(people.length > 0 && !mine && !submitted);

  const open = () => goto(`/aufgabe/${task.id}`);

  /** "+": an open task becomes mine; on someone else's I join and we share the XP. */
  async function take() {
    if (taking) return;
    error = "";
    taking = true;
    const joining = people.length > 0;
    try {
      const res = await taskStore.action(task.id, "claim");
      taken = true;
      flyText = joining ? "Du machst mit" : "Deine Aufgabe";
      setTimeout(() => (taken = false), 1400);
      feedback(isQueued(res) ? "step" : "confirm");
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      taking = false;
    }
  }
  let flyText = $state("");
</script>

<article class="task" class:editable={!!onedit} class:taken class:faded class:overdue={condition >= 1} class:mine>
  <button class="hit" aria-label="Öffnen: {task.title}" onclick={open}></button>
  <div class="ic" aria-hidden="true"><Icon name={iconOf(task.icon, iconOf(room?.icon))} size={20} /></div>
  <div class="body">
    <h3>{task.title}</h3>
    <div class="meta">
      {#if room}<span>{room.name}</span>{/if}
      {#if submitted}
        <span class="wait">{canReview ? `${memberName(task.completed_by)} ist fertig` : "wartet aufs OK"}</span>
      {/if}
      {#if hasSteps}<span class="steps">{done}/{task.steps.length} Schritte</span>{/if}
      {#if list}<span class="steps">{toBuy} Artikel</span>{/if}
      {#if task.on_call}<span class="chip oncall">auf Abruf</span>{/if}
      {#if task.kind === "penalty"}<span class="chip aufhol">Aufholaufgabe</span>{/if}
    </div>
    {#if task.review_note && !submitted}<p class="note">„{task.review_note}“</p>{/if}
    {#if error}<p class="err" role="alert">{error}</p>{/if}
  </div>
  <div class="right">
    <span class="xp-chip">{task.xp} XP{#if task.bounty > 0}<b class="bonus">+{task.bounty}</b>{/if}</span>
    {#if people.length}
      <span class="who" title={people.map((p) => p!.display_name).join(", ")}>
        {#each people.slice(0, 3) as p (p!.membership_id)}
          <span class="av"><Avatar animal={p!.animal as AnimalName} color={p!.color} size={22} name={p!.display_name} /></span>
        {/each}
        {#if people.length > 3}<small>+{people.length - 3}</small>{/if}
        {#if mine}<small>du</small>{/if}
      </span>
    {:else}
      <span class="who nobody" title="noch offen">offen</span>
    {/if}
  </div>
  {#if submitted}
    {#if canReview}
      <button class="check review" aria-label="Prüfen: {task.title}" onclick={() => (reviewing = true)}><Icon name="check" size={18} /></button>
    {:else}
      <span class="check waiting" aria-hidden="true"><Icon name="flag" size={18} /></span>
    {/if}
  {:else if canTake}
    <button class="check take" class:busy={taking} aria-label="{people.length ? 'Mitmachen' : 'Übernehmen'}: {task.title}" disabled={taking} onclick={take}>
      <Icon name="plus" size={20} />
    </button>
  {:else}
    <button class="check go" aria-label="Öffnen: {task.title}" onclick={open}>
      <Icon name={task.list_id ? "cart" : "chevron"} size={18} />
    </button>
  {/if}
  {#if onedit}
    <button class="edit" aria-label="Einrichtung bearbeiten: {task.title}" onclick={onedit}><Icon name="edit" size={16} /></button>
  {/if}
  <div class="cond"><ConditionBar value={condition} none={!task.due_at} /></div>

  {#if taken}
    <span class="fly" aria-hidden="true">{flyText}</span>
  {/if}
</article>

{#if reviewing}
  <ReviewSheet {task} onclose={() => (reviewing = false)} />
{/if}

<style>
  .task {
    position: relative;
    overflow: hidden;
    display: grid;
    grid-template-columns: 38px 1fr auto auto;
    align-items: center;
    gap: var(--space-2) var(--space-3);
    padding: var(--space-3);
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    transition: background 0.3s, border-color 0.3s;
  }

  .hit {
    position: absolute;
    inset: 0;
    min-height: 0;
    padding: 0;
    border-radius: inherit;
    background: transparent;
    z-index: 0;
  }

  .ic,
  .body,
  .right,
  .cond {
    position: relative;
    z-index: 1;
    pointer-events: none;
  }

  /* Someone else's task: in the background of the attention */
  .task.faded {
    background: color-mix(in srgb, var(--surface) 55%, var(--bg));
    border-style: dashed;
  }

  .task.faded .ic,
  .task.faded .body,
  .task.faded .right,
  .task.faded .cond {
    opacity: 0.55;
    filter: saturate(0.4);
    transition: opacity 0.2s, filter 0.2s;
  }

  .task.faded:hover .ic,
  .task.faded:hover .body,
  .task.faded:hover .right,
  .task.faded:hover .cond {
    opacity: 0.9;
    filter: none;
  }

  .bonus {
    margin-left: 5px;
    padding-left: 5px;
    border-left: 1px solid color-mix(in srgb, var(--xp) 35%, transparent);
  }

  .av {
    display: grid;
    margin-left: -6px;
    border-radius: 50%;
    box-shadow: 0 0 0 2px var(--surface);
  }

  .av:first-child {
    margin-left: 0;
  }

  .steps {
    padding: 0 7px;
    border-radius: 999px;
    background: var(--surface-2);
    font-size: 0.78rem;
    font-weight: 700;
  }

  .err {
    color: var(--over);
    font-size: 0.85rem;
  }

  .wait {
    padding: 0 8px;
    border-radius: 999px;
    background: var(--coin-soft);
    color: var(--coin);
    font-weight: 700;
    font-size: 0.8rem;
  }

  .note {
    margin-top: 2px;
    font-size: 0.85rem;
    font-style: italic;
    color: var(--fg);
  }

  .check.review {
    color: #fff;
    background: var(--primary);
    border-color: var(--primary);
  }

  .check.waiting {
    color: var(--coin);
    border-style: dashed;
    min-height: var(--tap);
  }

  .check.go {
    color: var(--primary);
    border-color: color-mix(in srgb, var(--primary) 40%, var(--line));
  }

  .task.overdue {
    border-color: color-mix(in srgb, var(--over) 45%, var(--line));
  }

  .task.taken {
    background: color-mix(in srgb, var(--primary) 7%, var(--surface));
    border-color: color-mix(in srgb, var(--primary) 45%, var(--line));
  }

  .ic {
    width: 38px;
    height: 38px;
    border-radius: 13px;
    background: var(--surface-2);
    display: grid;
    place-items: center;
  }

  h3 {
    font-family: var(--font-body);
    font-size: 1rem;
    font-weight: 700;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
    font-size: 0.85rem;
    color: var(--muted);
  }

  .chip {
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 0.75rem;
    font-weight: 700;
  }

  .chip.aufhol {
    background: var(--over-soft);
    color: var(--over);
  }

  .chip.oncall {
    background: var(--primary-soft);
    color: var(--primary);
  }

  .right {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
  }

  .check {
    width: var(--tap);
    padding: 0;
    border-radius: 50%;
    border: 2px solid var(--line);
    background: transparent;
    color: var(--fresh);
    display: grid;
    place-items: center;
    position: relative;
    z-index: 2;
  }

  /* "+": take an open task over */
  .check.take {
    border-style: dashed;
    border-color: color-mix(in srgb, var(--primary) 55%, var(--line));
    color: var(--primary);
  }

  .check.take:hover:not(:disabled) {
    background: var(--primary-soft);
    border-style: solid;
  }

  .check.take.busy {
    opacity: 0.5;
  }

  .who {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .who small {
    font-size: 0.72rem;
    font-weight: 700;
    color: var(--muted);
  }

  .who.nobody {
    padding: 0 7px;
    border-radius: 999px;
    border: 1.5px dashed var(--line);
    font-size: 0.72rem;
    font-weight: 700;
    color: var(--muted);
  }

  /* The pencil sits in the bottom row, next to the bar. */
  .edit {
    position: relative;
    z-index: 2;
    grid-row: 2;
    grid-column: 4;
    justify-self: center;
    width: 30px;
    min-height: 26px;
    padding: 0;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: transparent;
    color: var(--muted);
  }

  .edit:hover {
    background: var(--surface-2);
    color: var(--fg);
  }

  .cond {
    grid-column: 2 / -1;
    grid-row: 2;
  }

  .editable .cond {
    grid-column: 2 / 4;
  }

  .fly {
    position: absolute;
    right: 58px;
    top: 50%;
    transform: translateY(-50%);
    z-index: 3;
    background: var(--primary);
    color: var(--primary-fg);
    font-weight: 800;
    font-size: 0.8rem;
    border-radius: 999px;
    padding: 2px 9px;
    white-space: nowrap;
    animation: fly-in 1.4s ease-out forwards;
  }

  @keyframes fly-in {
    0% { transform: translateY(-20%) scale(0.5); opacity: 0; }
    15% { transform: translateY(-50%) scale(1.2); opacity: 1; }
    25% { transform: translateY(-50%) scale(1); }
    75% { transform: translateY(-50%); opacity: 1; }
    100% { transform: translateY(-110%); opacity: 0; }
  }

</style>
