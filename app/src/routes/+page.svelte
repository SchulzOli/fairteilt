<script lang="ts">
  import TaskCard from "$lib/TaskCard.svelte";
  import TaskSheet, { PRESETS, type TaskPreset } from "$lib/TaskSheet.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Krumel from "$lib/ui/Krumel.svelte";
  import { familyGoal } from "$lib/demo";
  import { session } from "$lib/session.svelte";
  import { conditionOf, taskStore } from "$lib/tasks.svelte";
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
  const running = $derived(rest.filter((t) => t.status === "in_progress" && t.assignee_id === me));
  const mine = $derived(rest.filter((t) => t.status !== "in_progress" && t.assignee_id === me));
  const open = $derived(rest.filter((t) => !t.assignee_id));
  const others = $derived(rest.filter((t) => t.assignee_id && t.assignee_id !== me));
  const streak = $derived(taskStore.myAccount?.streak ?? { days: 0, today: false });

  const familyPct = $derived(Math.min(100, (familyGoal.xp / familyGoal.goal) * 100));
  const krumelPct = $derived(Math.min(100, (familyGoal.krumelXp / familyGoal.goal) * 100));
  const ahead = $derived(familyGoal.xp >= familyGoal.krumelXp);

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
  <span class="streak" class:cold={!streak.today}
    title={streak.today ? "Serie: Tage mit mindestens einer erledigten Aufgabe" : "Heute noch eine Aufgabe, dann geht die Serie weiter"}>
    <Icon name="flame" size={16} />{streak.days} {streak.days === 1 ? "Tag" : "Tage"}
  </span>
</header>

<section class="card race" aria-label="Familienziel">
  <div class="row between">
    <strong>Familie gegen Krümel</strong>
    <span class="coin-chip">{familyGoal.reward}</span>
  </div>
  <div class="track" aria-hidden="true">
    <span class="lane"></span>
    <span class="fam" style="width: {familyPct}%"></span>
    <span class="runner krumel" style="left: {krumelPct}%"><Krumel size={24} /></span>
    <span class="runner us" style="left: {familyPct}%">wir</span>
    <span class="goal"><Icon name="flag" size={20} /></span>
  </div>
  <div class="row between small muted">
    <span>{familyGoal.xp} / {familyGoal.goal} XP</span>
    <span>{ahead ? "Ihr liegt vorn" : "Krümel knapp vorn"} · noch {familyGoal.daysLeft} Tage</span>
  </div>
</section>

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

  {#if running.length}
    <h2 class="section-title">Läuft gerade</h2>
    <div class="stack">
      {#each running as task (task.id)}<TaskCard {task} />{/each}
    </div>
  {/if}

  <h2 class="section-title">Für dich</h2>
  <div class="stack">
    {#each mine as task (task.id)}
      <TaskCard {task} />
    {:else}
      <p class="muted">{running.length ? "Sonst nichts. Super!" : "Alles erledigt. Danke dir!"}</p>
    {/each}
  </div>

  {#if open.length}
    <h2 class="section-title">Offen · wer hilft?</h2>
    <div class="stack">
      {#each open as task (task.id)}<TaskCard {task} />{/each}
    </div>
  {/if}

  {#if others.length}
    <h2 class="section-title">Bei den anderen</h2>
    <div class="stack">
      {#each others as task (task.id)}<TaskCard {task} />{/each}
    </div>
  {/if}
{/if}

<p class="muted small demo">Das Familienziel ist noch ein Beispiel. Das echte Rennen gegen Krümel kommt mit M4.</p>

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

  .streak.cold {
    background: var(--surface-2);
    color: var(--muted);
  }

  .race {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .track {
    position: relative;
    height: 34px;
    margin: 2px 14px 0 4px;
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
    right: 0;
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
    right: -14px;
    top: 6px;
    color: var(--coin);
  }

  .demo {
    margin-top: var(--space-5);
    text-align: center;
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
