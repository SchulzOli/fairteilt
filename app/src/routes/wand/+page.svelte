<script lang="ts">
  // Wall display on a tablet in landscape (KONZEPT §7.4): clock, family goal, one column per
  // person with today's tasks. Check off: tap the task → animal → PIN. The screen stays on;
  // at night (21–6 h) it dims and stays quiet.
  import { onMount } from "svelte";
  import type { MemberDto, TaskDto, TaskActionResponse } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { conditionOf, iconOf, stepsDone, taskStore } from "$lib/tasks.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import type { AnimalName } from "$lib/ui/Avatar.svelte";
  import ConditionBar from "$lib/ui/ConditionBar.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Krumel from "$lib/ui/Krumel.svelte";
  import PinPad from "$lib/ui/PinPad.svelte";
  import { sameDay } from "$lib/plan/views";
  import { itemKey, shopping } from "$lib/shopping.svelte";

  let flow = $state<{ task: TaskDto; who: MemberDto | null } | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let cheer = $state<{ id: string; text: string } | null>(null);
  let lock: WakeLockSentinel | null = null;
  /** Writing something on the shopping list: tap your animal (no PIN, it books nothing). */
  let writing = $state<{ who: MemberDto | null; text: string } | null>(null);
  let added = $state<string[]>([]);

  const list = $derived(shopping.current);
  const toBuy = $derived(list ? list.items.filter((i) => !i.checked_at) : []);
  const listTask = $derived(list?.task_id ? taskStore.tasks.find((t) => t.id === list.task_id) : undefined);
  const wallSuggestions = $derived.by(() => {
    const on = new Set(toBuy.map((i) => itemKey(i.name)));
    const typed = writing ? itemKey(writing.text) : "";
    return (shopping.data?.suggestions ?? []).filter((s) => !on.has(itemKey(s.name)) && (!typed || itemKey(s.name).startsWith(typed))).slice(0, 10);
  });

  async function write(text: string) {
    if (!writing?.who || !list || !text.trim()) return;
    error = null;
    try {
      await shopping.add(list.id, text, { asMember: writing.who.membership_id });
      added = [...added, text.trim()];
      writing.text = "";
      feedback("step");
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  const now = $derived(taskStore.now);
  const hour = $derived(new Date(now).getHours());
  const night = $derived(hour >= 21 || hour < 6);
  const goal = $derived(taskStore.period?.goal);

  /** Today's work: due today or earlier, getting due, planned for today, or running. */
  function today(t: TaskDto): boolean {
    if (t.status === "submitted") return true;
    if (t.status === "in_progress") return true;
    // One-off tasks without a date are always "now".
    if (!t.due_at && !t.scheduled_start) return true;
    if (t.scheduled_start && sameDay(Date.parse(t.scheduled_start), now)) return true;
    if (t.due_at && (Date.parse(t.due_at) < now || sameDay(Date.parse(t.due_at), now))) return true;
    return conditionOf(t, now) >= 0.75;
  }

  const columns = $derived.by(() => {
    const list = taskStore.tasks.filter(today).sort((a, b) => conditionOf(b, now) - conditionOf(a, now));
    const cols = session.members.map((m) => ({ key: m.membership_id, member: m as MemberDto | null, tasks: list.filter((t) => t.assignee_id === m.membership_id) }));
    cols.push({ key: "open", member: null, tasks: list.filter((t) => !t.assignee_id) });
    return cols.filter((c) => c.member || c.tasks.length);
  });

  const streakOf = (id: string) => taskStore.accounts.find((a) => a.membership_id === id)?.streak;
  const clock = $derived(new Intl.DateTimeFormat("de-DE", { hour: "2-digit", minute: "2-digit" }).format(new Date(now)));
  const date = $derived(new Intl.DateTimeFormat("de-DE", { weekday: "long", day: "numeric", month: "long" }).format(new Date(now)));

  async function keepAwake() {
    try {
      if (document.visibilityState === "visible" && "wakeLock" in navigator) lock = await navigator.wakeLock.request("screen");
    } catch {
      /* not allowed here: the screen may turn off */
    }
  }

  onMount(() => {
    void keepAwake();
    const again = () => void keepAwake();
    document.addEventListener("visibilitychange", again);
    return () => {
      document.removeEventListener("visibilitychange", again);
      void lock?.release();
    };
  });

  function start(task: TaskDto, column: MemberDto | null) {
    if (task.status === "submitted") return;
    error = null;
    flow = { task, who: column && column.has_pin ? column : null };
  }

  async function enter(pin: string) {
    if (!flow?.who) return;
    const { task, who } = flow;
    busy = true;
    error = null;
    try {
      const res = await session.asMember(who.membership_id, pin, async (call) => {
        const open = task.steps.filter((s) => s.done_at === null);
        let last: TaskActionResponse | null = null;
        if (open.length === 0) last = await call<TaskActionResponse>("POST", `/tasks/${task.id}/action`, { action: "done" });
        for (const s of open) last = await call<TaskActionResponse>("POST", `/tasks/${task.id}/steps/${s.id}/check`);
        return last!;
      });
      flow = null;
      if (!night) feedback("done");
      cheer = { id: task.id, text: res.submitted ? `Danke, ${who.display_name}! Wartet aufs OK` : `+${res.xp_awarded ?? task.xp} XP für ${who.display_name}` };
      setTimeout(() => (cheer = null), 2800);
      taskStore.scheduleReload();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="wall" class:night>
  <header class="top">
    <div class="time">
      <span class="clock">{clock}</span>
      <span class="date">{date}</span>
    </div>
    {#if goal}
      <div class="goal" class:won={goal.status === "won"}>
        <span class="buddy" aria-hidden="true"><Krumel size={28} /></span>
        <div class="race">
          <div class="row between small"><b>Familie gegen Krümel · {goal.reward}</b><span>{goal.family_xp} / {goal.target} XP</span></div>
          <div class="track">
            <i class="fam" style="width: {Math.min(100, (goal.family_xp / goal.target) * 100)}%"></i>
            <i class="krumel" style="left: {Math.min(100, (goal.krumel_xp / goal.target) * 100)}%"></i>
          </div>
        </div>
      </div>
    {/if}
    <a class="leave" href="/" aria-label="Wandanzeige beenden"><Icon name="close" size={20} /></a>
  </header>

  <div class="cols">
    {#each columns as c (c.key)}
      {@const s = c.member ? streakOf(c.member.membership_id) : undefined}
      <section class="col" style="--mc: {c.member?.color ?? 'var(--muted)'}">
        <h2>
          {#if c.member}<Avatar animal={c.member.animal as AnimalName} color={c.member.color} size={40} />{/if}
          <span>{c.member?.display_name ?? "Offen"}</span>
          {#if s && s.days > 0}<span class="streak" class:cold={!s.today}><Icon name="flame" size={14} />{s.days}</span>{/if}
        </h2>
        <div class="tasks">
          {#each c.tasks as t (t.id)}
            <button class="task" class:waiting={t.status === "submitted"} class:running={t.status === "in_progress"} onclick={() => start(t, c.member)}>
              <span class="ic"><Icon name={iconOf(t.icon)} size={22} /></span>
              <span class="tt">
                <b>{t.title}</b>
                <span class="small">
                  {t.status === "submitted" ? "wartet aufs OK" : t.status === "in_progress" ? "kauft ein" : `${t.xp} XP`}{#if t.steps.length} · {stepsDone(t)}/{t.steps.length}{/if}
                </span>
              </span>
              {#if cheer?.id === t.id}<span class="cheer">{cheer.text}</span>{/if}
              <span class="bar"><ConditionBar value={conditionOf(t, now)} /></span>
            </button>
          {:else}
            <p class="done-all">Alles erledigt</p>
          {/each}
        </div>
      </section>
    {/each}
    {#if list}
      <section class="col shop" style="--mc: var(--primary)">
        <h2><span class="shop-ic"><Icon name="cart" size={22} /></span><span>{list.name}</span></h2>
        <p class="small muted">
          {#if listTask?.assignee_id}{session.members.find((m) => m.membership_id === listTask.assignee_id)?.display_name ?? "Jemand"} kauft ein{:else if listTask}Wer geht einkaufen?{:else}Nichts zu kaufen{/if}
        </p>
        <ul class="shop-items">
          {#each toBuy.slice(0, 12) as i (i.id)}
            <li class:urgent={i.urgent}>{i.name}{#if i.note}<span class="small muted"> · {i.note}</span>{/if}</li>
          {/each}
          {#if toBuy.length > 12}<li class="small muted">und {toBuy.length - 12} weitere</li>{/if}
        </ul>
        <button class="primary write" onclick={() => ((writing = { who: null, text: "" }), (added = []))}><Icon name="plus" size={20} />Aufschreiben</button>
      </section>
    {/if}
  </div>
  {#if cheer && !columns.some((c) => c.tasks.some((t) => t.id === cheer?.id))}<p class="toast">{cheer.text}</p>{/if}
</div>

{#if writing}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (writing = null)}>
    <div class="sheet stack dialog" role="dialog" aria-modal="true" aria-labelledby="write-title">
      <h2 id="write-title">Was fehlt?</h2>
      {#if !writing.who}
        <p class="muted">Wer schreibt auf?</p>
        <div class="who">
          {#each session.members as m (m.membership_id)}
            <button class="animal" onclick={() => writing && (writing.who = m)}>
              <Avatar animal={m.animal as AnimalName} color={m.color} size={64} />
              <span>{m.display_name}</span>
            </button>
          {/each}
        </div>
        <p class="muted small">Aufschreiben geht ohne PIN, es gibt dafür keine XP.</p>
      {:else}
        <form class="row" onsubmit={(e) => (e.preventDefault(), void write(writing?.text ?? ""))}>
          <Avatar animal={writing.who.animal as AnimalName} color={writing.who.color} size={40} />
          <input type="text" bind:value={writing.text} maxlength="60" placeholder="z. B. Milch" aria-label="Artikel" />
          <button class="primary" type="submit" disabled={!writing.text.trim()}>Dazu</button>
        </form>
        {#if wallSuggestions.length}
          <div class="sugg">
            {#each wallSuggestions as s (s.name)}<button onclick={() => write(s.name)}>+ {s.name}</button>{/each}
          </div>
        {/if}
        {#if added.length}<p class="small">Aufgeschrieben: {added.join(", ")}</p>{/if}
        {#if error}<p class="err" role="alert">{error}</p>{/if}
      {/if}
      <button onclick={() => (writing = null)}>{added.length ? "Fertig" : "Abbrechen"}</button>
    </div>
  </div>
{/if}

{#if flow}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (flow = null)}>
    <div class="sheet stack dialog" role="dialog" aria-modal="true" aria-labelledby="wall-title">
      <h2 id="wall-title">„{flow.task.title}“ erledigt?</h2>
      {#if !flow.who}
        <p class="muted">Wer hat es gemacht?</p>
        <div class="who">
          {#each session.members as m (m.membership_id)}
            <button class="animal" disabled={!m.has_pin} onclick={() => flow && (flow.who = m)}>
              <Avatar animal={m.animal as AnimalName} color={m.color} size={64} />
              <span>{m.display_name}</span>
              {#if !m.has_pin}<small class="muted">ohne PIN</small>{/if}
            </button>
          {/each}
        </div>
        <p class="muted small">Wer keine PIN hat, hakt am eigenen Handy ab.</p>
      {:else}
        <div class="row center">
          <Avatar animal={flow.who.animal as AnimalName} color={flow.who.color} size={48} />
          <b>{flow.who.display_name}, deine PIN</b>
        </div>
        <PinPad oncomplete={enter} {error} {busy} />
        <button class="link" onclick={() => flow && (flow.who = null)}>Jemand anderes</button>
      {/if}
      <button onclick={() => (flow = null)}>Abbrechen</button>
    </div>
  </div>
{/if}

<style>
  .wall {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: calc(env(safe-area-inset-top) + var(--space-3)) var(--space-4) var(--space-4);
    background: var(--bg);
    overflow: hidden;
    transition: filter 1s;
  }

  .wall.night {
    filter: brightness(0.55);
  }

  .top {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }

  .time {
    display: flex;
    flex-direction: column;
  }

  .clock {
    font-family: var(--font-display);
    font-size: clamp(2.2rem, 6vw, 4rem);
    font-weight: 700;
    line-height: 1;
    font-variant-numeric: tabular-nums;
  }

  .date {
    color: var(--muted);
  }

  .goal {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius);
    background: var(--surface);
    border: 1px solid var(--line);
  }

  .goal.won {
    border-color: var(--coin);
    background: color-mix(in srgb, var(--coin-soft) 50%, var(--surface));
  }

  .buddy {
    flex: none;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--krumel) 18%, var(--surface));
  }

  .race {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .between {
    justify-content: space-between;
  }

  .track {
    position: relative;
    height: 10px;
    border-radius: 6px;
    background: var(--surface-2);
  }

  .track .fam {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 6px;
    background: var(--xp);
  }

  .track .krumel {
    position: absolute;
    top: -4px;
    width: 4px;
    height: 18px;
    border-radius: 2px;
    background: var(--krumel);
  }

  .leave {
    width: 44px;
    height: 44px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--muted);
    background: var(--surface);
    border: 1px solid var(--line);
  }

  .cols {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: minmax(220px, 1fr);
    gap: var(--space-3);
    overflow-x: auto;
  }

  .col {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-height: 0;
    padding: var(--space-3);
    border-radius: 22px;
    background: color-mix(in srgb, var(--mc) 7%, var(--surface));
    border: 1px solid color-mix(in srgb, var(--mc) 30%, var(--line));
  }

  .col h2 {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: 1.3rem;
  }

  .streak {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--over-soft);
    color: var(--over);
    font-size: 0.85rem;
  }

  .streak.cold {
    background: var(--surface-2);
    color: var(--muted);
  }

  .tasks {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    overflow-y: auto;
  }

  .task {
    position: relative;
    display: grid;
    grid-template-columns: 44px 1fr;
    gap: 6px var(--space-3);
    align-items: center;
    min-height: 72px;
    padding: var(--space-3);
    border-radius: 16px;
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--fg);
    text-align: left;
    font-weight: 400;
  }

  .task.running {
    border-color: var(--primary);
  }

  .task.waiting {
    opacity: 0.7;
  }

  .ic {
    width: 44px;
    height: 44px;
    border-radius: 14px;
    display: grid;
    place-items: center;
    background: var(--surface-2);
  }

  .tt {
    display: flex;
    flex-direction: column;
  }

  .tt b {
    font-size: 1.05rem;
  }

  .bar {
    grid-column: 1 / -1;
  }

  .cheer {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    border-radius: inherit;
    background: color-mix(in srgb, var(--fresh) 88%, transparent);
    color: #fff;
    font-weight: 800;
    animation: pop 0.4s ease;
  }

  @keyframes pop {
    from { transform: scale(0.9); opacity: 0; }
  }

  .shop-ic {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--primary-soft);
    color: var(--primary);
  }

  .shop-items {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    margin: 0;
    padding: 0 0 0 var(--space-4);
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-weight: 700;
  }

  .shop-items li.urgent::marker {
    color: var(--over);
  }

  .write {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }

  .err {
    color: var(--over);
  }

  .sugg {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .sugg button {
    min-height: 40px;
    border-radius: 999px;
  }

  .done-all {
    color: var(--muted);
    text-align: center;
    padding: var(--space-4) 0;
  }

  .toast {
    position: fixed;
    bottom: var(--space-4);
    left: 50%;
    transform: translateX(-50%);
    padding: var(--space-2) var(--space-4);
    border-radius: 999px;
    background: var(--xp);
    color: #fff;
    font-weight: 800;
  }

  .dialog {
    max-width: 520px;
    align-items: stretch;
  }

  .who {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: var(--space-3);
  }

  .animal {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: var(--space-2);
    min-width: 96px;
    background: var(--surface);
    border-radius: 18px;
  }

  .animal:disabled {
    opacity: 0.45;
  }

  .center {
    justify-content: center;
  }

  .link {
    background: transparent;
    border: none;
    color: var(--primary);
    font-weight: 700;
  }
</style>
