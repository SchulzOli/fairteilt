<script lang="ts">
  // Editor for a saved view (KONZEPT §7.3): filter (person, room, state, kind, bonus, period),
  // display (list, kanban, calendar, canvas), grouping and sorting; private or shared.
  import { untrack } from "svelte";
  import type { TaskKind, ViewDto, ViewFilter, ViewSort } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { feedback } from "$lib/settings.svelte";
  import { iconOf, taskStore } from "$lib/tasks.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import type { AnimalName } from "$lib/ui/Avatar.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import { STATUS_LABEL, defaultSort, emptyFilter } from "./views";

  let { view = null, onclose, onsaved }: { view?: ViewDto | null; onclose: () => void; onsaved: (v: ViewDto | null) => void } = $props();

  const init = untrack(() => view);
  let name = $state(init?.name ?? "");
  let shared = $state(init?.shared ?? false);
  let layout = $state(init?.layout ?? "list");
  let filter = $state<ViewFilter>(init ? structuredClone($state.snapshot(init.filter)) : emptyFilter());
  let sort = $state<ViewSort>(init ? { ...init.sort } : defaultSort());
  let busy = $state(false);
  let error = $state("");

  const me = $derived(session.membership?.membership_id);
  const mine = $derived(!init || init.owner_id === me);

  function toggle<T>(list: T[], v: T): T[] {
    return list.includes(v) ? list.filter((x) => x !== v) : [...list, v];
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = "";
    try {
      const hid = session.membership!.household_id;
      const body = { name: name.trim(), shared, layout, filter, sort };
      const v = init
        ? await session.request<ViewDto>("PUT", `/views/${init.id}`, body)
        : await session.request<ViewDto>("POST", `/households/${hid}/views`, body);
      feedback("confirm");
      onsaved(v);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function remove() {
    if (!init) return;
    busy = true;
    try {
      await session.request("DELETE", `/views/${init.id}`);
      onsaved(null);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  const KINDS: [TaskKind, string][] = [["regular", "Normal"], ["penalty", "Aufholaufgabe"]];
  const LAYOUTS = [["list", "Liste"], ["kanban", "Kanban"], ["calendar", "Kalender"], ["canvas", "Fläche"]];
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="view-title">
    <form class="stack" onsubmit={save}>
      <div class="row between">
        <h2 id="view-title">{init ? "Ansicht bearbeiten" : "Neue Ansicht"}</h2>
        <button type="button" class="icon-btn" aria-label="Schließen" onclick={onclose}><Icon name="close" size={20} /></button>
      </div>
      <fieldset class="stack" disabled={!mine}>
        <label class="field"><span>Name</span><input id="view-name" type="text" maxlength="40" required bind:value={name} placeholder="z. B. Diese Woche, Mias Aufgaben" /></label>

        <div class="field"><span>Darstellung</span>
          <div class="seg">{#each LAYOUTS as [k, l] (k)}<button type="button" class:sel={layout === k} onclick={() => (layout = k)}>{l}</button>{/each}</div>
        </div>

        <div class="field"><span>Wer?</span>
          <div class="chips">
            {#each session.members as m (m.membership_id)}
              <button type="button" class="chip person" class:sel={filter.members.includes(m.membership_id)} onclick={() => (filter.members = toggle(filter.members, m.membership_id))}>
                <Avatar animal={m.animal as AnimalName} color={m.color} size={20} />{m.display_name}
              </button>
            {/each}
            <button type="button" class="chip" class:sel={filter.open} onclick={() => (filter.open = !filter.open)}>Offen</button>
          </div>
        </div>

        {#if taskStore.rooms.length}
          <div class="field"><span>Räume</span>
            <div class="chips">
              {#each taskStore.rooms as r (r.id)}
                <button type="button" class="chip" class:sel={filter.rooms.includes(r.id)} onclick={() => (filter.rooms = toggle(filter.rooms, r.id))}><Icon name={iconOf(r.icon)} size={14} />{r.name}</button>
              {/each}
            </div>
          </div>
        {/if}

        <div class="field"><span>Stand</span>
          <div class="chips">
            {#each Object.entries(STATUS_LABEL) as [k, l] (k)}
              <button type="button" class="chip" class:sel={filter.statuses.includes(k)} onclick={() => (filter.statuses = toggle(filter.statuses, k))}>{l}</button>
            {/each}
          </div>
        </div>

        <div class="field"><span>Art</span>
          <div class="chips">
            {#each KINDS as [k, l] (k)}
              <button type="button" class="chip" class:sel={filter.kinds.includes(k)} onclick={() => (filter.kinds = toggle(filter.kinds, k))}>{l}</button>
            {/each}
            <button type="button" class="chip" class:sel={filter.bounty_only} onclick={() => (filter.bounty_only = !filter.bounty_only)}>Mit Bonus</button>
          </div>
        </div>

        <div class="field"><span>Fällig</span>
          <div class="seg">
            {#each [[null, "Egal"], [0, "Heute"], [7, "7 Tage"], [30, "30 Tage"]] as [d, l] (l)}
              <button type="button" class:sel={filter.due_within_days === d} onclick={() => (filter.due_within_days = d as number | null)}>{l}</button>
            {/each}
          </div>
        </div>

        <div class="row two">
          <label class="field"><span>Sortieren</span>
            <select bind:value={sort.by}>
              <option value="condition">dringend zuerst</option>
              <option value="due">nach Fälligkeit</option>
              <option value="xp">meiste XP</option>
              <option value="title">alphabetisch</option>
            </select>
          </label>
          <label class="field"><span>Gruppieren</span>
            <select bind:value={sort.group}>
              <option value="none">nicht</option>
              <option value="member">nach Person</option>
              <option value="room">nach Raum</option>
              <option value="status">nach Stand</option>
            </select>
          </label>
        </div>

        <label class="toggle"><span><b>Mit dem Haushalt teilen</b><br /><span class="muted small">Alle sehen sie, ändern kannst nur du.</span></span><input type="checkbox" bind:checked={shared} /></label>
      </fieldset>

      {#if error}<p class="err" role="alert">{error}</p>{/if}
      {#if mine}<button class="primary" type="submit" disabled={busy || !name.trim()}>{init ? "Speichern" : "Ansicht anlegen"}</button>{/if}
      {#if init && (mine || (init.shared && session.isAdmin))}<button type="button" class="danger" disabled={busy} onclick={remove}>Ansicht löschen</button>{/if}
    </form>
  </div>
</div>

<style>
  h2 {
    font-size: 1.3rem;
  }

  .between {
    justify-content: space-between;
  }

  fieldset {
    border: none;
    margin: 0;
    padding: 0;
    min-width: 0;
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

  .chip.person {
    padding-left: 5px;
  }

  .chip.sel {
    background: var(--primary-soft);
    border-color: var(--primary);
    color: var(--primary);
    font-weight: 700;
  }

  .two {
    align-items: start;
  }

  .two > * {
    flex: 1;
  }

  .toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .danger {
    color: var(--over);
    background: transparent;
  }

  .err {
    color: var(--over);
  }
</style>
