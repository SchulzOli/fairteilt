<script lang="ts">
  // An adult confirms a child's task (KONZEPT §4.2): XP can be adjusted within min…max,
  // and a short message goes along. "Nochmal machen" sends it back with the message.
  import { untrack } from "svelte";
  import type { TaskDto } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { memberName, taskStore } from "$lib/tasks.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import type { AnimalName } from "$lib/ui/Avatar.svelte";
  import Icon from "$lib/ui/Icon.svelte";

  let { task, onclose }: { task: TaskDto; onclose: () => void } = $props();

  let xp = $state(untrack(() => task.xp));
  let note = $state("");
  let busy = $state(false);
  let error = $state("");

  const who = $derived(session.members.find((m) => m.membership_id === task.completed_by));
  const name = $derived(who?.display_name ?? memberName(task.completed_by));
  const when = $derived(
    task.completed_at
      ? new Intl.DateTimeFormat("de-DE", { weekday: "short", hour: "2-digit", minute: "2-digit" }).format(new Date(task.completed_at))
      : "",
  );

  async function decide(approve: boolean) {
    busy = true;
    error = "";
    try {
      await taskStore.review(task.id, approve, approve && xp !== task.xp ? xp : null, note.trim() || null);
      feedback(approve ? "confirm" : "step");
      onclose();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="sheet stack" role="dialog" aria-modal="true" aria-labelledby="review-title">
    <div class="row between">
      <h2 id="review-title">Bestätigen</h2>
      <button class="icon-btn" aria-label="Schließen" onclick={onclose}><Icon name="close" size={20} /></button>
    </div>

    <div class="row who">
      {#if who}<Avatar animal={who.animal as AnimalName} color={who.color} size={44} name={name} />{/if}
      <div>
        <strong>{name} hat „{task.title}“ erledigt</strong>
        <div class="muted small">{when}</div>
      </div>
    </div>

    {#if task.steps.length}
      <ul class="steps">
        {#each task.steps as s (s.id)}<li><Icon name="check" size={14} />{s.title}</li>{/each}
      </ul>
    {/if}

    <div class="field"><span>XP</span>
      <div class="stepper">
        <button type="button" aria-label="Weniger XP" disabled={xp <= task.points_min} onclick={() => (xp = Math.max(task.points_min, xp - 1))}>−</button>
        <b>{xp}</b>
        <button type="button" aria-label="Mehr XP" disabled={xp >= task.points_max} onclick={() => (xp = Math.min(task.points_max, xp + 1))}>+</button>
        <span class="muted small">zwischen {task.points_min} und {task.points_max}</span>
      </div>
      {#if task.bounty > 0}<span class="muted small">Dazu kommt der Bonus: <span class="coin-chip">+{task.bounty}</span></span>{/if}
    </div>

    <label class="field"><span>Nachricht an {name} <small class="muted">(optional)</small></span>
      <input id="review-note" type="text" bind:value={note} maxlength="300" placeholder="z. B. Super gemacht!" />
    </label>

    {#if error}<p class="err" role="alert">{error}</p>{/if}
    <button class="primary" disabled={busy} onclick={() => decide(true)}><Icon name="check" size={18} />Bestätigen · {xp} XP</button>
    <button disabled={busy} onclick={() => decide(false)}>Nochmal machen lassen</button>
  </div>
</div>

<style>
  h2 {
    font-size: 1.3rem;
  }

  .between {
    justify-content: space-between;
  }

  .icon-btn {
    min-height: 36px;
    width: 36px;
    padding: 0;
    display: grid;
    place-items: center;
    background: transparent;
    border: none;
    color: var(--muted);
  }

  .steps {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .steps li {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 10px;
    border-radius: 999px;
    background: var(--ok-soft);
    color: var(--ok);
    font-size: 0.85rem;
  }

  .stepper {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    font-weight: 400;
  }

  .stepper button {
    width: 40px;
    min-height: 40px;
    padding: 0;
    border-radius: 50%;
    font-size: 1.2rem;
  }

  .stepper b {
    min-width: 2ch;
    text-align: center;
    font-size: 1.3rem;
  }

  .primary {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }

  .err {
    color: var(--over);
  }
</style>
