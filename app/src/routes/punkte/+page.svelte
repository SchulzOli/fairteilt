<script lang="ts">
  import { api, errorMessage, isTauri, type PeriodPolicy, type Settlement } from "$lib/api";
  import { members } from "$lib/demo";

  // Interactive settlement preview, computed by punkte-core in the Tauri core (offline).
  let contributions = $state<Record<string, number>>({
    [members[0].id]: 70,
    [members[1].id]: 45,
    [members[2].id]: 20,
  });
  let deficit = $state<"carry_over" | "penalty_tasks" | "zero_sum" | "balance_deduction">("carry_over");
  let tolerance = $state(20);
  let settlement = $state<Settlement | null>(null);
  let error = $state<string | null>(null);

  const policy = $derived<PeriodPolicy>({
    length: { type: "monthly", start_day: 1 },
    mode: "fair_share",
    tolerance_percent: tolerance,
    deficit_action:
      deficit === "balance_deduction" || deficit === "carry_over"
        ? { type: deficit, percent: deficit === "carry_over" ? 100 : 50 }
        : { type: deficit },
    // Carry the lead over when deficits carry over, otherwise pay a 50 % bonus.
    surplus_action: deficit === "carry_over" ? { type: "carry_over", percent: 100 } : { type: "bonus", percent: 50 },
    auto_approve_after_hours: 24,
    warn_before_end_hours: 48,
  });

  $effect(() => {
    const input = members.map((m) => ({
      member_id: m.id,
      weight: m.weight,
      availability: 1,
      contribution: contributions[m.id] ?? 0,
      carry_in: 0,
      quota: null,
    }));
    if (!isTauri()) {
      error = "Die Vorschau läuft im Tauri-Kern. Bitte mit `pnpm tauri dev` starten.";
      return;
    }
    api
      .previewSettlement(policy, input)
      .then((s) => {
        settlement = s;
        error = null;
      })
      .catch((e) => (error = errorMessage(e)));
  });

  const nameOf = (id: string) => members.find((m) => m.id === id)?.name ?? "?";
  const outcomeLabel = { within_tolerance: "im Rahmen", deficit: "Fehlbetrag", surplus: "Überschuss" } as const;
</script>

<header>
  <h1>Punkte</h1>
  <p class="muted">Ausgleich des Monats ausprobieren: Leistungspunkte verschieben und sehen, was passiert.</p>
</header>

<section class="card stack">
  {#each members as m (m.id)}
    <label class="slider">
      <span class="who"><span class="dot" style="background: {m.color}"></span>{m.name}
        <span class="muted small">Gewicht {m.weight}</span></span>
      <input type="range" min="0" max="150" bind:value={contributions[m.id]} />
      <span class="num">{contributions[m.id]} LP</span>
    </label>
  {/each}

  <div class="grid2">
    <label class="stack tight">
      <span class="muted small">Bei Fehlbetrag</span>
      <select bind:value={deficit}>
        <option value="carry_over">Übertrag in nächsten Monat</option>
        <option value="penalty_tasks">Aufholaufgaben</option>
        <option value="zero_sum">Nullsumme (zahlt an andere)</option>
        <option value="balance_deduction">Guthaben-Abzug 50 %</option>
      </select>
    </label>
    <label class="stack tight">
      <span class="muted small">Toleranz: {tolerance} %</span>
      <input type="range" min="0" max="30" bind:value={tolerance} />
    </label>
  </div>
</section>

{#if error}
  <p class="card error">{error}</p>
{:else if settlement}
  <h2 class="section-title">Ergebnis · {settlement.total_contribution} LP gesamt</h2>
  <div class="stack">
    {#each settlement.lines as line (line.member_id)}
      <article class="card result {line.outcome}">
        <div class="row between">
          <strong>{nameOf(line.member_id)}</strong>
          <span class="chip">{outcomeLabel[line.outcome]}</span>
        </div>
        <div class="muted small">
          Ziel {line.target} · geleistet {line.contribution} ·
          Abweichung {line.deviation > 0 ? "+" : ""}{line.deviation}
        </div>
        {#if line.penalty_points_due > 0}
          <div class="small">⚠️ Aufholaufgaben im Wert von {line.penalty_points_due} P</div>
        {/if}
        {#if line.carry_next !== 0}
          <div class="small">↪ {line.carry_next > 0 ? `+${line.carry_next} Ziel` : `${line.carry_next} Ziel`} im nächsten Monat</div>
        {/if}
        {#if line.balance_delta !== 0}
          <div class="small">{line.balance_delta > 0 ? "💰 +" : "💸 "}{line.balance_delta} Guthaben</div>
        {/if}
      </article>
    {/each}
  </div>
{/if}

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

  .slider {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: var(--space-1) var(--space-3);
    align-items: center;
  }

  .slider input {
    grid-column: 1 / -1;
    grid-row: 2;
    width: 100%;
    accent-color: var(--accent);
  }

  .who {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: 600;
  }

  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }

  .num {
    font-variant-numeric: tabular-nums;
    font-weight: 700;
  }

  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-3);
  }

  .tight {
    gap: var(--space-1);
  }

  .section-title {
    margin: var(--space-5) 0 var(--space-3);
  }

  .between {
    justify-content: space-between;
  }

  .result {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    border-left-width: 4px;
  }

  .result.deficit {
    border-left-color: var(--danger);
  }

  .result.surplus {
    border-left-color: var(--success);
  }

  .error {
    color: var(--danger);
    margin-top: var(--space-4);
  }
</style>
