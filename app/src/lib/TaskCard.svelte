<script lang="ts">
  import { goto } from "$app/navigation";
  import type { TaskDto } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { conditionOf, dueText, iconOf, minutesSince, stepsDone, taskStore } from "$lib/tasks.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import type { AnimalName } from "$lib/ui/Avatar.svelte";
  import ConditionBar from "$lib/ui/ConditionBar.svelte";
  import Icon from "$lib/ui/Icon.svelte";

  let { task }: { task: TaskDto } = $props();

  let celebrating = $state(false);
  let earned = $state(0);
  let error = $state("");

  const now = $derived(taskStore.now);
  const condition = $derived(conditionOf(task, now));
  const room = $derived(taskStore.roomOf(task));
  const running = $derived(task.status === "in_progress");
  const done = $derived(stepsDone(task));
  const hasSteps = $derived(task.steps.length > 0);
  const assignee = $derived(session.members.find((m) => m.membership_id === task.assignee_id));
  const isMine = $derived(task.assignee_id !== null && task.assignee_id === session.membership?.membership_id);

  // Confetti pieces fly out of the check button; everything stays inside the card.
  const COLORS = ["var(--coin-bg)", "#d63384", "var(--xp)", "var(--primary)", "#e8660f", "var(--fresh)", "#2563eb", "var(--over)"];
  const pieces = Array.from({ length: 30 }, (_, i) => ({
    x: -(12 + ((i * 53) % 220)),
    y: ((i * 37) % 56) - 30,
    r: ((i * 71) % 400) - 200,
    w: [5, 4, 6, 3, 5][i % 5],
    h: [8, 7, 6, 9, 5][i % 5],
    d: ((i * 13) % 30) / 100,
    c: COLORS[i % COLORS.length],
    round: i % 5 === 2,
  }));

  const open = () => goto(`/aufgabe/${task.id}`);

  async function complete() {
    if (celebrating) return;
    // Tasks with steps are finished step by step on their page (XP only at the end).
    if (hasSteps) return open();
    error = "";
    try {
      celebrating = true;
      taskStore.hold(1600);
      const res = await taskStore.action(task.id, "done");
      earned = res.xp_awarded ?? task.xp;
      feedback("done");
    } catch (e) {
      celebrating = false;
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<article class="task" class:celebrating class:overdue={condition >= 1} class:running>
  <button class="hit" aria-label="Öffnen: {task.title}" onclick={open}></button>
  <div class="ic" aria-hidden="true"><Icon name={iconOf(task.icon, iconOf(room?.icon))} size={20} /></div>
  <div class="body">
    <h3 class:done={celebrating}>{task.title}</h3>
    <div class="meta">
      {#if room}<span>{room.name}</span><span aria-hidden="true">·</span>{/if}
      {#if running && task.started_at}
        <span class="run"><Icon name="timer" size={14} />läuft seit {minutesSince(task.started_at, now)} Min.</span>
      {:else}
        <span class:late={condition >= 1}>{dueText(task, now)}</span>
      {/if}
      {#if hasSteps}<span class="steps">{done}/{task.steps.length} Schritte</span>{/if}
      {#if task.kind === "penalty"}<span class="chip aufhol">Aufholaufgabe</span>{/if}
      {#if task.kind === "reward"}<span class="chip reward">Belohnung</span>{/if}
    </div>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
  </div>
  <div class="right">
    <span class="xp-chip">{task.xp} XP</span>
    {#if assignee && !isMine}
      <Avatar animal={assignee.animal as AnimalName} color={assignee.color} size={22} name={assignee.display_name} />
    {/if}
  </div>
  {#if hasSteps}
    <button class="check go" aria-label="{running ? 'Weiter' : 'Starten'}: {task.title}" onclick={open}>
      <Icon name={running ? "chevron" : "play"} size={16} />
    </button>
  {:else}
    <button class="check" class:on={celebrating} aria-label="Erledigt: {task.title}" onclick={complete}>
      <Icon name="check" size={18} />
    </button>
  {/if}
  <div class="cond"><ConditionBar value={condition} /></div>

  {#if celebrating}
    <span class="fly" aria-hidden="true">+{earned || task.xp} XP</span>
    <span class="confetti" aria-hidden="true">
      {#each pieces as p, i (i)}
        <i
          class:round={p.round}
          style="--x:{p.x}px;--y:{p.y}px;--r:{p.r}deg;--w:{p.w}px;--h:{p.h}px;--d:{p.d}s;background:{p.c}"
        ></i>
      {/each}
    </span>
  {/if}
</article>

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

  .check.go {
    color: var(--primary);
    border-color: color-mix(in srgb, var(--primary) 40%, var(--line));
  }

  .task.overdue {
    border-color: color-mix(in srgb, var(--over) 45%, var(--line));
  }

  .task.celebrating {
    background: color-mix(in srgb, var(--fresh) 9%, var(--surface));
    border-color: color-mix(in srgb, var(--fresh) 45%, var(--line));
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

  h3.done {
    text-decoration: line-through;
    opacity: 0.6;
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

  .check.on {
    background: var(--fresh);
    border-color: var(--fresh);
    color: #fff;
  }

  .cond {
    grid-column: 2 / -1;
  }

  .fly {
    position: absolute;
    right: 58px;
    top: 50%;
    transform: translateY(-50%);
    z-index: 3;
    background: var(--xp);
    color: #fff;
    font-weight: 800;
    font-size: 0.8rem;
    border-radius: 999px;
    padding: 2px 9px;
    animation: fly-in 1.4s ease-out forwards;
  }

  .confetti {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 1;
  }

  .confetti i {
    position: absolute;
    right: 28px;
    top: 50%;
    width: var(--w);
    height: var(--h);
    border-radius: 2px;
    opacity: 0;
    animation: burst 1.3s cubic-bezier(0.15, 0.7, 0.3, 1) forwards;
    animation-delay: var(--d);
  }

  .confetti i.round {
    border-radius: 50%;
  }

  @keyframes burst {
    0% { transform: translate(0, 0) rotate(0deg) scale(0.3); opacity: 0; }
    10% { opacity: 1; }
    50% { transform: translate(var(--x), var(--y)) rotate(var(--r)) scale(1); opacity: 1; }
    100% { transform: translate(calc(var(--x) * 1.1), calc(var(--y) + 26px)) rotate(calc(var(--r) * 3)); opacity: 0; }
  }

  @keyframes fly-in {
    0% { transform: translateY(-20%) scale(0.5); opacity: 0; }
    15% { transform: translateY(-50%) scale(1.2); opacity: 1; }
    25% { transform: translateY(-50%) scale(1); }
    75% { transform: translateY(-50%); opacity: 1; }
    100% { transform: translateY(-110%); opacity: 0; }
  }

  @media (prefers-reduced-motion: reduce) {
    .confetti {
      display: none;
    }
  }
</style>
