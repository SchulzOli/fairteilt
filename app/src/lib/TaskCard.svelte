<script lang="ts">
  import type { DemoTask } from "$lib/demo";

  let { task, ondone }: { task: DemoTask; ondone?: (id: string) => void } = $props();
</script>

<article class="task" class:overdue={task.overdue}>
  <div class="icon" aria-hidden="true">{task.icon}</div>
  <div class="body">
    <h3>{task.title}</h3>
    <div class="meta">
      <span class:late={task.overdue}>{task.due}</span>
      <span>·</span>
      <span>{task.room}</span>
      {#if task.kind === "penalty"}<span class="chip penalty">Aufholaufgabe</span>{/if}
      {#if task.kind === "reward"}<span class="chip reward">Belohnung</span>{/if}
    </div>
  </div>
  <div class="points">
    <span class="value">{task.points}</span>
    {#if task.bounty}<span class="chip bounty">+{task.bounty}</span>{/if}
  </div>
  {#if ondone}
    <button class="done" aria-label="Erledigt: {task.title}" onclick={() => ondone(task.id)}>✓</button>
  {/if}
</article>

<style>
  .task {
    display: grid;
    grid-template-columns: 40px 1fr auto auto;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .task.overdue {
    border-color: color-mix(in srgb, var(--danger) 45%, var(--border));
  }

  .icon {
    font-size: 1.5rem;
    text-align: center;
  }

  h3 {
    font-size: 1rem;
    font-weight: 600;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
    font-size: 0.85rem;
    color: var(--text-muted);
  }

  .late {
    color: var(--danger);
    font-weight: 600;
  }

  .points {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
  }

  .value {
    font-size: 1.15rem;
    font-weight: 700;
  }

  .value::after {
    content: " P";
    font-size: 0.75rem;
    color: var(--text-muted);
    font-weight: 600;
  }

  .chip.bounty {
    background: color-mix(in srgb, var(--bounty) 18%, transparent);
    color: var(--bounty);
  }

  .chip.penalty {
    background: color-mix(in srgb, var(--danger) 15%, transparent);
    color: var(--danger);
  }

  .chip.reward {
    background: color-mix(in srgb, var(--success) 15%, transparent);
    color: var(--success);
  }

  .done {
    width: var(--tap);
    padding: 0;
    border-radius: 50%;
    border: 2px solid var(--border);
    background: transparent;
    font-size: 1.1rem;
    color: var(--success);
  }
</style>
