<script lang="ts">
  // One settlement ("Wer lag wo?", KONZEPT §5.3): target vs. XP per member and what follows.
  // Drafts can be approved by admins; otherwise they are approved automatically.
  import type { SettlementDto, SettlementLineDto } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { taskStore } from "$lib/tasks.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import type { AnimalName } from "$lib/ui/Avatar.svelte";

  let { settlement }: { settlement: SettlementDto } = $props();

  let busy = $state(false);
  let error = $state("");

  const day = new Intl.DateTimeFormat("de-DE", { day: "numeric", month: "short" });
  const dayTime = new Intl.DateTimeFormat("de-DE", { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  // The period ends at midnight: show the last day that belongs to it.
  const range = $derived(`${day.format(new Date(settlement.starts_at))} – ${day.format(new Date(Date.parse(settlement.ends_at) - 1))}`);
  const draft = $derived(settlement.status === "draft");

  const outcomeText: Record<string, string> = { within_tolerance: "im Rahmen", deficit: "Rückstand", surplus: "Vorsprung" };

  function consequence(l: SettlementLineDto): string {
    const parts: string[] = [];
    if (l.penalty_points_due > 0) parts.push(`Aufholaufgaben für ${l.penalty_points_due} XP`);
    if (l.balance_delta > 0) parts.push(`+${l.balance_delta} G`);
    if (l.balance_delta < 0) parts.push(`${l.balance_delta} G`);
    if (l.carry_next > 0) parts.push(`${l.carry_next} XP mehr im nächsten Monat`);
    if (l.carry_next < 0) parts.push(`${-l.carry_next} XP Vorsprung für den nächsten Monat`);
    return parts.join(" · ");
  }

  async function approve() {
    busy = true;
    error = "";
    try {
      await session.request("POST", `/settlements/${settlement.id}/approve`);
      feedback("confirm");
      taskStore.scheduleReload();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<article class="card settlement" class:draft>
  <div class="row between">
    <strong>{range}</strong>
    <span class="status">{draft ? "Entwurf" : "abgeschlossen"}</span>
  </div>
  <div class="muted small">
    Zusammen {settlement.total_xp} XP{#if settlement.goal}{` · Familienziel ${settlement.goal.target} XP: `}<b class="goal {settlement.goal.status}">{settlement.goal.status === "won" ? `geschafft (${settlement.goal.reward})` : "Krümel war schneller"}</b>{/if}
  </div>
  <ul>
    {#each settlement.lines as l (l.member_id)}
      {@const m = session.members.find((x) => x.membership_id === l.member_id)}
      <li>
        {#if m}<Avatar animal={m.animal as AnimalName} color={m.color} size={28} />{/if}
        <div class="who">
          <span><b>{m?.display_name ?? "?"}</b> <span class="muted small">{l.contribution} von {l.target} XP</span></span>
          {#if consequence(l)}<span class="small">{consequence(l)}</span>{/if}
        </div>
        <span class="outcome {l.outcome}">{outcomeText[l.outcome] ?? l.outcome}</span>
      </li>
    {/each}
  </ul>
  {#if draft}
    <p class="muted small">
      {settlement.auto_approve_at ? `Wird am ${dayTime.format(new Date(settlement.auto_approve_at))} automatisch freigegeben.` : "Wartet auf Freigabe."}
      Erst dann gibt es Guthaben und Aufholaufgaben.
    </p>
    {#if session.isAdmin}
      <button class="primary" disabled={busy} onclick={approve}>Jetzt freigeben</button>
    {/if}
  {/if}
  {#if error}<p class="err" role="alert">{error}</p>{/if}
</article>

<style>
  .settlement {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .settlement.draft {
    border-color: color-mix(in srgb, var(--primary) 45%, var(--line));
  }

  .between {
    justify-content: space-between;
  }

  .status {
    font-size: 0.8rem;
    font-weight: 700;
    padding: 2px 10px;
    border-radius: 999px;
    background: var(--surface-2);
  }

  .draft .status {
    background: var(--primary-soft);
    color: var(--primary);
  }

  ul {
    list-style: none;
    margin: var(--space-2) 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  li {
    display: grid;
    grid-template-columns: 28px 1fr auto;
    align-items: center;
    gap: var(--space-3);
  }

  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .outcome {
    font-size: 0.78rem;
    font-weight: 700;
    padding: 2px 9px;
    border-radius: 999px;
    background: var(--ok-soft);
    color: var(--ok);
    white-space: nowrap;
  }

  .outcome.deficit {
    background: var(--over-soft);
    color: var(--over);
  }

  .outcome.surplus {
    background: var(--xp-soft);
    color: var(--xp);
  }

  .goal.won {
    color: var(--coin);
  }

  .err {
    color: var(--over);
  }
</style>
