<script lang="ts">
  import TaskCard from "$lib/TaskCard.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Krumel from "$lib/ui/Krumel.svelte";
  import { familyGoal, me, myStreakDays, tasks } from "$lib/demo";

  let done = $state<string[]>([]);

  const mine = $derived(tasks.filter((t) => t.assignee === me.name && !done.includes(t.id)));
  const open = $derived(tasks.filter((t) => !t.assignee && !done.includes(t.id)));

  const familyPct = $derived(Math.min(100, (familyGoal.xp / familyGoal.goal) * 100));
  const krumelPct = $derived(Math.min(100, (familyGoal.krumelXp / familyGoal.goal) * 100));
  const ahead = $derived(familyGoal.xp >= familyGoal.krumelXp);

  const today = new Intl.DateTimeFormat("de-DE", { weekday: "long", day: "numeric", month: "long" }).format(new Date());
</script>

<header class="head">
  <div class="row">
    <Avatar animal={me.animal} color={me.color} size={44} name={me.name} />
    <div>
      <div class="muted small">{today}</div>
      <h1 class="page-title">Hallo {me.name}</h1>
    </div>
  </div>
  <span class="streak" title="Serie: Tage mit mindestens einer erledigten Aufgabe">
    <Icon name="flame" size={16} />{myStreakDays} Tage
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

<h2 class="section-title">Heute für dich</h2>
<div class="stack">
  {#each mine as task (task.id)}
    <TaskCard {task} ondone={(id) => (done = [...done, id])} />
  {:else}
    <p class="muted">Alles erledigt. Danke dir!</p>
  {/each}
</div>

<h2 class="section-title">Offen · wer hilft?</h2>
<div class="stack">
  {#each open as task (task.id)}
    <TaskCard {task} ondone={(id) => (done = [...done, id])} />
  {/each}
</div>

<p class="muted small demo">Beispieldaten. Die echte Anbindung folgt mit M1–M3.</p>

<button class="fab primary" aria-label="Aufgabe anlegen"><Icon name="plus" size={24} /></button>

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

  .fab {
    position: fixed;
    right: var(--space-4);
    bottom: calc(var(--nav-h) + env(safe-area-inset-bottom) + var(--space-4));
    width: 56px;
    height: 56px;
    border-radius: 18px;
    padding: 0;
    display: grid;
    place-items: center;
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.22);
  }
</style>
