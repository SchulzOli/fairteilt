<script lang="ts">
  // Admins set the family goal (XP) and what the family gets for beating Krümel.
  import { untrack } from "svelte";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { taskStore } from "$lib/tasks.svelte";
  import Icon from "$lib/ui/Icon.svelte";

  let { onclose }: { onclose: () => void } = $props();

  const goal = untrack(() => taskStore.period?.goal);
  let target = $state(goal?.target ?? 100);
  let reward = $state(goal?.reward ?? "");
  let keep = $state(true);
  let busy = $state(false);
  let error = $state("");

  async function save(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = "";
    try {
      const hid = session.membership!.household_id;
      await session.request("PUT", `/households/${hid}/goal`, { target, reward: reward.trim(), every_period: keep });
      feedback("confirm");
      taskStore.scheduleReload();
      onclose();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="goal-title">
  <form class="stack" onsubmit={save}>
    <div class="row between">
      <h2 id="goal-title">Familienziel</h2>
      <button type="button" class="icon-btn" aria-label="Schließen" onclick={onclose}><Icon name="close" size={20} /></button>
    </div>
    <p class="muted small">Gemeinsam so viele XP sammeln, bevor Krümel ankommt. Er läuft gleichmäßig und kommt am Ende des Monats an; liegen gebliebene Aufgaben schieben ihn an.</p>
    <div class="field"><span>Ziel</span>
      <div class="stepper">
        <button type="button" aria-label="Weniger" onclick={() => (target = Math.max(10, target - 10))}>−</button>
        <b>{target} XP</b>
        <button type="button" aria-label="Mehr" onclick={() => (target = Math.min(100000, target + 10))}>+</button>
      </div>
    </div>
    <label class="field"><span>Familienbelohnung</span>
      <input id="goal-reward" type="text" bind:value={reward} maxlength="60" required placeholder="z. B. Eis essen" />
    </label>
    <label class="toggle"><span>Jeden Monat dieses Ziel</span><input type="checkbox" bind:checked={keep} /></label>
    <p class="muted small">{keep ? "Das Ziel bleibt gleich." : "Ab nächstem Monat schlägt Fairteilt das Ziel aus dem letzten Monat vor."}</p>
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    <button class="primary" type="submit" disabled={busy || !reward.trim()}>Speichern</button>
  </form>
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

  .stepper {
    display: flex;
    align-items: center;
    gap: var(--space-3);
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
    font-size: 1.3rem;
  }

  .toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-weight: 700;
  }

  .err {
    color: var(--over);
  }
</style>
