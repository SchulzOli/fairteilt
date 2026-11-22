<script lang="ts">
  import TaskCard from "$lib/TaskCard.svelte";
  import { me, tasks } from "$lib/demo";

  let done = $state<string[]>([]);

  const mine = $derived(tasks.filter((t) => t.assignee === me.name && !done.includes(t.id)));
  const open = $derived(tasks.filter((t) => !t.assignee && !done.includes(t.id)));

  // Demo values for the period progress.
  const contribution = 112;
  const target = 140;
  const progress = $derived(Math.min(100, Math.round((contribution / target) * 100)));
</script>

<header class="stack">
  <p class="muted greeting">Hallo {me.name} 👋</p>
  <h1>Heute</h1>
</header>

<section class="card period" aria-label="Fortschritt in dieser Woche">
  <div class="row between">
    <div>
      <div class="muted small">Diese Woche</div>
      <div class="big">{contribution} <span class="muted">/ {target} LP</span></div>
    </div>
    <div class="balance">
      <div class="muted small">Guthaben</div>
      <div class="big">86 G</div>
    </div>
  </div>
  <div class="bar" role="progressbar" aria-valuenow={progress} aria-valuemin="0" aria-valuemax="100">
    <div style="width: {progress}%"></div>
  </div>
  <p class="muted small">Noch {target - contribution} Leistungspunkte bis zu deinem fairen Anteil · 3 Tage übrig</p>
</section>

<h2 class="section-title">Meine Aufgaben</h2>
<div class="stack">
  {#each mine as task (task.id)}
    <TaskCard {task} ondone={(id) => (done = [...done, id])} />
  {:else}
    <p class="muted">Alles erledigt 🎉</p>
  {/each}
</div>

<h2 class="section-title">Offen für alle</h2>
<div class="stack">
  {#each open as task (task.id)}
    <TaskCard {task} />
  {/each}
</div>

<p class="muted small demo">Demo-Daten. Die echte Anbindung folgt mit M1–M3.</p>

<button class="fab primary" aria-label="Aufgabe anlegen">＋</button>

<style>
  header {
    gap: 0;
    margin-bottom: var(--space-4);
  }

  h1 {
    font-size: 1.9rem;
  }

  .greeting {
    margin: 0;
  }

  .period {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin-bottom: var(--space-3);
  }

  .between {
    justify-content: space-between;
  }

  .balance {
    text-align: right;
  }

  .big {
    font-size: 1.4rem;
    font-weight: 700;
  }

  .small {
    font-size: 0.85rem;
    margin: 0;
  }

  .bar {
    height: 10px;
    border-radius: 999px;
    background: var(--surface-2);
    overflow: hidden;
  }

  .bar > div {
    height: 100%;
    background: var(--accent);
    border-radius: inherit;
    transition: width 0.3s ease;
  }

  .section-title {
    margin: var(--space-5) 0 var(--space-3);
  }

  .demo {
    margin-top: var(--space-5);
    text-align: center;
  }

  .fab {
    position: fixed;
    right: var(--space-4);
    bottom: calc(var(--nav-h) + env(safe-area-inset-bottom) + var(--space-4));
    width: 56px;
    height: 56px;
    border-radius: 18px;
    font-size: 1.6rem;
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.2);
  }
</style>
