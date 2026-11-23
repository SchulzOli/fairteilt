<script lang="ts">
  // Punkte (KONZEPT §7.3): own XP and streak, the family's monthly goal (adults set it up),
  // closings, history, and the family in member order (no ranking). Only XP since 23.11.2026:
  // no Guthaben, no transfers, no rewards catalog; bonus XP on tasks is a gift with a
  // monthly allowance.
  import GoalSheet from "$lib/GoalSheet.svelte";
  import SettlementCard from "$lib/SettlementCard.svelte";
  import type { LedgerEntryDto, ReverseResponse } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { memberName, taskStore } from "$lib/tasks.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import type { AnimalName } from "$lib/ui/Avatar.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Krumel from "$lib/ui/Krumel.svelte";

  let history = $state<LedgerEntryDto[]>([]);
  let more = $state(false);
  let busy = $state(false);
  let error = $state("");
  let sent = $state("");
  let goalSheet = $state(false);
  /** Admins: whose history is shown (null = your own). */
  let viewing = $state<string | null>(null);
  let reversing = $state<string | null>(null);
  let reason = $state("");

  const hid = $derived(session.membership?.household_id);
  const me = $derived(session.membership?.membership_id);
  const account = $derived(taskStore.myAccount);
  const goal = $derived(taskStore.period?.goal);
  const familyPct = $derived(goal ? Math.min(100, (goal.family_xp / Math.max(1, goal.target)) * 100) : 0);
  const krumelPct = $derived(goal ? Math.min(100, (goal.krumel_xp / Math.max(1, goal.target)) * 100) : 0);
  const PAGE = 20;
  /** Only what moved XP (older Guthaben entries are not shown any more). */
  const shown = $derived(history.filter((e) => e.contribution_delta !== 0));

  async function loadHistory(append = false) {
    if (!hid) return;
    const before = append && history.length ? `&before=${encodeURIComponent(history[history.length - 1].created_at)}` : "";
    const whose = viewing && viewing !== me ? `&member=${viewing}` : "";
    const rows = await session.request<LedgerEntryDto[]>("GET", `/households/${hid}/ledger?limit=${PAGE}${before}${whose}`);
    history = append ? [...history, ...rows] : rows;
    more = rows.length === PAGE;
  }

  // Reload whenever the live channel reports changes (taskStore.version counts reloads).
  $effect(() => {
    void taskStore.version;
    void viewing;
    void loadHistory().catch((e) => (error = e.message));
  });

  /** Admins take back a task credit (counter-entry with a reason). */
  async function reverse(e: LedgerEntryDto) {
    busy = true;
    error = "";
    try {
      const r = await session.request<ReverseResponse>("POST", `/households/${hid}/ledger/${e.id}/reverse`, { note: reason.trim() });
      sent = `Zurückgenommen: ${r.xp} XP.`;
      reversing = null;
      reason = "";
      taskStore.scheduleReload();
      await loadHistory();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  /** What a ledger entry means, in words. */
  function describe(e: LedgerEntryDto): string {
    const task = e.task_title ? `„${e.task_title}“` : "Aufgabe";
    switch (e.kind) {
      case "task_reward":
        return task;
      case "bounty_payout":
        return `Bonus für ${task}`;
      case "settlement_bonus":
        return "Bonus aus dem Abschluss";
      case "penalty_deduction":
        return "Abzug aus dem Abschluss";
      case "settlement_zero_sum":
        return "Ausgleich aus dem Abschluss";
      case "manual_adjust":
        return e.task_title ? `Korrektur: „${e.task_title}“` : "Korrektur";
      default:
        return "Buchung";
    }
  }

  const day = new Intl.DateTimeFormat("de-DE", { weekday: "short", day: "numeric", month: "short" });
  const signed = (n: number) => (n > 0 ? `+${n}` : `${n}`);
</script>

<h1 class="page-title">Punkte</h1>

<section class="card account" aria-label="Deine XP">
  <div class="big">
    <span class="xp-badge" aria-hidden="true">XP</span>
    <div>
      <div class="num">{account?.xp_month ?? 0} <small>XP</small></div>
      <div class="muted small">diesen Monat</div>
    </div>
  </div>
  <div class="stats">
    <div><b class="flame" class:cold={!account?.streak.today}><Icon name="flame" size={16} />{account?.streak.days ?? 0}</b><span class="muted small">{account?.streak.days === 1 ? "Tag" : "Tage"} Serie</span></div>
    <div><b>{account?.xp_total ?? 0}</b><span class="muted small">XP insgesamt</span></div>
    <div><b>{account?.bonus_left ?? 0}</b><span class="muted small">XP zum Verschenken</span></div>
  </div>
  {#if account && account.waiting > 0}
    <p class="muted small">{account.waiting} {account.waiting === 1 ? "Aufgabe wartet" : "Aufgaben warten"} noch auf Bestätigung.</p>
  {/if}
  <p class="muted small">XP zum Verschenken setzt du als Bonus auf Aufgaben anderer: Wer sie erledigt, bekommt sie obendrauf.</p>
  {#if sent}<p class="ok" role="status">{sent}</p>{/if}
  {#if error}<p class="err" role="alert">{error}</p>{/if}
</section>

<h2 class="section-title">Monatsziel</h2>
{#if goal}
  <section class="card goal" aria-label="Monatsziel der Familie">
    <div class="goal-head">
      <span class="goal-ic" aria-hidden="true"><Icon name="flag" size={20} /></span>
      <div class="goal-text">
        <b>{goal.target} XP gemeinsam</b>
        <span class="muted small">Belohnung: {goal.reward}</span>
      </div>
      {#if session.isAdmin}<button class="edit-goal" onclick={() => (goalSheet = true)}><Icon name="edit" size={16} />Ändern</button>{/if}
    </div>
    <div class="race" aria-hidden="true">
      <div class="lane"><span class="who">Wir</span><span class="track"><i class="fam" style="width: {familyPct}%"></i></span><b>{goal.family_xp}</b></div>
      <div class="lane"><span class="who"><Krumel size={18} /></span><span class="track"><i class="krumel" style="width: {krumelPct}%"></i></span><b>{goal.krumel_xp}</b></div>
    </div>
    <p class="muted small">
      {goal.status === "won" ? `Geschafft! ${goal.reward} gehört euch.` : goal.status === "lost" ? "Krümel war diesmal schneller." : goal.family_xp >= goal.krumel_xp ? "Ihr liegt vor Krümel." : `Krümel liegt ${goal.krumel_xp - goal.family_xp} XP vorn.`}
    </p>
  </section>
{:else}
  <p class="muted">Das Monatsziel erscheint, sobald der Monat läuft.</p>
{/if}

{#if taskStore.period?.settlements.length}
  <h2 class="section-title">Abschluss</h2>
  <div class="stack">
    {#each taskStore.period.settlements.slice(0, 2) as s (s.id)}<SettlementCard settlement={s} />{/each}
  </div>
{/if}

<h2 class="section-title">Verlauf</h2>
{#if session.isAdmin && session.members.length > 1}
  <div class="whose" role="radiogroup" aria-label="Wessen Verlauf">
    {#each session.members as m (m.membership_id)}
      {@const mine = m.membership_id === me}
      <button role="radio" aria-checked={(viewing ?? me) === m.membership_id} class:sel={(viewing ?? me) === m.membership_id}
        onclick={() => ((viewing = mine ? null : m.membership_id), (reversing = null))}>
        <Avatar animal={m.animal as AnimalName} color={m.color} size={22} />{mine ? "Ich" : m.display_name}
      </button>
    {/each}
  </div>
{/if}
{#if shown.length === 0}
  <p class="muted">Noch keine XP. Erledige eine Aufgabe, dann taucht sie hier auf.</p>
{:else}
  <ul class="card history">
    {#each shown as e (e.id)}
      <li>
        <div class="what">
          <span>{describe(e)}</span>
          {#if e.note}<span class="note">„{e.note}“{#if e.created_by && e.created_by !== me && e.kind === "task_reward"}{` · ${memberName(e.created_by)}`}{/if}</span>{/if}
          <span class="muted small">{day.format(new Date(e.created_at))}</span>
        </div>
        <div class="amounts">
          <span class="xp-chip" class:struck={e.reversed}>{signed(e.contribution_delta)} XP</span>
          {#if e.reversed}<span class="muted small">zurückgenommen</span>
          {:else if viewing && viewing !== me && e.kind === "task_reward" && reversing !== e.id}
            <button class="undo" onclick={() => ((reversing = e.id), (reason = ""))}>Zurücknehmen</button>
          {/if}
        </div>
        {#if reversing === e.id}
          <form class="reverse" onsubmit={(ev) => (ev.preventDefault(), reverse(e))}>
            <input id="reverse-reason" type="text" bind:value={reason} maxlength="200" required placeholder="Warum? (sieht {memberName(viewing)})" />
            <div class="two">
              <button type="button" onclick={() => (reversing = null)}>Abbrechen</button>
              <button class="primary" type="submit" disabled={busy || !reason.trim()}>Zurücknehmen</button>
            </div>
            <p class="muted small">Die XP dieser Aufgabe werden gegengebucht.</p>
          </form>
        {/if}
      </li>
    {/each}
  </ul>
  {#if more}<button class="more" onclick={() => loadHistory(true)}>Ältere laden</button>{/if}
{/if}

<h2 class="section-title">Familie</h2>
<div class="card family">
  {#each taskStore.accounts as a (a.membership_id)}
    {@const m = session.members.find((x) => x.membership_id === a.membership_id)}
    {#if m}
      <div class="member">
        <Avatar animal={m.animal as AnimalName} color={m.color} size={32} />
        <span class="name">{m.display_name}</span>
        <span class="xp-chip">{a.xp_month} XP</span>
        <span class="flame small" class:cold={!a.streak.today}><Icon name="flame" size={14} />{a.streak.days}</span>
      </div>
    {/if}
  {/each}
</div>

{#if goalSheet}
  <GoalSheet onclose={() => (goalSheet = false)} />
{/if}

<style>
  .account {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin-top: var(--space-3);
  }

  .big {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .xp-badge {
    width: 52px;
    height: 52px;
    border-radius: 16px;
    background: var(--xp-soft);
    color: var(--xp);
    display: grid;
    place-items: center;
    font-family: var(--font-display);
    font-weight: 800;
    font-size: 1.1rem;
  }

  .num {
    font-family: var(--font-display);
    font-size: 2.2rem;
    font-weight: 700;
    line-height: 1.1;
  }

  .num small {
    font-size: 1rem;
    color: var(--xp);
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--space-2);
    text-align: center;
  }

  .stats > div {
    display: flex;
    flex-direction: column;
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
  }

  .stats b {
    font-size: 1.2rem;
    font-variant-numeric: tabular-nums;
  }

  .flame {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    color: var(--over);
  }

  .flame.cold {
    color: var(--muted);
  }

  /* ---- monthly goal ---- */
  .goal {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .goal-head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .goal-ic {
    width: 40px;
    height: 40px;
    border-radius: 13px;
    background: var(--primary-soft);
    color: var(--primary);
    display: grid;
    place-items: center;
    flex: none;
  }

  .goal-text {
    flex: 1;
    display: flex;
    flex-direction: column;
  }

  .goal-text b {
    font-family: var(--font-display);
    font-size: 1.1rem;
  }

  .edit-goal {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 38px;
    padding: 0 12px;
    border-radius: 999px;
    background: var(--surface-2);
    font-size: 0.85rem;
  }

  .race {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .lane {
    display: grid;
    grid-template-columns: 34px 1fr 44px;
    align-items: center;
    gap: var(--space-2);
  }

  .lane .who {
    font-size: 0.8rem;
    font-weight: 800;
    display: grid;
    place-items: center;
  }

  .lane b {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .track {
    height: 10px;
    border-radius: 999px;
    background: var(--surface-2);
    overflow: hidden;
  }

  .track i {
    display: block;
    height: 100%;
    border-radius: inherit;
  }

  .track .fam {
    background: var(--primary);
  }

  .track .krumel {
    background: var(--krumel);
  }

  /* ---- history ---- */
  .two {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 1fr;
    gap: var(--space-2);
  }

  .history {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .history li {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: var(--space-2);
    align-items: center;
    padding: var(--space-3);
    border-bottom: 1px solid var(--line);
  }

  .history li:last-child {
    border-bottom: none;
  }

  .what {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .note {
    font-size: 0.85rem;
    font-style: italic;
  }

  .amounts {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
  }

  .whose {
    display: flex;
    gap: 6px;
    overflow-x: auto;
    margin-bottom: var(--space-2);
  }

  .whose button {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 38px;
    padding: 0 12px 0 6px;
    border-radius: 999px;
    font-size: 0.85rem;
  }

  .whose button.sel {
    background: var(--primary-soft);
    color: var(--primary);
  }

  .struck {
    text-decoration: line-through;
    opacity: 0.6;
  }

  .undo {
    min-height: 30px;
    padding: 0 8px;
    background: transparent;
    color: var(--over);
    font-size: 0.8rem;
  }

  .reverse {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .more {
    width: 100%;
    margin-top: var(--space-2);
  }

  .family {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .member {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    align-items: center;
    gap: var(--space-3);
  }

  .name {
    font-weight: 700;
  }

  .ok {
    color: var(--ok);
  }

  .err {
    color: var(--over);
  }
</style>
