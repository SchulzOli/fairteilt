<script lang="ts">
  // Friendly check-in once the planned time of a started task is up (KONZEPT §7.1).
  // No countdown, no alarm: one calm question with three answers.
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { minutesSince, taskStore } from "$lib/tasks.svelte";
  import Krumel from "$lib/ui/Krumel.svelte";

  let busy = $state(false);
  let error = $state("");
  let thanks = $state<{ title: string; xp: number } | null>(null);

  const task = $derived(
    taskStore.tasks.find(
      (t) =>
        t.status === "in_progress" &&
        t.assignee_id === session.membership?.membership_id &&
        t.remind_at !== null &&
        Date.parse(t.remind_at) <= taskStore.now,
    ),
  );

  async function run(fn: () => Promise<void>) {
    busy = true;
    error = "";
    try {
      await fn();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  /** "Erledigt": checks the remaining steps in order; the last one completes the task. */
  const finish = () =>
    run(async () => {
      const t = task!;
      let xp: number | null = null;
      const open = t.steps.filter((s) => s.done_at === null);
      if (open.length === 0) xp = (await taskStore.action(t.id, "done")).xp_awarded;
      for (const s of open) xp = (await taskStore.checkStep(t.id, s.id)).xp_awarded ?? xp;
      feedback("done");
      thanks = { title: t.title, xp: xp ?? t.xp };
      setTimeout(() => (thanks = null), 2600);
    });

  const snooze = () => run(async () => void (await taskStore.action(task!.id, "snooze", 10)));
  const later = () => run(async () => void (await taskStore.action(task!.id, "pause")));
</script>

{#if task}
  <div class="backdrop" role="presentation">
    <div class="sheet stack checkin" role="alertdialog" aria-modal="true" aria-labelledby="checkin-title" aria-describedby="checkin-text">
      <div class="row">
        <span class="buddy" aria-hidden="true"><Krumel size={40} /></span>
        <div>
          <h2 id="checkin-title">Wie läuft's mit „{task.title}“?</h2>
          <p id="checkin-text" class="muted">
            Du bist seit {minutesSince(task.started_at ?? task.remind_at!, taskStore.now)} Minuten dabei. Kein Stress, sag einfach kurz Bescheid.
          </p>
        </div>
      </div>
      {#if error}<p class="err" role="alert">{error}</p>{/if}
      <button class="primary" disabled={busy} onclick={finish}>Erledigt</button>
      <div class="two">
        <button disabled={busy} onclick={snooze}>+10 Min.</button>
        <button disabled={busy} onclick={later}>Später</button>
      </div>
    </div>
  </div>
{/if}

{#if thanks}
  <div class="toast" role="status">Danke! „{thanks.title}“ ist erledigt · <b>+{thanks.xp} XP</b></div>
{/if}

<style>
  .checkin {
    padding-top: var(--space-5);
  }

  h2 {
    font-size: 1.2rem;
  }

  .buddy {
    flex: none;
    width: 56px;
    height: 56px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--krumel) 18%, var(--surface));
  }

  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-2);
  }

  .err {
    color: var(--over);
  }

  .toast {
    position: fixed;
    left: 50%;
    bottom: calc(var(--nav-h) + env(safe-area-inset-bottom) + var(--space-4));
    transform: translateX(-50%);
    z-index: 40;
    max-width: calc(100vw - 2 * var(--space-4));
    padding: var(--space-2) var(--space-4);
    border-radius: 999px;
    background: var(--xp);
    color: #fff;
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.2);
    animation: pop 0.3s ease;
  }

  @keyframes pop {
    from { transform: translate(-50%, 12px); opacity: 0; }
  }
</style>
