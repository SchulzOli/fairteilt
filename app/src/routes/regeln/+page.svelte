<script lang="ts">
  // Household rules (KONZEPT §4.1, §5.1, §6): everyone can read them, admins change them.
  import SettlementPreview from "$lib/SettlementPreview.svelte";
  import type { DeficitAction, RulesDto, SurplusAction } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { taskStore } from "$lib/tasks.svelte";
  import Icon from "$lib/ui/Icon.svelte";

  let rules = $state<RulesDto | null>(null);
  let busy = $state(false);
  let error = $state("");
  let saved = $state(false);

  const hid = $derived(session.membership?.household_id);
  const admin = $derived(session.isAdmin);
  const WEEKDAYS = ["Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag", "Sonntag"];

  $effect(() => {
    if (!hid) return;
    session.request<RulesDto>("GET", `/households/${hid}/rules`).then((r) => (rules = r), (e) => (error = e.message));
  });

  type Deficit = DeficitAction["type"];
  type Surplus = SurplusAction["type"];
  const percentOf = (a: DeficitAction | SurplusAction) => ("percent" in a ? a.percent : 100);

  function setDeficit(type: Deficit) {
    if (!rules) return;
    const p = percentOf(rules.period_policy.deficit_action);
    rules.period_policy.deficit_action =
      type === "carry_over" || type === "balance_deduction" ? { type, percent: p } : ({ type } as DeficitAction);
  }

  function setSurplus(type: Surplus) {
    if (!rules) return;
    const p = percentOf(rules.period_policy.surplus_action);
    rules.period_policy.surplus_action = type === "none" ? { type } : { type, percent: p };
  }

  function setLength(type: "monthly" | "weekly" | "custom_days") {
    if (!rules) return;
    rules.period_policy.length =
      type === "monthly" ? { type, start_day: 1 } : type === "weekly" ? { type, start_weekday: 1 } : { type, days: 14 };
  }

  async function save() {
    if (!rules) return;
    busy = true;
    error = "";
    saved = false;
    try {
      rules = await session.request<RulesDto>("PUT", `/households/${hid}/rules`, rules);
      feedback("confirm");
      saved = true;
      taskStore.scheduleReload();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<a class="back" href="/familie"><Icon name="back" size={18} />Familie</a>
<h1 class="page-title">Regeln</h1>
<p class="muted small">{admin ? "Änderungen gelten sofort; ein anderer Zeitraum ab dem nächsten Abschluss." : "Nur Admins können die Regeln ändern."}</p>

{#if rules}
  {@const pp = rules.point_policy}
  {@const per = rules.period_policy}
  <fieldset class="card stack" disabled={!admin}>
    <legend>Aufgaben und Punkte</legend>
    <div class="field"><span>Wer braucht eine Bestätigung?</span>
      <div class="seg">
        <button type="button" class:sel={pp.approval === "children_only"} onclick={() => (pp.approval = "children_only")}>Nur Kinder</button>
        <button type="button" class:sel={pp.approval === "everyone"} onclick={() => (pp.approval = "everyone")}>Alle</button>
        <button type="button" class:sel={pp.approval === "nobody"} onclick={() => (pp.approval = "nobody")}>Niemand</button>
      </div>
    </div>
    <label class="toggle"><span>Beim Bestätigen XP anpassen</span><input type="checkbox" bind:checked={pp.quality_adjust} /></label>
    <label class="field"><span>Neue Aufgaben: {pp.default_points} XP (erlaubt {pp.task_min} bis {pp.task_max})</span>
      <input type="range" min={pp.task_min} max={pp.task_max} bind:value={pp.default_points} />
    </label>
    <label class="toggle"><span>Liegen gebliebene Aufgaben werden wertvoller</span>
      <input type="checkbox" checked={pp.escalation !== null}
        onchange={(e) => (pp.escalation = e.currentTarget.checked ? { step_points: 2, step_interval_secs: 86_400 } : null)} />
    </label>
    {#if pp.escalation}
      <label class="field"><span>+{pp.escalation.step_points} XP pro Tag</span><input type="range" min="1" max="10" bind:value={pp.escalation.step_points} /></label>
    {/if}
  </fieldset>

  <fieldset class="card stack" disabled={!admin}>
    <legend>XP-Bonus auf Aufgaben</legend>
    <p class="muted small">Jeder kann XP als Bonus auf Aufgaben anderer setzen. Das kostet nichts; wer die Aufgabe erledigt, bekommt sie obendrauf (bei mehreren geteilt).</p>
    <label class="field"><span>{pp.bonus_xp_per_month > 0 ? `Höchstens ${pp.bonus_xp_per_month} XP pro Person und Monat` : "Aus"}</span>
      <input type="range" min="0" max="200" step="5" bind:value={pp.bonus_xp_per_month} />
    </label>
  </fieldset>

  <fieldset class="card stack" disabled={!admin}>
    <legend>Abschluss</legend>
    <div class="field"><span>Zeitraum</span>
      <div class="seg">
        <button type="button" class:sel={per.length.type === "monthly"} onclick={() => setLength("monthly")}>Monat</button>
        <button type="button" class:sel={per.length.type === "weekly"} onclick={() => setLength("weekly")}>Woche</button>
        <button type="button" class:sel={per.length.type === "custom_days"} onclick={() => setLength("custom_days")}>Tage</button>
      </div>
      {#if per.length.type === "monthly"}
        <label class="inline">ab dem <input type="number" min="1" max="28" bind:value={per.length.start_day} />. des Monats</label>
      {:else if per.length.type === "weekly"}
        <label class="inline">ab <select bind:value={per.length.start_weekday}>{#each WEEKDAYS as d, i (d)}<option value={i + 1}>{d}</option>{/each}</select></label>
      {:else}
        <label class="inline">alle <input type="number" min="1" max="90" bind:value={per.length.days} /> Tage</label>
      {/if}
    </div>
    <label class="field"><span>Toleranz: ±{per.tolerance_percent} % ohne Folgen</span><input type="range" min="0" max="50" step="5" bind:value={per.tolerance_percent} /></label>
    <div class="field"><span>Bei Rückstand</span>
      <select value={per.deficit_action.type} onchange={(e) => setDeficit(e.currentTarget.value as Deficit)}>
        <option value="carry_over">Wandert in den nächsten Zeitraum</option>
        <option value="penalty_tasks">Aufholaufgaben</option>
        <option value="none">Nichts</option>
      </select>
      {#if per.deficit_action.type === "carry_over"}
        <label class="inline">zu <input type="number" min="0" max="200" bind:value={per.deficit_action.percent} /> %</label>
      {/if}
    </div>
    <div class="field"><span>Bei Vorsprung</span>
      <select value={per.surplus_action.type} onchange={(e) => setSurplus(e.currentTarget.value as Surplus)}>
        <option value="carry_over">Wandert in den nächsten Zeitraum</option>
        <option value="none">Nichts</option>
      </select>
      {#if per.surplus_action.type !== "none"}
        <label class="inline">zu <input type="number" min="0" max="200" bind:value={per.surplus_action.percent} /> %</label>
      {/if}
    </div>
    <label class="toggle"><span>Automatisch freigeben</span>
      <input type="checkbox" checked={per.auto_approve_after_hours !== null} onchange={(e) => (per.auto_approve_after_hours = e.currentTarget.checked ? 24 : null)} />
    </label>
    {#if per.auto_approve_after_hours !== null}
      <label class="inline">nach <input type="number" min="1" max="336" bind:value={per.auto_approve_after_hours} /> Stunden</label>
    {/if}
  </fieldset>

  <fieldset class="card stack" disabled={!admin}>
    <legend>Familienziel</legend>
    <label class="field"><span>Belohnung</span><input type="text" maxlength="60" bind:value={rules.goal_policy.reward} /></label>
    <label class="toggle"><span>Festes Ziel</span>
      <input type="checkbox" checked={rules.goal_policy.target !== null}
        onchange={(e) => rules && (rules.goal_policy.target = e.currentTarget.checked ? (taskStore.period?.goal.target ?? 200) : null)} />
    </label>
    {#if rules.goal_policy.target !== null}
      <label class="inline"><input type="number" min="1" max="100000" step="10" bind:value={rules.goal_policy.target} /> XP pro Zeitraum</label>
    {:else}
      <p class="muted small">Fairteilt schlägt jeden Zeitraum vor, was ihr im letzten geschafft habt.</p>
    {/if}
    <label class="field"><span>Schub für Krümel: {rules.goal_policy.boost_percent} % der XP jeder überfälligen Aufgabe pro Tag</span>
      <input type="range" min="0" max="100" step="5" bind:value={rules.goal_policy.boost_percent} />
    </label>
  </fieldset>

  {#if admin}
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    {#if saved}<p class="ok" role="status">Gespeichert.</p>{/if}
    <button class="primary save" disabled={busy} onclick={save}>Regeln speichern</button>
  {/if}

  <details class="preview">
    <summary>Abschluss ausprobieren</summary>
    <SettlementPreview />
  </details>
{:else if error}
  <p class="err">{error}</p>
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

  fieldset {
    margin: var(--space-4) 0 0;
    min-width: 0;
  }

  legend {
    float: left;
    width: 100%;
    font-family: var(--font-display);
    font-weight: 700;
    font-size: 1.1rem;
    margin-bottom: var(--space-1);
  }

  fieldset:disabled {
    opacity: 0.85;
  }

  .toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    font-weight: 700;
  }

  .inline {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: 400;
  }

  .inline input {
    width: 90px;
  }

  input[type="range"] {
    accent-color: var(--primary);
  }

  .save {
    width: 100%;
    margin-top: var(--space-4);
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

  .preview {
    margin-top: var(--space-5);
  }

  .preview summary {
    cursor: pointer;
    color: var(--muted);
    font-weight: 700;
  }
</style>
