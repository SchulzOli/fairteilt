<script lang="ts">
  import { goto } from "$app/navigation";
  import type { TaskDto } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { conditionOf, dueText, iconOf, isQueued, memberName, minutesSince, stepsDone, taskStore } from "$lib/tasks.svelte";
  // Cards never finish a task directly (since 23.11.2026): "+" takes an open task over, the
  // task page is where it is done (steps, "Erledigt", giving it back).
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

  const now = $derived(taskStore.now);
  const condition = $derived(conditionOf(task, now));
  const room = $derived(taskStore.roomOf(task));
  const running = $derived(task.status === "in_progress");
  const done = $derived(stepsDone(task));
  const hasSteps = $derived(task.steps.length > 0);
  // Shopping (M8): finished in the shopping mode, where the cart is checked off.
  const list = $derived(shopping.listOf(task));
  const toBuy = $derived(list ? list.items.filter((i) => !i.checked_at).length : 0);
  const assignee = $derived(session.members.find((m) => m.membership_id === task.assignee_id));
  const isMine = $derived(task.assignee_id !== null && task.assignee_id === session.membership?.membership_id);
  const submitted = $derived(task.status === "submitted");
  const canReview = $derived(
    submitted && session.membership?.role !== "child" && task.completed_by !== session.membership?.membership_id,
  );

  const isOpen = $derived(task.assignee_id === null && (task.status === "open" || task.status === "claimed"));

  const open = () => goto(`/aufgabe/${task.id}`);

  /** "+": this open task is mine now. Tapping my card then opens it. */
  async function take() {
    if (taking) return;
    error = "";
    taking = true;
    try {
      const res = await taskStore.action(task.id, "claim");
      taken = true;
      setTimeout(() => (taken = false), 1400);
      feedback(isQueued(res) ? "step" : "confirm");
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      taking = false;
    }
  }
</script>

<article class="task" class:editable={!!onedit} class:taken class:overdue={condition >= 1} class:running class:mine={isMine}>
  <button class="hit" aria-label="Öffnen: {task.title}" onclick={open}></button>
  <div class="ic" aria-hidden="true"><Icon name={iconOf(task.icon, iconOf(room?.icon))} size={20} /></div>
  <div class="body">
    <h3>{task.title}</h3>
    <div class="meta">
      {#if room}<span>{room.name}</span><span aria-hidden="true">·</span>{/if}
      {#if submitted}
        <span class="wait">{canReview ? `${memberName(task.completed_by)} ist fertig` : "wartet aufs OK"}</span>
      {:else if running && task.started_at}
        <span class="run"><Icon name="timer" size={14} />läuft seit {minutesSince(task.started_at, now)} Min.</span>
      {:else}
        <span class:late={condition >= 1}>{dueText(task, now)}</span>
      {/if}
      {#if hasSteps}<span class="steps">{done}/{task.steps.length} Schritte</span>{/if}
      {#if list}<span class="steps">{toBuy} Artikel</span>{/if}
      {#if task.on_call}<span class="chip oncall">auf Abruf</span>{/if}
      {#if task.kind === "penalty"}<span class="chip aufhol">Aufholaufgabe</span>{/if}
      {#if task.kind === "reward"}<span class="chip reward">Belohnung</span>{/if}
    </div>
    {#if task.review_note && !submitted}<p class="note">„{task.review_note}“</p>{/if}
    {#if error}<p class="err" role="alert">{error}</p>{/if}
  </div>
  <div class="right">
    <span class="xp-chip">{task.xp} XP</span>
    {#if task.bounty > 0}<span class="coin-chip">+{task.bounty}</span>{/if}
    {#if assignee}
      <span class="who"><Avatar animal={assignee.animal as AnimalName} color={assignee.color} size={22} name={assignee.display_name} />{#if isMine}<small>du</small>{/if}</span>
    {:else}
      <span class="who nobody" title="noch offen">offen</span>
    {/if}
  </div>
  {#if submitted}
    {#if canReview}
      <button class="check review" aria-label="Prüfen: {task.title}" onclick={() => (reviewing = true)}><Icon name="check" size={18} /></button>
    {:else}
      <span class="check waiting" aria-hidden="true"><Icon name="timer" size={18} /></span>
    {/if}
  {:else if isOpen}
    <button class="check take" class:busy={taking} aria-label="Übernehmen: {task.title}" disabled={taking} onclick={take}>
      <Icon name="plus" size={20} />
    </button>
  {:else if isMine}
    <button class="check go" aria-label="Öffnen: {task.title}" onclick={open}>
      <Icon name={running ? "timer" : task.list_id ? "cart" : "chevron"} size={18} />
    </button>
  {:else}
    <span class="check spacer" aria-hidden="true"></span>
  {/if}
  {#if onedit}
    <button class="edit" aria-label="Einrichtung bearbeiten: {task.title}" onclick={onedit}><Icon name="edit" size={16} /></button>
  {/if}
  <div class="cond"><ConditionBar value={condition} /></div>

  {#if taken}
    <span class="fly" aria-hidden="true">Deine Aufgabe</span>
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

  .task.running {
    border-color: color-mix(in srgb, var(--primary) 45%, var(--line));
  }

  .run {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    color: var(--primary);
    font-weight: 700;
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

  .late {
    color: var(--over);
    font-weight: 700;
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

  .chip.reward {
    background: var(--ok-soft);
    color: var(--ok);
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

  .check.spacer {
    border: none;
    min-height: var(--tap);
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
