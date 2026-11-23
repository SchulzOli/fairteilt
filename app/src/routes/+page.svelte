<script lang="ts">
  import TaskCard from "$lib/TaskCard.svelte";
  import TaskSheet, { PRESETS, type TaskPreset } from "$lib/TaskSheet.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Krumel from "$lib/ui/Krumel.svelte";
  import GoalSheet from "$lib/GoalSheet.svelte";
  import { session } from "$lib/session.svelte";
  import { conditionOf, isOn, taskStore } from "$lib/tasks.svelte";
  import type { AnimalName } from "$lib/ui/Avatar.svelte";

  let sheet = $state<{ preset?: TaskPreset } | null>(null);

  const meName = $derived(session.me?.user.display_name ?? "");
  const me = $derived(session.membership?.membership_id);
  const planner = $derived(session.membership?.role !== "child");
  const byUrgency = $derived(
    [...taskStore.tasks].sort((a, b) => conditionOf(b, taskStore.now) - conditionOf(a, taskStore.now)),
  );
  const toReview = $derived(planner ? byUrgency.filter((t) => t.status === "submitted" && t.completed_by !== me) : []);
  const rest = $derived(byUrgency.filter((t) => !toReview.includes(t)));
  // Mine: I have it or joined it. Others': shown faded at the end, "+" still lets me join.
  const mine = $derived(rest.filter((t) => isOn(t, me)));
  const open = $derived(rest.filter((t) => !t.assignee_id));
  const others = $derived(rest.filter((t) => t.assignee_id && !isOn(t, me)));
  const streak = $derived(taskStore.myAccount?.streak ?? { days: 0, today: false });

  let goalSheet = $state(false);
  const goal = $derived(taskStore.period?.goal);
  const familyPct = $derived(goal ? Math.min(100, (goal.family_xp / goal.target) * 100) : 0);
  const krumelPct = $derived(goal ? Math.min(100, (goal.krumel_xp / goal.target) * 100) : 0);
  const ahead = $derived(goal ? goal.family_xp >= goal.krumel_xp : true);
  const daysLeft = $derived(
    taskStore.period ? Math.max(0, Math.ceil((Date.parse(taskStore.period.ends_at) - taskStore.now) / 86_400_000)) : 0,
  );
  const raceText = $derived.by(() => {
    if (!goal) return "";
    if (goal.status === "won") return `Geschafft! ${goal.reward} gehört euch`;
    if (goal.status === "lost") return "Krümel war schneller. Nächstes Mal!";
    const behind = goal.krumel_xp - goal.family_xp;
    return `${ahead ? "Ihr liegt vorn" : behind < 10 ? "Krümel knapp vorn" : "Krümel liegt vorn"} · noch ${daysLeft} ${daysLeft === 1 ? "Tag" : "Tage"}`;
  });

  const today = new Intl.DateTimeFormat("de-DE", { weekday: "long", day: "numeric", month: "long" }).format(new Date());
</script>

<header class="head">
  <div class="row">
    <Avatar animal={(session.membership?.animal ?? "fox") as AnimalName} color={session.membership?.color ?? "#2563eb"} size={44} name={meName} />
    <div>
      <div class="muted small">{today}</div>
      <h1 class="page-title">Hallo {meName}</h1>
    </div>
  </div>
  <a class="bell" href="/postfach" aria-label="Postfach{taskStore.unread ? `, ${taskStore.unread} neu` : ''}">
    <Icon name="bell" size={20} />
    {#if taskStore.unread}<span class="badge">{taskStore.unread > 9 ? "9+" : taskStore.unread}</span>{/if}
  </a>
  <span class="streak" class:cold={!streak.today}
    title={streak.today ? "Serie: Tage mit mindestens einer erledigten Aufgabe" : "Heute noch eine Aufgabe, dann geht die Serie weiter"}>
    <Icon name="flame" size={16} />{streak.days} {streak.days === 1 ? "Tag" : "Tage"}
  </span>
</header>

{#if goal}
<section class="card race" class:won={goal.status === "won"} aria-label="Familienziel">
  <div class="row between">
    <strong>Familie gegen Krümel</strong>
    {#if session.isAdmin}
      <button class="reward" onclick={() => (goalSheet = true)} aria-label="Familienziel ändern: {goal.reward}"><span class="goal-chip"><Icon name="flag" size={14} />{goal.reward}</span></button>
    {:else}
      <span class="goal-chip"><Icon name="flag" size={14} />{goal.reward}</span>
    {/if}
  </div>
  <div class="track" aria-hidden="true">
    <span class="lane"></span>
    <span class="fam" style="width: {familyPct}%"></span>
    <span class="runner krumel" style="left: {krumelPct}%"><Krumel size={24} /></span>
    <span class="runner us" style="left: {familyPct}%">wir</span>
    <span class="goal"><Icon name="flag" size={16} /></span>
  </div>
  <div class="row between small muted">
    <span>{goal.family_xp} / {goal.target} XP</span>
    <span class:result={goal.status !== "running"}>{raceText}</span>
  </div>
</section>
{/if}

{#if !taskStore.loaded}
  <p class="muted">Aufgaben laden …</p>
{:else if taskStore.tasks.length === 0}
  <section class="card stack empty">
    <h2 class="sub">Noch keine Aufgaben</h2>
    {#if planner}
      <p class="muted">Leg die erste an oder fang mit einem Vorschlag an. Jede Aufgabe bekommt einen Zustandsbalken, der sich mit der Zeit füllt.</p>
      <div class="presets">
        {#each PRESETS as p (p.title)}
          <button onclick={() => (sheet = { preset: p })}><Icon name={p.icon} size={18} />{p.title}</button>
        {/each}
      </div>
    {:else}
      <p class="muted">Sobald deine Eltern Aufgaben anlegen, tauchen sie hier auf.</p>
    {/if}
  </section>
{:else}
  {#if toReview.length}
    <h2 class="section-title">Zu bestätigen</h2>
    <div class="stack">
      {#each toReview as task (task.id)}<TaskCard {task} />{/each}
    </div>
  {/if}

  <h2 class="section-title">Für dich</h2>
  <div class="stack">
    {#each mine as task (task.id)}
      <TaskCard {task} />
    {:else}
      <p class="muted">Alles erledigt. Danke dir!</p>
    {/each}
  </div>

  {#if open.length}
    <h2 class="section-title">Offen · wer hilft?</h2>
    <div class="stack">
      {#each open as task (task.id)}<TaskCard {task} />{/each}
    </div>
  {/if}

  {#if others.length}
    <h2 class="section-title">Bei den anderen · mitmachen mit +</h2>
    <div class="stack">
      {#each others as task (task.id)}<TaskCard {task} />{/each}
    </div>
  {/if}
{/if}

{#if goalSheet}
  <GoalSheet onclose={() => (goalSheet = false)} />
{/if}

{#if planner}
  <button class="fab primary" aria-label="Aufgabe anlegen" onclick={() => (sheet = {})}><Icon name="plus" size={24} /></button>
{/if}

{#if sheet}
  <TaskSheet preset={sheet.preset} onclose={() => (sheet = null)} />
{/if}

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-bottom: var(--space-4);
  }

  .small {
    font-size: 0.85rem;
  }

  .between {
    justify-content: space-between;
  }

  .streak {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--over-soft);
    color: var(--over);
    font-weight: 700;
    font-size: 0.85rem;
    white-space: nowrap;
  }

  .bell {
    position: relative;
    margin-left: auto;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--fg);
    background: var(--surface);
    border: 1px solid var(--line);
  }

  .badge {
    position: absolute;
    top: -4px;
    right: -4px;
    min-width: 18px;
    height: 18px;
    padding: 0 4px;
    border-radius: 9px;
    background: var(--over);
    color: #fff;
    font-size: 0.7rem;
    font-weight: 800;
    display: grid;
    place-items: center;
  }

  .streak.cold {
    background: var(--surface-2);
    color: var(--muted);
  }

  .race {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  /* The runners move on the track; the finish post sits right of it, so "wir" arrives next
     to the flag instead of covering it. */
  .track {
    position: relative;
    height: 34px;
    margin: 2px 48px 0 4px;
  }

  .lane,
  .fam {
    position: absolute;
    left: 0;
    top: 14px;
    height: 7px;
    border-radius: 7px;
  }

  .lane {
    right: -32px;
    background: var(--surface-2);
  }

  .fam {
    background: var(--xp);
    transition: width 0.6s ease;
  }

  .runner {
    position: absolute;
    top: 1px;
    transform: translateX(-50%);
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    border: 2px solid var(--surface);
  }

  .runner.krumel {
    background: color-mix(in srgb, var(--krumel) 22%, var(--surface));
  }

  .runner.us {
    background: var(--xp);
    color: #fff;
    font-weight: 800;
    font-size: 0.72rem;
  }

  .goal {
    position: absolute;
    right: -48px;
    top: 1px;
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--coin-soft);
    color: var(--coin);
    border: 2px solid var(--surface);
    box-shadow: 0 0 0 1.5px color-mix(in srgb, var(--coin-bg) 70%, transparent);
  }

  .race.won .goal {
    background: var(--coin-bg);
    color: #5b3a05;
    box-shadow: none;
  }

  .reward {
    padding: 0;
    min-height: 0;
    background: transparent;
    border: none;
  }

  .goal-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 10px;
    border-radius: 999px;
    background: var(--primary-soft);
    color: var(--primary);
    font-weight: 700;
    font-size: 0.82rem;
    white-space: nowrap;
  }

  .race.won {
    border-color: color-mix(in srgb, var(--coin) 50%, var(--line));
    background: color-mix(in srgb, var(--coin-soft) 45%, var(--surface));
  }

  .result {
    color: var(--fg);
    font-weight: 700;
  }

  .sub {
    font-size: 1.15rem;
  }

  .empty {
    margin-top: var(--space-4);
  }

  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .presets button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border-radius: 999px;
    min-height: 40px;
  }
</style>
