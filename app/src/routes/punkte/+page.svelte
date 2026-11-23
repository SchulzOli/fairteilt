<script lang="ts">
  // Punkte (KONZEPT §7.3): own account, transfers, history; the family's accounts in member
  // order (no ranking). XP measure work, G (Guthaben) can be given away.
  import SettlementPreview from "$lib/SettlementPreview.svelte";
  import type { LedgerEntryDto, ReverseResponse, TransferDto } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { memberName, taskStore } from "$lib/tasks.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import type { AnimalName } from "$lib/ui/Avatar.svelte";
  import Icon from "$lib/ui/Icon.svelte";

  let history = $state<LedgerEntryDto[]>([]);
  let more = $state(false);
  let transfers = $state<TransferDto[]>([]);
  let sending = $state(false);
  let to = $state<string | null>(null);
  let amount = $state(5);
  let note = $state("");
  let busy = $state(false);
  let error = $state("");
  let sent = $state("");
  /** Admins: whose history is shown (null = your own). */
  let viewing = $state<string | null>(null);
  let reversing = $state<string | null>(null);
  let reason = $state("");

  const hid = $derived(session.membership?.household_id);
  const me = $derived(session.membership?.membership_id);
  const account = $derived(taskStore.myAccount);
  const others = $derived(session.members.filter((m) => m.membership_id !== me));
  const pending = $derived(transfers.filter((t) => t.status === "pending"));
  const isChild = $derived(session.membership?.role === "child");
  const PAGE = 20;

  async function loadHistory(append = false) {
    if (!hid) return;
    const before = append && history.length ? `&before=${encodeURIComponent(history[history.length - 1].created_at)}` : "";
    const whose = viewing && viewing !== me ? `&member=${viewing}` : "";
    const rows = await session.request<LedgerEntryDto[]>("GET", `/households/${hid}/ledger?limit=${PAGE}${before}${whose}`);
    history = append ? [...history, ...rows] : rows;
    more = rows.length === PAGE;
  }

  async function loadTransfers() {
    if (!hid) return;
    transfers = await session.request<TransferDto[]>("GET", `/households/${hid}/transfers`);
  }

  // Reload whenever the live channel reports changes (taskStore.version counts reloads).
  $effect(() => {
    void taskStore.version;
    void loadHistory().catch((e) => (error = e.message));
    void loadTransfers().catch(() => {});
  });

  async function send(e: SubmitEvent) {
    e.preventDefault();
    if (!to) return (error = "Bitte wähle, an wen.");
    busy = true;
    error = "";
    try {
      const t = await session.request<TransferDto>("POST", `/households/${hid}/transfers`, { to, amount, note: note.trim() });
      feedback("coin");
      sent = t.status === "pending" ? `Ein Erwachsener gibt die ${t.amount} G an ${memberName(t.to_id)} noch frei.` : `${t.amount} G an ${memberName(t.to_id)} geschickt.`;
      sending = false;
      note = "";
      taskStore.scheduleReload();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  /** Admins take back a task credit (counter-entry with a reason). */
  async function reverse(e: LedgerEntryDto) {
    busy = true;
    error = "";
    try {
      const r = await session.request<ReverseResponse>("POST", `/households/${hid}/ledger/${e.id}/reverse`, { note: reason.trim() });
      sent = `Zurückgenommen: ${r.xp} XP${r.g ? ` und ${r.g} G` : ""}.`;
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

  async function decide(t: TransferDto, approve: boolean) {
    busy = true;
    error = "";
    try {
      await session.request("POST", `/transfers/${t.id}/decide`, { approve });
      taskStore.scheduleReload();
      await loadTransfers();
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
      case "bounty_hold":
        return `Bonus auf ${task}`;
      case "bounty_payout":
        return `Bonus für ${task}`;
      case "bounty_refund":
        return `Bonus zurück: ${task}`;
      case "transfer_in":
        return `Von ${memberName(e.counterpart_id)}`;
      case "transfer_out":
        return `An ${memberName(e.counterpart_id)}`;
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

<section class="card account" aria-label="Dein Konto">
  <div class="big">
    <span class="coin" aria-hidden="true">G</span>
    <div>
      <div class="num">{account?.balance ?? 0} <small>G</small></div>
      <div class="muted small">Guthaben zum Verschenken und für Boni</div>
    </div>
  </div>
  <div class="stats">
    <div><b>{account?.xp_month ?? 0}</b><span class="muted small">XP diesen Monat</span></div>
    <div><b class="flame"><Icon name="flame" size={16} />{account?.streak.days ?? 0}</b><span class="muted small">{account?.streak.days === 1 ? "Tag" : "Tage"} Serie</span></div>
    <div><b>{account?.xp_total ?? 0}</b><span class="muted small">XP insgesamt</span></div>
  </div>
  {#if account && account.waiting > 0}
    <p class="muted small">{account.waiting} {account.waiting === 1 ? "Aufgabe wartet" : "Aufgaben warten"} noch auf Bestätigung.</p>
  {/if}
  {#if sent}<p class="ok" role="status">{sent}</p>{/if}
  {#if !sending}
    <button class="primary" disabled={(account?.balance ?? 0) <= 0} onclick={() => ((sending = true), (sent = ""), (amount = Math.min(5, account?.balance ?? 0)))}>
      Guthaben übertragen
    </button>
  {:else}
    <form class="stack" onsubmit={send}>
      <div class="field"><span>An wen?</span>
        <div class="people">
          {#each others as m (m.membership_id)}
            <button type="button" class="person" class:sel={to === m.membership_id} onclick={() => (to = m.membership_id)}>
              <Avatar animal={m.animal as AnimalName} color={m.color} size={28} />{m.display_name}
            </button>
          {/each}
        </div>
      </div>
      <div class="field"><span>Wie viel?</span>
        <div class="stepper">
          <button type="button" aria-label="Weniger" disabled={amount <= 1} onclick={() => (amount = Math.max(1, amount - 5))}>−</button>
          <b>{amount} G</b>
          <button type="button" aria-label="Mehr" disabled={amount >= (account?.balance ?? 0)} onclick={() => (amount = Math.min(account?.balance ?? 0, amount + 5))}>+</button>
        </div>
      </div>
      <label class="field"><span>Wofür?</span>
        <input id="transfer-note" type="text" bind:value={note} maxlength="200" required placeholder="z. B. Danke fürs Müll-Rausbringen" />
      </label>
      {#if isChild}<p class="muted small">Ein Erwachsener gibt die Übertragung noch frei.</p>{/if}
      <div class="two">
        <button type="button" onclick={() => (sending = false)}>Abbrechen</button>
        <button class="primary" type="submit" disabled={busy || !to || !note.trim()}>Schicken</button>
      </div>
    </form>
  {/if}
  {#if error}<p class="err" role="alert">{error}</p>{/if}
</section>

{#if session.isAdmin && pending.length}
  <h2 class="section-title">Freigeben</h2>
  <div class="stack">
    {#each pending as t (t.id)}
      <article class="card release">
        <div><b>{memberName(t.from_id)}</b> möchte <b>{t.amount} G</b> an <b>{memberName(t.to_id)}</b> geben</div>
        <div class="muted small">„{t.note}“</div>
        <div class="two">
          <button disabled={busy} onclick={() => decide(t, false)}>Ablehnen</button>
          <button class="primary" disabled={busy} onclick={() => decide(t, true)}>Freigeben</button>
        </div>
      </article>
    {/each}
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
{#if history.length === 0}
  <p class="muted">Noch keine Buchungen. Erledige eine Aufgabe, dann taucht sie hier auf.</p>
{:else}
  <ul class="card history">
    {#each history as e (e.id)}
      <li>
        <div class="what">
          <span>{describe(e)}</span>
          {#if e.note && e.kind !== "transfer_in" && e.kind !== "transfer_out"}<span class="note">„{e.note}“{#if e.created_by && e.created_by !== me && e.kind === "task_reward"}{` · ${memberName(e.created_by)}`}{/if}</span>
          {:else if e.note}<span class="note">„{e.note}“</span>{/if}
          <span class="muted small">{day.format(new Date(e.created_at))}</span>
        </div>
        <div class="amounts">
          {#if e.contribution_delta !== 0}<span class="xp-chip" class:struck={e.reversed}>{signed(e.contribution_delta)} XP</span>{/if}
          {#if e.balance_delta !== 0}<span class="g" class:minus={e.balance_delta < 0} class:struck={e.reversed}>{signed(e.balance_delta)} G</span>{/if}
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
            <p class="muted small">Die XP und das Guthaben dieser Aufgabe werden gegengebucht, ein Bonus bleibt.</p>
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
        <span class="muted small">{a.xp_month} XP</span>
        <span class="g">{a.balance} G</span>
        <span class="flame small" class:cold={!a.streak.today}><Icon name="flame" size={14} />{a.streak.days}</span>
      </div>
    {/if}
  {/each}
</div>

<details class="preview">
  <summary>Ausgleich ausprobieren (Vorschau)</summary>
  <SettlementPreview />
</details>

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

  .coin {
    width: 52px;
    height: 52px;
    border-radius: 50%;
    background: var(--coin-bg);
    color: #5a3a04;
    display: grid;
    place-items: center;
    font-family: var(--font-display);
    font-weight: 800;
    font-size: 1.4rem;
    box-shadow: inset 0 -3px 0 rgb(0 0 0 / 0.15);
  }

  .num {
    font-family: var(--font-display);
    font-size: 2.2rem;
    font-weight: 700;
    line-height: 1.1;
  }

  .num small {
    font-size: 1rem;
    color: var(--coin);
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

  .people {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    font-weight: 400;
  }

  .person {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 12px 0 4px;
    border-radius: 999px;
    min-height: 40px;
  }

  .person.sel {
    background: var(--primary-soft);
    border-color: var(--primary);
    color: var(--primary);
    font-weight: 700;
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

  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-2);
  }

  .release {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .history {
    list-style: none;
    margin: 0;
    padding: 0 var(--space-4);
  }

  .history li {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) 0;
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
    font-style: italic;
    font-size: 0.9rem;
  }

  .amounts {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
    white-space: nowrap;
  }

  .g {
    font-weight: 800;
    color: var(--coin);
    font-variant-numeric: tabular-nums;
  }

  .g.minus {
    color: var(--muted);
  }

  .whose {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: var(--space-3);
  }

  .whose button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
    padding: 0 12px 0 4px;
    border-radius: 999px;
    font-size: 0.9rem;
  }

  .whose button.sel {
    background: var(--primary-soft);
    border-color: var(--primary);
    color: var(--primary);
    font-weight: 700;
  }

  .struck {
    text-decoration: line-through;
    opacity: 0.6;
  }

  .undo {
    min-height: 30px;
    padding: 0 10px;
    font-size: 0.8rem;
  }

  .reverse {
    flex-basis: 100%;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .more {
    display: block;
    margin: var(--space-3) auto 0;
  }

  .family {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .member {
    display: grid;
    grid-template-columns: 32px 1fr auto auto auto;
    align-items: center;
    gap: var(--space-3);
  }

  .name {
    font-weight: 700;
  }

  .ok {
    color: var(--ok);
    font-weight: 700;
  }

  .err {
    color: var(--over);
  }

  .preview {
    margin-top: var(--space-5);
  }

  .preview summary {
    cursor: pointer;
    color: var(--muted);
    font-weight: 700;
  }
</style>
