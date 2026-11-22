<script lang="ts">
  import { members, tasks } from "$lib/demo";

  // Placeholder for the canvas board (M6): zones per member, cards dragged between them.
  // Here: simple zones with a tap-to-move, which is also the small-screen fallback.
  let assignment = $state<Record<string, string | undefined>>(
    Object.fromEntries(tasks.map((t) => [t.id, t.assignee])),
  );
  let picked = $state<string | null>(null);

  const zones = [{ name: undefined, label: "Offen", color: "var(--text-muted)" }, ...members.map((m) => ({ name: m.name, label: m.name, color: m.color }))];

  function drop(zone: string | undefined) {
    if (!picked) return;
    assignment[picked] = zone;
    picked = null;
  }
</script>

<header>
  <h1>Board</h1>
  <p class="muted small">Karte antippen, dann eine Zone antippen = zuweisen. Freies Canvas mit Zoom folgt in M6.</p>
</header>

<div class="zones">
  {#each zones as z (z.label)}
    <div class="zone" class:target={picked !== null} style="--zone: {z.color}"
      onclick={() => drop(z.name)} onkeydown={(e) => e.key === "Enter" && drop(z.name)}
      role="button" tabindex="0" aria-label="Zone {z.label}">
      <h2>{z.label}</h2>
      <div class="cards">
        {#each tasks.filter((t) => assignment[t.id] === z.name) as t (t.id)}
          <button class="card-mini" class:picked={picked === t.id}
            onclick={(e) => { e.stopPropagation(); picked = picked === t.id ? null : t.id; }}>
            {t.icon} {t.title} <span class="muted">{t.points} P</span>
          </button>
        {/each}
      </div>
    </div>
  {/each}
</div>

<style>
  header {
    margin-bottom: var(--space-4);
  }

  h1 {
    font-size: 1.9rem;
  }

  .small {
    font-size: 0.85rem;
  }

  .zones {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: var(--space-3);
  }

  .zone {
    min-height: 140px;
    padding: var(--space-3);
    border: 2px dashed color-mix(in srgb, var(--zone) 50%, var(--border));
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--zone) 6%, var(--surface));
  }

  .zone.target {
    border-style: solid;
  }

  .zone h2 {
    font-size: 0.9rem;
    color: var(--zone);
    margin-bottom: var(--space-2);
  }

  .cards {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .card-mini {
    text-align: left;
    font-size: 0.85rem;
    background: var(--surface);
    border: 1px solid var(--border);
    padding: var(--space-2);
  }

  .card-mini.picked {
    outline: 2px solid var(--accent);
  }
</style>
