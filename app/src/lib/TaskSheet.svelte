<script lang="ts" module>
  import type { IconName } from "$lib/ui/Icon.svelte";

  export interface TaskPreset {
    title: string;
    room: string;
    icon: IconName;
    days: number;
    xp: number;
    steps?: string[];
    sequential?: boolean;
    duration?: number;
  }

  /** Quick starts for an empty household (KONZEPT §7.1). */
  export const PRESETS: TaskPreset[] = [
    { title: "Bad putzen", room: "Bad", icon: "bath", days: 7, xp: 20, steps: ["Spiegel", "Waschbecken", "Toilette", "Dusche", "Boden"], duration: 30 },
    { title: "Spülmaschine", room: "Küche", icon: "dish", days: 1, xp: 6, steps: ["Einräumen", "Ausräumen"], sequential: true, duration: 10 },
    { title: "Müll rausbringen", room: "Flur", icon: "bin", days: 3, xp: 5, duration: 5 },
    { title: "Staubsaugen", room: "Wohnzimmer", icon: "vacuum", days: 4, xp: 12, duration: 25 },
    { title: "Wäsche", room: "Wäsche", icon: "shirt", days: 3, xp: 15, steps: ["Waschen", "Aufhängen", "Zusammenlegen"], sequential: true, duration: 20 },
    { title: "Blumen gießen", room: "Wohnzimmer", icon: "plant", days: 3, xp: 4, duration: 5 },
  ];
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import type { Assignment, CreateTemplateRequest, Schedule, TemplateApproval, TemplateDto } from "$lib/generated";
  import { shopping } from "$lib/shopping.svelte";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { TASK_ICONS, iconOf, taskStore } from "$lib/tasks.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import type { AnimalName } from "$lib/ui/Avatar.svelte";
  import Icon from "$lib/ui/Icon.svelte";

  let {
    onclose,
    preset,
    roomId,
    template,
  }: { onclose: () => void; preset?: TaskPreset; roomId?: string; /** Edit this setup instead of creating one. */ template?: TemplateDto } = $props();

  type Rhythm = "once" | "interval" | "weekly" | "on_call";
  type Who = "open" | "fixed" | "rotation" | "least_loaded";
  const WEEKDAYS = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];

  // The form starts from the preset once; later prop changes do not reset what was typed.
  const init = untrack(() => ({ preset, roomId, t: template }));
  const t0 = init.t;
  const initialRoom = () =>
    t0 ? t0.room_id : (init.roomId ?? (init.preset ? taskStore.rooms.find((r) => r.name === init.preset!.room)?.id : undefined) ?? null);
  const s0 = t0?.schedule;
  const a0 = t0?.assignment;

  let title = $state(t0?.title ?? init.preset?.title ?? "");
  let room = $state<string | null>(initialRoom());
  let icon = $state<IconName | null>((t0?.icon as IconName | null) ?? init.preset?.icon ?? null);
  let rhythm = $state<Rhythm>(s0 ? (s0.type as Rhythm) : "interval");
  let days = $state(s0?.type === "interval" ? s0.days : (init.preset?.days ?? 7));
  let weekdays = $state<number[]>(s0?.type === "weekly" ? [...s0.weekdays] : []);
  /** "Auf Abruf": due this many days after it was called (0 = no due date). */
  let dueAfter = $state(s0?.type === "on_call" ? (s0.due_after_days ?? 0) : 2);
  let xp = $state(t0?.points.default ?? init.preset?.xp ?? 10);
  let who = $state<Who>(a0 ? (a0.type as Who) : "open");
  let people = $state<string[]>(a0?.type === "fixed" ? [a0.member] : a0?.type === "rotation" || a0?.type === "least_loaded" ? [...a0.members] : []);
  let steps = $state<string[]>([...(t0?.steps ?? init.preset?.steps ?? [])]);
  let sequential = $state(t0 ? t0.step_order === "sequential" : (init.preset?.sequential ?? false));
  // No timer since 23.11.2026: the duration is kept as it was set, the form does not ask for it.
  const duration = t0 ? t0.duration_min : null;
  let approval = $state<TemplateApproval>(t0?.approval ?? "household");
  let confirmDelete = $state(false);
  /** The task of a shopping list: its rhythm stays "auf Abruf". */
  const isList = $derived(!!t0 && shopping.lists.some((l) => l.template_id === t0.id));
  let newStep = $state("");
  let busy = $state(false);
  let error = $state("");

  const roomIcon = $derived(iconOf(taskStore.rooms.find((r) => r.id === room)?.icon));
  const members = $derived(session.members);

  function togglePerson(id: string) {
    if (who === "fixed") people = [id];
    else people = people.includes(id) ? people.filter((p) => p !== id) : [...people, id];
  }

  function addStep() {
    const s = newStep.trim();
    if (s && steps.length < 20) steps = [...steps, s];
    newStep = "";
  }

  function schedule(): Schedule {
    if (rhythm === "once") return { type: "once" };
    if (rhythm === "weekly") return { type: "weekly", weekdays: [...weekdays].sort(), every_weeks: 1 };
    if (rhythm === "on_call") return { type: "on_call", due_after_days: dueAfter > 0 ? dueAfter : null };
    return { type: "interval", days };
  }

  function assignment(): Assignment {
    if (who === "fixed") return { type: "fixed", member: people[0] };
    if (who === "rotation") return { type: "rotation", members: people, next: 0 };
    if (who === "least_loaded") return { type: "least_loaded", members: people };
    return { type: "open" };
  }

  async function remove() {
    if (!t0) return;
    if (!confirmDelete) return void (confirmDelete = true);
    busy = true;
    error = "";
    try {
      await session.request("DELETE", `/templates/${t0.id}`);
      taskStore.scheduleReload();
      feedback("step");
      onclose();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    error = "";
    if (rhythm === "weekly" && weekdays.length === 0) return (error = "Bitte mindestens einen Wochentag wählen.");
    if (who !== "open" && people.length === 0) return (error = "Bitte wähle, wer die Aufgabe macht.");
    busy = true;
    try {
      const body: CreateTemplateRequest = {
        title: title.trim(),
        room_id: room,
        icon,
        kind: null,
        points: { min: Math.max(1, Math.round(xp / 2)), default: xp, max: Math.min(100, Math.round(xp * 1.5)) },
        schedule: schedule(),
        assignment: assignment(),
        steps,
        step_order: sequential ? "sequential" : "free",
        duration_min: duration,
        due_at: null,
        approval,
      };
      if (t0) {
        await session.request("PATCH", `/templates/${t0.id}`, body);
        taskStore.scheduleReload();
      } else {
        await taskStore.createTemplate(body);
      }
      feedback("confirm");
      onclose();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="sheet-title">
    <div class="row between">
      <h2 id="sheet-title">{t0 ? "Aufgabe bearbeiten" : "Neue Aufgabe"}</h2>
      <button class="icon-btn" aria-label="Schließen" onclick={onclose}><Icon name="close" size={20} /></button>
    </div>

    <form class="stack" onsubmit={save}>
      <label class="field"><span>Was ist zu tun?</span>
        <input id="task-title" type="text" bind:value={title} maxlength="80" required placeholder="z. B. Bad putzen" />
      </label>

      <div class="field"><span>Raum</span>
        <div class="chips">
          {#each taskStore.rooms as r (r.id)}
            <button type="button" class="chip" class:sel={room === r.id} onclick={() => (room = room === r.id ? null : r.id)}>
              <Icon name={iconOf(r.icon)} size={16} />{r.name}
            </button>
          {:else}
            <span class="muted small">Noch keine Räume. Du kannst sie unter Plan · Räume anlegen.</span>
          {/each}
        </div>
      </div>

      <div class="field"><span>Symbol</span>
        <div class="chips icons">
          {#each TASK_ICONS as i (i)}
            <button type="button" class="chip ic" class:sel={(icon ?? roomIcon) === i} aria-label="Symbol {i}" onclick={() => (icon = i)}>
              <Icon name={i} size={18} />
            </button>
          {/each}
        </div>
      </div>

      <div class="field"><span>Wie oft?</span>
        {#if isList}<p class="muted small">Einkaufen bleibt „auf Abruf“: Die Liste ruft die Aufgabe, sobald etwas draufsteht.</p>{/if}
        <div class="seg" role="radiogroup" aria-label="Rhythmus" class:locked={isList}>
          <button type="button" role="radio" aria-checked={rhythm === "interval"} class:sel={rhythm === "interval"} onclick={() => (rhythm = "interval")}>Alle X Tage</button>
          <button type="button" role="radio" aria-checked={rhythm === "weekly"} class:sel={rhythm === "weekly"} onclick={() => (rhythm = "weekly")}>Wochentage</button>
          <button type="button" role="radio" aria-checked={rhythm === "on_call"} class:sel={rhythm === "on_call"} onclick={() => (rhythm = "on_call")}>Auf Abruf</button>
          <button type="button" role="radio" aria-checked={rhythm === "once"} class:sel={rhythm === "once"} onclick={() => (rhythm = "once")}>Einmal</button>
        </div>
        {#if rhythm === "interval"}
          <div class="stepper">
            <span>alle</span>
            <button type="button" aria-label="Weniger Tage" onclick={() => (days = Math.max(1, days - 1))}>−</button>
            <b>{days}</b>
            <button type="button" aria-label="Mehr Tage" onclick={() => (days = Math.min(365, days + 1))}>+</button>
            <span>{days === 1 ? "Tag" : "Tage"} nach dem letzten Mal</span>
          </div>
        {:else if rhythm === "on_call"}
          <div class="stepper">
            <span>fällig</span>
            <button type="button" aria-label="Früher fällig" onclick={() => (dueAfter = Math.max(0, dueAfter - 1))}>−</button>
            <b>{dueAfter === 0 ? "–" : dueAfter}</b>
            <button type="button" aria-label="Später fällig" onclick={() => (dueAfter = Math.min(30, dueAfter + 1))}>+</button>
            <span>{dueAfter === 0 ? "ohne Termin" : dueAfter === 1 ? "Tag nach dem Abruf" : "Tage nach dem Abruf"}</span>
          </div>
          <p class="muted small">Ruht, bis jemand unter Plan · Räume „Ist wieder nötig“ tippt. Einkaufen kommt von selbst, sobald etwas auf der Liste steht.</p>
        {:else if rhythm === "weekly"}
          <div class="chips">
            {#each WEEKDAYS as d, i (d)}
              <button type="button" class="chip day" class:sel={weekdays.includes(i + 1)}
                onclick={() => (weekdays = weekdays.includes(i + 1) ? weekdays.filter((w) => w !== i + 1) : [...weekdays, i + 1])}>{d}</button>
            {/each}
          </div>
        {/if}
      </div>

      <div class="field"><span>XP</span>
        <div class="stepper">
          <button type="button" aria-label="Weniger XP" onclick={() => (xp = Math.max(1, xp - 1))}>−</button>
          <b>{xp}</b>
          <button type="button" aria-label="Mehr XP" onclick={() => (xp = Math.min(66, xp + 1))}>+</button>
          <span class="muted">steigt bis {Math.min(100, Math.round(xp * 1.5))}, wenn sie liegen bleibt</span>
        </div>
      </div>

      <div class="field"><span>Wer?</span>
        <div class="seg" role="radiogroup" aria-label="Zuweisung">
          <button type="button" role="radio" aria-checked={who === "open"} class:sel={who === "open"} onclick={() => ((who = "open"), (people = []))}>Offen</button>
          <button type="button" role="radio" aria-checked={who === "fixed"} class:sel={who === "fixed"} onclick={() => ((who = "fixed"), (people = people.slice(0, 1)))}>Fest</button>
          <button type="button" role="radio" aria-checked={who === "rotation"} class:sel={who === "rotation"} onclick={() => (who = "rotation")}>Reihum</button>
          <button type="button" role="radio" aria-checked={who === "least_loaded"} class:sel={who === "least_loaded"} onclick={() => (who = "least_loaded")}>Wenigste XP</button>
        </div>
        {#if who !== "open"}
          <div class="chips">
            {#each members as m (m.membership_id)}
              <button type="button" class="chip person" class:sel={people.includes(m.membership_id)} onclick={() => togglePerson(m.membership_id)}>
                <Avatar animal={m.animal as AnimalName} color={m.color} size={22} />{m.display_name}
                {#if who === "rotation" && people.includes(m.membership_id)}<small>{people.indexOf(m.membership_id) + 1}.</small>{/if}
              </button>
            {/each}
          </div>
        {/if}
      </div>

      <div class="field"><span>Schritte <small class="muted">(optional, XP gibt es am Ende)</small></span>
        {#if steps.length}
          <ol class="steps">
            {#each steps as s, i (i)}
              <li><span>{s}</span><button type="button" class="icon-btn" aria-label="Schritt {s} entfernen" onclick={() => (steps = steps.filter((_, j) => j !== i))}><Icon name="close" size={16} /></button></li>
            {/each}
          </ol>
        {/if}
        <div class="row add">
          <input id="new-step" type="text" bind:value={newStep} maxlength="80" placeholder="Schritt hinzufügen"
            onkeydown={(e) => e.key === "Enter" && (e.preventDefault(), addStep())} />
          <button type="button" aria-label="Schritt hinzufügen" onclick={addStep}><Icon name="plus" size={18} /></button>
        </div>
        {#if steps.length > 1}
          <div class="seg" role="radiogroup" aria-label="Reihenfolge">
            <button type="button" role="radio" aria-checked={!sequential} class:sel={!sequential} onclick={() => (sequential = false)}>Beliebige Reihenfolge</button>
            <button type="button" role="radio" aria-checked={sequential} class:sel={sequential} onclick={() => (sequential = true)}>Der Reihe nach</button>
          </div>
        {/if}
      </div>

      <div class="field"><span>Bestätigung</span>
        <div class="seg" role="radiogroup" aria-label="Bestätigung">
          <button type="button" role="radio" aria-checked={approval === "household"} class:sel={approval === "household"} onclick={() => (approval = "household")}>Wie im Haushalt</button>
          <button type="button" role="radio" aria-checked={approval === "always"} class:sel={approval === "always"} onclick={() => (approval = "always")}>Immer</button>
          <button type="button" role="radio" aria-checked={approval === "never"} class:sel={approval === "never"} onclick={() => (approval = "never")}>Nie</button>
        </div>
        <small class="muted">{approval === "household" ? "Aufgaben von Kindern bestätigt ein Erwachsener." : approval === "always" ? "Jemand anderes bestätigt immer." : "Zählt sofort, ohne Bestätigung."}</small>
      </div>

      {#if t0}
        <p class="muted small">Name, Symbol und Raum ändern sich sofort. XP, Schritte, Rhythmus und Zuweisung gelten für die offene Runde, solange noch niemand dran ist, sonst ab der nächsten.</p>
      {/if}
      {#if error}<p class="err" role="alert">{error}</p>{/if}
      <button class="primary" type="submit" disabled={busy || !title.trim()}>{t0 ? "Speichern" : "Aufgabe anlegen"}</button>
      {#if t0 && !isList}
        <button type="button" class="danger" class:confirm={confirmDelete} disabled={busy} onclick={remove}>
          <Icon name="trash" size={18} />{confirmDelete ? "Wirklich löschen? Die offene Runde fällt weg." : "Aufgabe löschen"}
        </button>
      {/if}
    </form>
  </div>
</div>

<style>
  h2 {
    font-size: 1.3rem;
  }

  .between {
    justify-content: space-between;
    margin-bottom: var(--space-3);
  }

  .icon-btn {
    min-height: 36px;
    width: 36px;
    padding: 0;
    display: grid;
    place-items: center;
    background: transparent;
    border: none;
    color: var(--muted);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    font-weight: 400;
  }

  .chip {
    min-height: 36px;
    padding: 0 12px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--fg);
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 0.9rem;
  }

  .chip.ic {
    width: 40px;
    padding: 0;
    justify-content: center;
  }

  .chip.day {
    width: 44px;
    padding: 0;
    justify-content: center;
  }

  .chip.person {
    padding-left: 5px;
  }

  .chip.sel {
    background: var(--primary-soft);
    border-color: var(--primary);
    color: var(--primary);
    font-weight: 700;
  }

  .stepper {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
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
    min-width: 2ch;
    text-align: center;
    font-size: 1.2rem;
  }

  .steps {
    margin: 0;
    padding-left: 1.4em;
    font-weight: 400;
  }

  .steps li span {
    flex: 1;
  }

  .steps li {
    display: list-item;
  }

  .steps li > * {
    vertical-align: middle;
  }

  .steps .icon-btn {
    display: inline-grid;
    min-height: 28px;
    width: 28px;
  }

  .add input {
    flex: 1;
  }

  .add button {
    width: var(--tap);
    padding: 0;
    display: grid;
    place-items: center;
  }

  .err {
    color: var(--over);
  }

  .locked {
    pointer-events: none;
    opacity: 0.55;
  }

  .danger {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    background: transparent;
    color: var(--over);
  }

  .danger.confirm {
    background: var(--over-soft);
  }
</style>
