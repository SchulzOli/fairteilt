<script lang="ts">
  // Einkaufsliste (M8, KONZEPT §10): everyone writes down what is missing; that earns no XP.
  // The first item calls the list's task "Einkaufen", which is done in the shopping mode.
  import { goto } from "$app/navigation";
  import type { Section } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { SECTIONS, bySection, itemKey, shopping } from "$lib/shopping.svelte";
  import { memberName, taskStore } from "$lib/tasks.svelte";
  import ItemRow from "$lib/shop/ItemRow.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import type { AnimalName } from "$lib/ui/Avatar.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import PlanSheet from "$lib/plan/PlanSheet.svelte";
  import type { TaskDto } from "$lib/generated";

  let planning = $state<TaskDto | null>(null);
  const whenFmt = new Intl.DateTimeFormat("de-DE", { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  let name = $state("");
  let note = $state("");
  let section = $state<Section | null>(null);
  let urgent = $state(false);
  let more = $state(false);
  let error = $state("");
  let manage = $state(false);
  let newList = $state("");
  let rename = $state("");

  const list = $derived(shopping.current);
  const planner = $derived(session.membership?.role !== "child");
  const open = $derived(list ? list.items.filter((i) => !i.checked_at) : []);
  const cart = $derived(list ? list.items.filter((i) => i.checked_at) : []);
  const round = $derived(list?.task_id ? taskStore.tasks.find((t) => t.id === list.task_id) : undefined);
  const shopper = $derived(round?.assignee_id ? session.members.find((m) => m.membership_id === round.assignee_id) : undefined);
  const me = $derived(session.membership?.membership_id);

  // Suggestions: what the family buys often and what is not on the list yet; while typing,
  // the ones that match.
  const suggestions = $derived.by(() => {
    const onList = new Set(open.map((i) => itemKey(i.name)));
    const typed = itemKey(name);
    return (shopping.data?.suggestions ?? [])
      .filter((s) => !onList.has(itemKey(s.name)) && (!typed || itemKey(s.name).startsWith(typed)))
      .slice(0, 8);
  });

  async function add(e?: SubmitEvent, text?: string, sec?: Section) {
    e?.preventDefault();
    error = "";
    const what = (text ?? name).trim();
    if (!list || !what) return;
    const opts = { note: text ? undefined : note || undefined, section: sec ?? section ?? undefined, urgent: text ? false : urgent };
    // Cleared right away, so the next item can be typed while this one is sent.
    if (!text) {
      name = "";
      note = "";
      urgent = false;
      section = null;
      more = false;
    }
    try {
      await shopping.add(list.id, what, opts);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function run(fn: () => Promise<unknown>) {
    error = "";
    try {
      await fn();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }
</script>

<header class="row between">
  <h1 class="page-title">{list?.name ?? "Einkaufen"}</h1>
  {#if shopping.lists.length > 1 || planner}
    <button class="ghost" aria-expanded={manage} onclick={() => ((manage = !manage), (rename = list?.name ?? ""))}>
      <Icon name="cart" size={18} />Listen
    </button>
  {/if}
</header>

{#if shopping.lists.length > 1}
  <div class="lists" role="tablist" aria-label="Listen">
    {#each shopping.lists as l (l.id)}
      <button role="tab" aria-selected={l.id === list?.id} class:sel={l.id === list?.id} onclick={() => shopping.select(l.id)}>
        {l.name}
        {#if l.items.some((i) => !i.checked_at)}<span class="count">{l.items.filter((i) => !i.checked_at).length}</span>{/if}
      </button>
    {/each}
  </div>
{/if}

{#if manage}
  <section class="card stack manage">
    {#if planner}
      <form class="row" onsubmit={(e) => (e.preventDefault(), run(async () => (await shopping.createList(newList.trim()), (newList = ""))))}>
        <input type="text" bind:value={newList} maxlength="40" placeholder="Neue Liste, z. B. Drogerie" aria-label="Name der neuen Liste" required />
        <button class="primary" type="submit">Anlegen</button>
      </form>
      <p class="muted small">Jede Liste hat ihre eigene Aufgabe „… einkaufen“, die kommt, sobald etwas draufsteht.</p>
      {#if list}
        <form class="row" onsubmit={(e) => (e.preventDefault(), run(() => shopping.renameList(list.id, rename.trim())))}>
          <input type="text" bind:value={rename} maxlength="40" aria-label="Name der Liste" required />
          <button type="submit">Umbenennen</button>
        </form>
        {#if shopping.lists.length > 1}
          <button class="del" onclick={() => run(() => shopping.deleteList(list.id))}>„{list.name}“ samt Aufgabe löschen</button>
        {/if}
      {/if}
    {:else}
      <p class="muted small">Listen legen Erwachsene an.</p>
    {/if}
  </section>
{/if}

{#if list}
  <!-- Who shops: the list's task ("Einkaufen") -->
  {#if round}
    <button class="duty" onclick={() => goto(`/aufgabe/${round.id}`)}>
      {#if shopper}
        <Avatar animal={shopper.animal as AnimalName} color={shopper.color} size={30} name={shopper.display_name} />
      {:else}
        <span class="duty-ic"><Icon name="cart" size={18} /></span>
      {/if}
      <span class="duty-text">
        <b>
          {#if round.status === "in_progress"}{shopper?.membership_id === me ? "Du kaufst gerade ein" : `${shopper?.display_name ?? "Jemand"} kauft gerade ein`}
          {:else if round.status === "submitted"}Einkauf wartet aufs OK
          {:else if shopper}{shopper.membership_id === me ? "Du bist dran" : `${shopper.display_name} ist dran`}
          {:else}Wer geht einkaufen?{/if}
        </b>
        <span class="muted small">„{round.title}“ · {round.xp} XP fürs Einkaufen</span>
      </span>
      <Icon name="chevron" size={18} />
    </button>
    {#if round.status !== "submitted" && (planner || round.assignee_id === me)}
      <button class="when" class:set={!!round.scheduled_start} onclick={() => (planning = round)}>
        <Icon name="plan" size={16} />
        {#if round.scheduled_start}Einkaufen am <b>{whenFmt.format(new Date(round.scheduled_start))}</b> · ändern{:else}Einkaufstag wählen{/if}
      </button>
    {/if}
  {:else if open.length === 0}
    <p class="duty calm muted">Nichts zu kaufen. Sobald etwas draufsteht, kommt „Einkaufen“ als Aufgabe.</p>
  {/if}

  <form class="add" onsubmit={(e) => add(e)}>
    <div class="row">
      <input type="text" bind:value={name} maxlength="60" placeholder="Was fehlt?" aria-label="Artikel" autocomplete="off" />
      <button class="primary plus" type="submit" aria-label="Aufschreiben" disabled={!name.trim()}><Icon name="plus" size={20} /></button>
    </div>
    {#if name.trim()}
      <div class="row opts">
        <button type="button" class="chip" class:sel={urgent} onclick={() => (urgent = !urgent)}>Dringend</button>
        <button type="button" class="chip" class:sel={more} onclick={() => (more = !more)}>Notiz & Abteilung</button>
      </div>
      {#if more}
        <input type="text" bind:value={note} maxlength="80" placeholder="Menge oder Notiz, z. B. 2 Liter" aria-label="Notiz" />
        <div class="chips" role="group" aria-label="Abteilung">
          {#each SECTIONS as s (s.id)}
            <button type="button" class="chip" class:sel={section === s.id} onclick={() => (section = section === s.id ? null : (s.id as Section))}>{s.label}</button>
          {/each}
        </div>
      {/if}
    {/if}
    {#if suggestions.length}
      <div class="chips sugg" aria-label="Vorschläge">
        {#each suggestions as s (s.name)}
          <button type="button" class="chip" onclick={() => add(undefined, s.name, s.section)}>+ {s.name}</button>
        {/each}
      </div>
    {/if}
  </form>
  {#if error}<p class="err" role="alert">{error}</p>{/if}

  {#each bySection(open) as g (g.section)}
    <h2 class="section-title">{g.label}</h2>
    <ul class="items">
      {#each g.items as item (item.id)}<ItemRow {item} />{/each}
    </ul>
  {/each}

  {#if cart.length}
    <h2 class="section-title">Im Wagen{shopper ? ` bei ${memberName(round?.assignee_id ?? null)}` : ""}</h2>
    <ul class="items">
      {#each cart as item (item.id)}<ItemRow {item} />{/each}
    </ul>
  {/if}

  <p class="muted small foot">Aufschreiben gibt keine XP. XP gibt es fürs Einkaufen. Abhaken, wer nebenbei etwas mitgebracht hat: Der Artikel verschwindet von der Liste.</p>
{:else if shopping.loaded}
  <p class="muted">Keine Liste gefunden.</p>
{:else}
  <p class="muted">Liste lädt …</p>
{/if}

{#if planning}
  <PlanSheet task={planning} onclose={() => (planning = null)} />
{/if}

<style>
  .between {
    justify-content: space-between;
  }

  .ghost {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    color: var(--primary);
  }

  .lists {
    display: flex;
    gap: var(--space-2);
    overflow-x: auto;
    margin: var(--space-3) 0;
  }

  .lists button {
    flex: none;
    border-radius: 999px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .lists button.sel {
    background: var(--primary);
    color: var(--primary-fg);
  }

  .count {
    min-width: 20px;
    padding: 0 6px;
    border-radius: 999px;
    background: color-mix(in srgb, currentColor 18%, transparent);
    font-size: 0.78rem;
  }

  .manage {
    margin: var(--space-3) 0;
  }

  .del {
    color: var(--over);
  }

  .duty {
    width: 100%;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: var(--space-3);
    margin: var(--space-3) 0;
    padding: var(--space-3);
    border-radius: var(--radius);
    background: var(--primary-soft);
    text-align: left;
    font-weight: 400;
  }

  .duty:has(+ .when) {
    margin-bottom: var(--space-2);
  }

  .when {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 40px;
    margin-bottom: var(--space-3);
    padding: 0 var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    color: var(--muted);
    font-weight: 400;
    font-size: 0.9rem;
    text-align: left;
  }

  .when.set {
    color: var(--primary);
  }

  .when b {
    font-weight: 800;
  }

  .duty.calm {
    display: block;
    background: var(--surface-2);
  }

  .duty-ic {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--surface);
    color: var(--primary);
  }

  .duty-text {
    display: flex;
    flex-direction: column;
  }

  .add {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }

  .add input {
    border: 2px solid var(--primary);
  }

  .plus {
    width: var(--tap);
    padding: 0;
    display: grid;
    place-items: center;
    flex: none;
  }

  .opts,
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    min-height: 34px;
    padding: 0 12px;
    border-radius: 999px;
    font-size: 0.85rem;
  }

  .chip.sel {
    background: var(--primary);
    color: var(--primary-fg);
  }

  .sugg .chip {
    background: var(--surface-2);
    font-weight: 700;
  }

  .items {
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .err {
    color: var(--over);
  }

  .small {
    font-size: 0.85rem;
  }

  .foot {
    margin-top: var(--space-5);
  }
</style>
