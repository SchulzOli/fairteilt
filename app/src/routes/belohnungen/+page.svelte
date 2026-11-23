<script lang="ts">
  // Reward catalog (KONZEPT §6): redeem G for rewards; admins manage the catalog and
  // approve requests. The family reward (race against Krümel) is shown on top.
  import type { RedemptionDto, RewardDto } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { memberName, taskStore } from "$lib/tasks.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Krumel from "$lib/ui/Krumel.svelte";

  let rewards = $state<RewardDto[]>([]);
  let redemptions = $state<RedemptionDto[]>([]);
  let confirm = $state<string | null>(null);
  let adding = $state(false);
  let busy = $state(false);
  let error = $state("");
  let done = $state("");

  // New reward form.
  let title = $state("");
  let description = $state("");
  let cost = $state(20);
  let needsApproval = $state(false);
  let limited = $state(false);
  let stock = $state(5);
  let cooldown = $state<number | null>(null);

  const hid = $derived(session.membership?.household_id);
  const me = $derived(session.membership?.membership_id);
  const balance = $derived(taskStore.myAccount?.balance ?? 0);
  const goal = $derived(taskStore.period?.goal);
  const mine = $derived(redemptions.filter((r) => r.member_id === me));
  const waiting = $derived(session.isAdmin ? redemptions.filter((r) => r.status === "requested" && r.member_id !== me) : []);

  async function load() {
    if (!hid) return;
    [rewards, redemptions] = await Promise.all([
      session.request<RewardDto[]>("GET", `/households/${hid}/rewards`),
      session.request<RedemptionDto[]>("GET", `/households/${hid}/redemptions`),
    ]);
  }

  $effect(() => {
    void taskStore.version;
    void load().catch((e) => (error = e.message));
  });

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

  const redeem = (r: RewardDto) =>
    run(async () => {
      const x = await session.request<RedemptionDto>("POST", `/rewards/${r.id}/redeem`);
      feedback(x.status === "approved" ? "coin" : "confirm");
      done = x.status === "approved" ? `„${r.title}“ gehört dir. Viel Spaß!` : `„${r.title}“ ist angefragt. Ein Erwachsener gibt es frei.`;
      confirm = null;
      taskStore.scheduleReload();
      await load();
    });

  const decide = (x: RedemptionDto, approve: boolean) =>
    run(async () => {
      await session.request("POST", `/redemptions/${x.id}/decide`, { approve });
      taskStore.scheduleReload();
      await load();
    });

  const archive = (r: RewardDto) =>
    run(async () => {
      await session.request("DELETE", `/households/${hid}/rewards/${r.id}`);
      await load();
    });

  const add = (e: SubmitEvent) => {
    e.preventDefault();
    return run(async () => {
      await session.request("POST", `/households/${hid}/rewards`, {
        title: title.trim(),
        description: description.trim() || null,
        cost,
        needs_approval: needsApproval,
        stock: limited ? stock : null,
        cooldown_hours: cooldown,
      });
      feedback("confirm");
      adding = false;
      title = description = "";
      await load();
    });
  };

  const when = new Intl.DateTimeFormat("de-DE", { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  const statusText: Record<string, string> = { requested: "angefragt", approved: "eingelöst", rejected: "abgelehnt, G zurück", fulfilled: "erledigt" };
</script>

<a class="back" href="/punkte"><Icon name="back" size={18} />Punkte</a>
<div class="row between">
  <h1 class="page-title">Belohnungen</h1>
  <span class="coin-chip">{balance}</span>
</div>

{#if goal}
  <section class="card family" class:won={goal.status === "won"}>
    <span class="buddy" aria-hidden="true"><Krumel size={36} /></span>
    <div>
      <b>Familienbelohnung: {goal.reward}</b>
      <div class="muted small">
        {goal.status === "won" ? "Geschafft! Ihr habt Krümel geschlagen." : goal.status === "lost" ? "Krümel war diesmal schneller." : `Gemeinsam ${goal.target} XP, bevor Krümel ankommt. Ihr habt ${goal.family_xp}.`}
      </div>
    </div>
  </section>
{/if}

{#if done}<p class="ok" role="status">{done}</p>{/if}
{#if error}<p class="err" role="alert">{error}</p>{/if}

{#if waiting.length}
  <h2 class="section-title">Freigeben</h2>
  <div class="stack">
    {#each waiting as x (x.id)}
      <article class="card stack tight">
        <div><b>{memberName(x.member_id)}</b> möchte <b>{x.title}</b> für {x.cost} G</div>
        <div class="two">
          <button disabled={busy} onclick={() => decide(x, false)}>Ablehnen</button>
          <button class="primary" disabled={busy} onclick={() => decide(x, true)}>Freigeben</button>
        </div>
      </article>
    {/each}
  </div>
{/if}

<h2 class="section-title">Katalog</h2>
{#if rewards.length === 0}
  <p class="muted">{session.isAdmin ? "Noch keine Belohnungen. Legt fest, wofür man Guthaben einlösen kann." : "Noch keine Belohnungen. Frag deine Eltern!"}</p>
{/if}
<div class="stack">
  {#each rewards as r (r.id)}
    {@const soldOut = r.left === 0}
    {@const later = r.available_at !== null}
    <article class="card reward">
      <div class="info">
        <b>{r.title}</b>
        {#if r.description}<span class="muted small">{r.description}</span>{/if}
        <span class="muted small">
          {[r.needs_approval ? "mit Freigabe" : null, r.left !== null ? `noch ${r.left}` : null, r.cooldown_hours ? `alle ${r.cooldown_hours} h` : null].filter(Boolean).join(" · ")}
        </span>
        {#if later}<span class="small wait">wieder ab {when.format(new Date(r.available_at!))}</span>{/if}
      </div>
      <span class="coin-chip">{r.cost}</span>
      {#if confirm === r.id}
        <div class="two wide">
          <button onclick={() => (confirm = null)}>Abbrechen</button>
          <button class="primary" disabled={busy} onclick={() => redeem(r)}>Für {r.cost} G einlösen</button>
        </div>
      {:else}
        <div class="actions wide">
          <button class="primary" disabled={busy || soldOut || later || balance < r.cost} onclick={() => ((confirm = r.id), (done = ""))}>
            {soldOut ? "Vergriffen" : later ? "Später wieder" : balance < r.cost ? `Noch ${r.cost - balance} G` : "Einlösen"}
          </button>
          {#if session.isAdmin}<button class="link" disabled={busy} onclick={() => archive(r)}>Entfernen</button>{/if}
        </div>
      {/if}
    </article>
  {/each}
</div>

{#if session.isAdmin}
  {#if adding}
    <form class="card stack add" onsubmit={add}>
      <label class="field"><span>Belohnung</span><input id="reward-title" type="text" bind:value={title} maxlength="60" required placeholder="z. B. Film aussuchen" /></label>
      <label class="field"><span>Beschreibung <small class="muted">(optional)</small></span><input id="reward-desc" type="text" bind:value={description} maxlength="200" /></label>
      <div class="field"><span>Preis</span>
        <div class="stepper">
          <button type="button" aria-label="Weniger" onclick={() => (cost = Math.max(1, cost - 5))}>−</button>
          <b>{cost} G</b>
          <button type="button" aria-label="Mehr" onclick={() => (cost += 5)}>+</button>
        </div>
      </div>
      <label class="toggle"><span>Braucht Freigabe</span><input type="checkbox" bind:checked={needsApproval} /></label>
      <label class="toggle"><span>Begrenzter Bestand</span><input type="checkbox" bind:checked={limited} /></label>
      {#if limited}<label class="field"><span>Wie viele?</span><input id="reward-stock" type="number" min="0" max="999" bind:value={stock} /></label>{/if}
      <div class="field"><span>Wie oft pro Person?</span>
        <div class="seg">
          <button type="button" class:sel={cooldown === null} onclick={() => (cooldown = null)}>Beliebig</button>
          <button type="button" class:sel={cooldown === 24} onclick={() => (cooldown = 24)}>1× am Tag</button>
          <button type="button" class:sel={cooldown === 168} onclick={() => (cooldown = 168)}>1× pro Woche</button>
        </div>
      </div>
      <div class="two">
        <button type="button" onclick={() => (adding = false)}>Abbrechen</button>
        <button class="primary" type="submit" disabled={busy || !title.trim()}>Anlegen</button>
      </div>
    </form>
  {:else}
    <button class="add-btn" onclick={() => (adding = true)}><Icon name="plus" size={18} />Belohnung hinzufügen</button>
  {/if}
{/if}

{#if mine.length}
  <h2 class="section-title">Deine Einlösungen</h2>
  <ul class="card mine">
    {#each mine as x (x.id)}
      <li><span>{x.title} <span class="muted small">· {when.format(new Date(x.created_at))}</span></span><span class="state {x.status}">{statusText[x.status] ?? x.status}</span></li>
    {/each}
  </ul>
{/if}

<style>
  .back {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-bottom: var(--space-3);
    color: var(--muted);
    font-weight: 700;
    text-decoration: none;
  }

  .between {
    justify-content: space-between;
  }

  .family {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin-top: var(--space-3);
  }

  .family.won {
    border-color: color-mix(in srgb, var(--coin) 50%, var(--line));
    background: color-mix(in srgb, var(--coin-soft) 45%, var(--surface));
  }

  .buddy {
    flex: none;
    width: 52px;
    height: 52px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--krumel) 18%, var(--surface));
  }

  .reward {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: var(--space-2) var(--space-3);
    align-items: start;
  }

  .info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .wide {
    grid-column: 1 / -1;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .actions .primary {
    flex: 1;
  }

  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-2);
  }

  .tight {
    gap: var(--space-2);
  }

  .wait {
    color: var(--coin);
    font-weight: 700;
  }

  .link {
    background: transparent;
    border: none;
    color: var(--muted);
    text-decoration: underline;
    min-height: 36px;
  }

  .add,
  .add-btn {
    margin-top: var(--space-3);
  }

  .add-btn {
    width: 100%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    background: transparent;
    border: 1px dashed var(--line);
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

  .toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-weight: 700;
  }

  .mine {
    list-style: none;
    margin: 0;
    padding: var(--space-2) var(--space-4);
  }

  .mine li {
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--line);
  }

  .mine li:last-child {
    border-bottom: none;
  }

  .state {
    font-size: 0.8rem;
    font-weight: 700;
    white-space: nowrap;
  }

  .state.requested {
    color: var(--coin);
  }

  .state.approved {
    color: var(--ok);
  }

  .state.rejected {
    color: var(--muted);
  }

  .ok {
    color: var(--ok);
    font-weight: 700;
    margin-top: var(--space-3);
  }

  .err {
    color: var(--over);
    margin-top: var(--space-3);
  }
</style>
