<script lang="ts">
  import { page } from "$app/state";
  import type { TaskDto, TaskStepDto } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { clock, conditionOf, dueText, iconOf, isQueued, memberName, stepsDone, taskStore } from "$lib/tasks.svelte";
  import ConditionBar from "$lib/ui/ConditionBar.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import ReviewSheet from "$lib/ReviewSheet.svelte";
  import { bySection, shopping } from "$lib/shopping.svelte";
  import ItemRow from "$lib/shop/ItemRow.svelte";

  const id = $derived(page.params.id ?? "");

  /** The task as last answered by the server for this page (also after it is completed). */
  let local = $state<TaskDto | null>(null);
  let finished = $state<{ xp: number; next: string | null; submitted: boolean; bounty: number | null; queued?: boolean } | null>(null);
  let reviewing = $state(false);
  let pledging = $state(false);
  let pledgeAmount = $state(5);
  let sparkle = $state<string | null>(null);
  let busy = $state(false);
  let error = $state("");

  const task = $derived(finished ? local : (taskStore.tasks.find((t) => t.id === id) ?? local));
  const now = $derived(taskStore.now);
  const me = $derived(session.membership?.membership_id);
  const room = $derived(task ? taskStore.roomOf(task) : undefined);
  const running = $derived(task?.status === "in_progress");
  const condition = $derived(task ? conditionOf(task, now) : 0);
  const done = $derived(task ? stepsDone(task) : 0);
  const openSteps = $derived(task ? task.steps.length - done : 0);
  const sequential = $derived(task?.step_order === "sequential");
  const checkinIn = $derived(
    task?.remind_at && Date.parse(task.remind_at) > now ? Math.ceil((Date.parse(task.remind_at) - now) / 60_000) : null,
  );
  const submitted = $derived(task?.status === "submitted");
  const canReview = $derived(submitted && session.membership?.role !== "child" && task?.completed_by !== me);
  const canPledge = $derived(
    !!task && !submitted && task.status !== "completed" && task.status !== "cancelled" && task.assignee_id !== me,
  );
  const balance = $derived(taskStore.myAccount?.balance ?? 0);
  const nextTask = $derived(finished?.next ? taskStore.tasks.find((t) => t.id === finished!.next) : undefined);

  // Shopping mode (M8): the list of this round's task, grouped as in the shop.
  const list = $derived(task ? shopping.listOf(task) : undefined);
  const toBuy = $derived(list ? list.items.filter((i) => !i.checked_at) : []);
  const inCart = $derived(list ? list.items.filter((i) => i.checked_at) : []);
  /** Bought this trip (kept for the thank-you after "Einkauf fertig"). */
  let bought = $state(0);
  let leftOver = $state<string[]>([]);
  /** Written down by someone else while I am in the shop. */
  const isFresh = (addedAt: string, by: string | null) =>
    !!task?.started_at && by !== me && Date.parse(addedAt) > Date.parse(task.started_at);

  // In the shop the screen stays on.
  $effect(() => {
    if (!list || !running || typeof navigator === "undefined" || !("wakeLock" in navigator)) return;
    let lock: WakeLockSentinel | null = null;
    navigator.wakeLock.request("screen").then((l) => (lock = l), () => {});
    return () => void lock?.release().catch(() => {});
  });

  // Not in the current list (e.g. opened from a link after it was done): fetch it.
  $effect(() => {
    if (taskStore.loaded && !taskStore.tasks.some((t) => t.id === id) && !local) {
      session.request<TaskDto>("GET", `/tasks/${id}`).then((t) => (local = t), (e) => (error = e.message));
    }
  });

  function canCheck(t: TaskDto, i: number): boolean {
    if (t.steps[i].done_at) return false;
    return !sequential || t.steps.slice(0, i).every((s) => s.done_at);
  }

  async function act(fn: () => Promise<void>) {
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

  const start = () => act(async () => void (local = (await taskStore.action(id, "start")).task));
  const pause = () => act(async () => void (local = (await taskStore.action(id, "pause")).task));
  const claim = () => act(async () => void (local = (await taskStore.action(id, "claim")).task));
  const skip = () => act(async () => void (local = (await taskStore.action(id, "cancel")).task));

  const complete = () =>
    act(async () => {
      bought = inCart.length;
      leftOver = toBuy.map((i) => i.name);
      const res = await taskStore.action(id, "done");
      local = res.task;
      finished = { xp: res.xp_awarded ?? 0, next: res.next_task_id, submitted: res.submitted, bounty: res.bounty_paid, queued: isQueued(res) };
      feedback("done");
    });

  const pledge = () =>
    act(async () => {
      local = await taskStore.pledge(id, pledgeAmount);
      pledging = false;
      feedback("coin");
    });

  const check = (step: TaskStepDto) =>
    act(async () => {
      const res = await taskStore.checkStep(id, step.id);
      local = res.task;
      sparkle = step.id;
      setTimeout(() => sparkle === step.id && (sparkle = null), 900);
      if (res.xp_awarded !== null || res.submitted || (isQueued(res) && !res.task.steps.some((x) => x.id !== step.id && !x.done_at))) {
        finished = { xp: res.xp_awarded ?? 0, next: res.next_task_id, submitted: res.submitted, bounty: res.bounty_paid, queued: isQueued(res) };
        feedback("done");
      } else {
        feedback("step");
      }
    });

  // Confetti for the whole task: more pieces than on a card, still inside the panel.
  const COLORS = ["var(--coin-bg)", "#d63384", "var(--xp)", "var(--primary)", "#e8660f", "var(--fresh)", "#2563eb", "var(--over)"];
  const pieces = Array.from({ length: 48 }, (_, i) => ({
    x: ((i * 61) % 300) - 150,
    y: -(40 + ((i * 37) % 110)),
    r: ((i * 71) % 500) - 250,
    w: [6, 4, 7, 3, 5][i % 5],
    h: [9, 7, 6, 10, 5][i % 5],
    d: ((i * 13) % 35) / 100,
    c: COLORS[i % COLORS.length],
    round: i % 4 === 1,
  }));

  function nextText(t: TaskDto): string {
    const who = memberName(t.assignee_id);
    return `Nächste Runde ${dueText(t, now).replace(" fällig", "")}${who ? ` · ${who} ist dran` : ""}.`;
  }
</script>

<a class="back" href="/"><Icon name="back" size={18} />Heute</a>

{#if !task}
  <p class="muted">{error || "Lädt …"}</p>
{:else}
  <header class="card head">
    <span class="ic" aria-hidden="true"><Icon name={iconOf(task.icon, iconOf(room?.icon))} size={26} /></span>
    <div>
      <h1 class="page-title">{task.title}</h1>
      <div class="muted small">
        {room?.name ?? "Ohne Raum"} ·
        {#if finished}erledigt{:else if task.assignee_id}{task.assignee_id === me ? "du bist" : `${memberName(task.assignee_id)} ist`} dran{:else}noch offen{/if}
      </div>
    </div>
    <span class="chips">
      <span class="xp-chip">{finished && !finished.submitted ? finished.xp : task.xp} XP</span>
      {#if task.bounty > 0 && !finished}<span class="coin-chip">+{task.bounty}</span>{/if}
    </span>
    {#if !finished}
      <div class="cond">
        <ConditionBar value={condition} />
        <span class="small" class:late={condition >= 1}>{dueText(task, now)}</span>
      </div>
    {/if}
  </header>

  {#if finished}
    <section class="card done-panel" aria-live="polite">
      <span class="confetti" aria-hidden="true">
        {#each pieces as p, i (i)}
          <i class:round={p.round} style="--x:{p.x}px;--y:{p.y}px;--r:{p.r}deg;--w:{p.w}px;--h:{p.h}px;--d:{p.d}s;background:{p.c}"></i>
        {/each}
      </span>
      <span class="big-check" aria-hidden="true"><Icon name="check" size={40} /></span>
      {#if finished.queued}
        <h2>Gespeichert!</h2>
        <p class="muted">Du bist gerade offline. Sobald die Verbindung wieder da ist, kommt es an und du bekommst deine XP.</p>
      {:else if finished.submitted}
        <h2>Prima!</h2>
        <p class="muted">Ein Erwachsener schaut noch kurz drüber, dann bekommst du deine {task.xp} XP.</p>
      {:else}
        <h2>{list ? "Einkauf fertig, danke!" : "Geschafft!"}</h2>
        <span class="xp-burst">+{finished.xp} XP{finished.bounty ? ` · +${finished.bounty} G` : ""}</span>
        {#if list && bought}<p class="muted">{bought} Artikel gekauft.</p>{/if}
        {#if list && leftOver.length}<p class="muted">Bleibt auf der Liste: {leftOver.join(", ")}. „{task.title}“ kommt gleich wieder.</p>{/if}
        {#if nextTask}<p class="muted">{nextText(nextTask)}</p>{/if}
      {/if}
      <a class="btnlike primary" href="/">Zurück zu Heute</a>
    </section>
  {:else if submitted}
    <section class="card status">
      {#if canReview}
        <p><b>{memberName(task.completed_by)}</b> ist fertig. Passt alles?</p>
        <button class="primary go" onclick={() => (reviewing = true)}><Icon name="check" size={20} />Prüfen und bestätigen</button>
      {:else}
        <p class="center"><b>Wartet auf Bestätigung.</b><br /><span class="muted small">Sobald ein Erwachsener zugestimmt hat, gibt es die XP.</span></p>
      {/if}
    </section>
  {:else}
    {#if task.review_note}
      <section class="card note"><span class="muted small">Nachricht</span><p>„{task.review_note}“</p></section>
    {/if}
    <section class="card status">
      {#if running && task.started_at}
        <div class="timer" aria-label="Läuft seit {clock(task.started_at, now)}">
          <span class="muted small">läuft seit</span>
          <span class="clock">{clock(task.started_at, now)}</span>
          <span class="muted small">Die Zeit zählt nur hoch.{checkinIn !== null ? ` In ${checkinIn} Min. fragt Fairteilt kurz nach.` : ""}</span>
        </div>
        {#if list}
          <div class="two">
            <button disabled={busy} onclick={pause}><Icon name="pause" size={18} />Pause</button>
            <button class="primary" disabled={busy || (inCart.length === 0 && toBuy.length > 0)} onclick={complete}><Icon name="check" size={18} />Einkauf fertig</button>
          </div>
          <p class="muted small center">{inCart.length} von {inCart.length + toBuy.length} im Wagen. Was übrig bleibt, kommt beim nächsten Mal.</p>
        {:else}
          <div class="two">
            <button disabled={busy} onclick={pause}><Icon name="pause" size={18} />Pause</button>
            <button class="primary" disabled={busy || openSteps > 0} onclick={complete}><Icon name="check" size={18} />Erledigt</button>
          </div>
          {#if openSteps > 0}<p class="muted small center">Noch {openSteps} {openSteps === 1 ? "Schritt" : "Schritte"}, dann ist sie erledigt.</p>{/if}
        {/if}
      {:else}
        <button class="primary go" disabled={busy} onclick={start}><Icon name={list ? "cart" : "play"} size={20} />{list ? "Einkauf starten" : "Los geht's"}</button>
        <div class="two">
          {#if task.status === "open"}<button disabled={busy} onclick={claim}>Übernehmen</button>{/if}
          {#if task.steps.length === 0 && !list}<button disabled={busy} onclick={complete}><Icon name="check" size={18} />Schon erledigt</button>{/if}
        </div>
        {#if list}<p class="muted small center">Abhaken legt in den Wagen. XP gibt es mit „Einkauf fertig“.</p>{/if}
      {/if}
      {#if error}<p class="err" role="alert">{error}</p>{/if}
    </section>

    {#if list}
      <section class="shop" aria-label="Einkaufsliste {list.name}">
        <div class="row between">
          <h2 class="sub">{list.name}</h2>
          <a class="muted small" href="/liste">Liste bearbeiten</a>
        </div>
        {#if toBuy.length + inCart.length > 0}
          <div class="progress" style="--n:{toBuy.length + inCart.length}" aria-hidden="true">
            {#each { length: toBuy.length + inCart.length } as _, i (i)}<i class:on={i < inCart.length}></i>{/each}
          </div>
        {/if}
        {#each bySection(toBuy) as g (g.section)}
          <h3 class="section-title">{g.label}</h3>
          <ul class="items">
            {#each g.items as item (item.id)}<ItemRow {item} mode="cart" fresh={isFresh(item.added_at, item.added_by)} />{/each}
          </ul>
        {:else}
          <p class="muted">{inCart.length ? "Alles im Wagen!" : "Nichts mehr auf der Liste."}</p>
        {/each}
        {#if inCart.length}
          <h3 class="section-title">Im Wagen</h3>
          <ul class="items">
            {#each inCart as item (item.id)}<ItemRow {item} mode="cart" />{/each}
          </ul>
        {/if}
      </section>
    {/if}

    {#if task.steps.length}
      <section class="card">
        <div class="row between">
          <h2 class="sub">Schritte</h2>
          <span class="muted small">{done}/{task.steps.length} · {sequential ? "der Reihe nach" : "beliebige Reihenfolge"}</span>
        </div>
        <ol class="steps">
          {#each task.steps as s, i (s.id)}
            {@const allowed = canCheck(task, i)}
            <li class:done={s.done_at} class:locked={!s.done_at && !allowed} class:sparkle={sparkle === s.id}>
              <button disabled={busy || !allowed} aria-label="{s.title}{s.done_at ? ' (erledigt)' : ''}" onclick={() => check(s)}>
                <span class="box" aria-hidden="true">
                  {#if s.done_at}<Icon name="check" size={16} />{:else if !allowed}<Icon name="lock" size={14} />{/if}
                </span>
                <span class="label">{s.title}</span>
                {#if s.done_by && s.done_by !== me}<span class="muted small">{memberName(s.done_by)}</span>{/if}
              </button>
              {#if sparkle === s.id}
                <span class="stars" aria-hidden="true">
                  {#each [0, 1, 2, 3, 4, 5] as k (k)}<i style="--a:{k * 60}deg"><Icon name="sparkle" size={12} /></i>{/each}
                </span>
              {/if}
            </li>
          {/each}
        </ol>
        <p class="muted small">XP gibt es, wenn der letzte Schritt erledigt ist.</p>
      </section>
    {/if}

    {#if canPledge}
      <section class="card bounty">
        {#if pledging}
          <div class="row between">
            <span>Bonus aus deinem Guthaben <span class="muted small">({balance} G)</span></span>
          </div>
          <div class="stepper">
            <button type="button" aria-label="Weniger" disabled={pledgeAmount <= 1} onclick={() => (pledgeAmount = Math.max(1, pledgeAmount - 5))}>−</button>
            <b>{pledgeAmount} G</b>
            <button type="button" aria-label="Mehr" disabled={pledgeAmount >= balance} onclick={() => (pledgeAmount = Math.min(balance, pledgeAmount + 5))}>+</button>
          </div>
          <div class="two">
            <button onclick={() => (pledging = false)}>Abbrechen</button>
            <button class="primary" disabled={busy || pledgeAmount > balance} onclick={pledge}>Bonus setzen</button>
          </div>
        {:else}
          <button class="link-btn" disabled={balance <= 0} onclick={() => ((pledgeAmount = Math.min(5, balance)), (pledging = true))}>
            <span class="coin-chip">G</span>{task.bounty > 0 ? "Bonus erhöhen" : "Bonus setzen"}{balance <= 0 ? " (kein Guthaben)" : ""}
          </button>
        {/if}
      </section>
    {/if}

    {#if session.isAdmin}
      <button class="link" disabled={busy} onclick={skip}>Diese Runde überspringen</button>
    {/if}
  {/if}
{/if}

{#if reviewing && task}
  <ReviewSheet {task} onclose={() => (reviewing = false)} />
{/if}

<style>
  .shop {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }

  .shop .section-title {
    margin: var(--space-3) 0 0;
  }

  .shop .items {
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .progress {
    display: grid;
    grid-template-columns: repeat(var(--n), 1fr);
    gap: 3px;
  }

  .progress i {
    height: 6px;
    border-radius: 3px;
    background: var(--surface-2);
  }

  .progress i.on {
    background: var(--primary);
  }

  .chips {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
  }

  .note {
    margin-bottom: var(--space-3);
    border-color: color-mix(in srgb, var(--coin) 40%, var(--line));
  }

  .note p {
    font-style: italic;
  }

  .bounty {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin-top: var(--space-3);
  }

  .stepper {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
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

  .link-btn {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    background: transparent;
    border: none;
    padding: 0;
    color: var(--fg);
    font-weight: 700;
  }

  .back {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-bottom: var(--space-3);
    color: var(--muted);
    font-weight: 700;
    text-decoration: none;
  }

  .head {
    display: grid;
    grid-template-columns: 48px 1fr auto;
    align-items: center;
    gap: var(--space-2) var(--space-3);
    margin-bottom: var(--space-3);
  }

  .ic {
    width: 48px;
    height: 48px;
    border-radius: 15px;
    background: var(--surface-2);
    display: grid;
    place-items: center;
  }

  .cond {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .late {
    color: var(--over);
    font-weight: 700;
  }

  .status {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    margin-bottom: var(--space-3);
  }

  .timer {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 2px;
  }

  .clock {
    font-family: var(--font-display);
    font-size: 3rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: var(--primary);
    line-height: 1.1;
  }

  .two {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 1fr;
    gap: var(--space-2);
  }

  .two:empty {
    display: none;
  }

  .two button,
  .go {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }

  .go {
    min-height: 56px;
    font-size: 1.1rem;
  }

  .center {
    text-align: center;
  }

  .between {
    justify-content: space-between;
  }

  .sub {
    font-size: 1.1rem;
  }

  .steps {
    list-style: none;
    margin: var(--space-2) 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .steps li {
    position: relative;
  }

  .steps button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    text-align: left;
    background: var(--surface-2);
    border: 1px solid transparent;
    color: var(--fg);
  }

  .steps button:disabled {
    opacity: 1;
  }

  .box {
    flex: none;
    width: 26px;
    height: 26px;
    border-radius: 8px;
    border: 2px solid var(--line);
    background: var(--surface);
    display: grid;
    place-items: center;
    color: #fff;
    transition: background 0.2s, border-color 0.2s, transform 0.2s;
  }

  .label {
    flex: 1;
  }

  li.done .box {
    background: var(--fresh);
    border-color: var(--fresh);
  }

  li.done .label {
    text-decoration: line-through;
    color: var(--muted);
  }

  li.locked .box {
    color: var(--muted);
  }

  li.locked .label {
    color: var(--muted);
  }

  li.sparkle .box {
    animation: pop 0.45s ease;
  }

  li.sparkle button {
    animation: glow 0.9s ease;
  }

  .stars {
    position: absolute;
    left: 26px;
    top: 50%;
    pointer-events: none;
  }

  .stars i {
    position: absolute;
    left: -6px;
    top: -6px;
    color: var(--coin-bg);
    animation: star 0.8s ease-out forwards;
  }

  @keyframes pop {
    0% { transform: scale(0.6); }
    60% { transform: scale(1.25); }
    100% { transform: scale(1); }
  }

  @keyframes glow {
    0% { border-color: var(--fresh); background: color-mix(in srgb, var(--fresh) 16%, var(--surface-2)); }
    100% { border-color: transparent; background: var(--surface-2); }
  }

  @keyframes star {
    0% { transform: rotate(var(--a)) translateX(0) scale(0.4); opacity: 1; }
    100% { transform: rotate(var(--a)) translateX(30px) scale(1); opacity: 0; }
  }

  .done-panel {
    position: relative;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: var(--space-2);
    padding: var(--space-5) var(--space-4);
    background: color-mix(in srgb, var(--fresh) 9%, var(--surface));
    border-color: color-mix(in srgb, var(--fresh) 45%, var(--line));
  }

  .big-check {
    width: 72px;
    height: 72px;
    border-radius: 50%;
    background: var(--fresh);
    color: #fff;
    display: grid;
    place-items: center;
    animation: pop 0.5s ease;
  }

  .xp-burst {
    background: var(--xp);
    color: #fff;
    font-weight: 800;
    border-radius: 999px;
    padding: 4px 14px;
    animation: pop 0.6s ease 0.15s both;
  }

  .btnlike {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: var(--tap);
    padding: 0 var(--space-4);
    border-radius: var(--radius-sm);
    text-decoration: none;
    font-weight: 700;
    margin-top: var(--space-2);
  }

  .btnlike.primary {
    background: var(--primary);
    color: var(--primary-fg);
  }

  .confetti {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  .confetti i {
    position: absolute;
    left: 50%;
    top: 62px;
    width: var(--w);
    height: var(--h);
    border-radius: 2px;
    opacity: 0;
    animation: burst 1.6s cubic-bezier(0.15, 0.7, 0.3, 1) forwards;
    animation-delay: var(--d);
  }

  .confetti i.round {
    border-radius: 50%;
  }

  @keyframes burst {
    0% { transform: translate(0, 0) rotate(0deg) scale(0.3); opacity: 0; }
    10% { opacity: 1; }
    45% { transform: translate(var(--x), var(--y)) rotate(var(--r)) scale(1); opacity: 1; }
    100% { transform: translate(calc(var(--x) * 1.15), calc(var(--y) + 170px)) rotate(calc(var(--r) * 3)); opacity: 0; }
  }

  .link {
    display: block;
    margin: var(--space-4) auto 0;
    background: transparent;
    border: none;
    color: var(--muted);
    text-decoration: underline;
    min-height: 36px;
  }

  .err {
    color: var(--over);
  }

  @media (prefers-reduced-motion: reduce) {
    .confetti,
    .stars {
      display: none;
    }
  }
</style>
