<script lang="ts">
  // One item on a shopping list. "list": writing things down (who added it, urgent, section);
  // "cart": the shopping mode (big check, nothing else in the way).
  import type { AnimalName } from "$lib/ui/Avatar.svelte";
  import type { Section, ShoppingItemDto } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { SECTIONS, shopping } from "$lib/shopping.svelte";
  import { feedback } from "$lib/settings.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import Icon from "$lib/ui/Icon.svelte";

  let { item, mode = "list", fresh = false }: { item: ShoppingItemDto; mode?: "list" | "cart"; fresh?: boolean } = $props();

  let menu = $state(false);
  let error = $state("");

  const author = $derived(session.members.find((m) => m.membership_id === item.added_by));
  const inCart = $derived(item.checked_at !== null);
  const me = $derived(session.membership?.membership_id);
  const mayChange = $derived(session.membership?.role !== "child" || item.added_by === me);

  async function run(fn: () => Promise<unknown>) {
    error = "";
    try {
      await fn();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  function toggle() {
    if (!inCart) feedback("step");
    void run(() => shopping.check(item, !inCart));
  }
</script>

<li class="item" class:cart={mode === "cart"} class:done={inCart} class:fresh>
  <button class="check" class:on={inCart} aria-label="{inCart ? 'Zurück auf die Liste' : 'Abhaken'}: {item.name}" onclick={toggle}>
    {#if inCart}<Icon name="check" size={mode === "cart" ? 18 : 16} />{/if}
  </button>
  <button class="body" onclick={() => (mode === "list" ? (menu = !menu) : toggle())} aria-expanded={mode === "list" ? menu : undefined}>
    <b>{item.name}</b>
    {#if item.note}<span class="note">{item.note}</span>{/if}
  </button>
  <span class="side">
    {#if fresh}<span class="tag new">neu</span>{/if}
    {#if item.urgent}<span class="tag urgent">dringend</span>{/if}
    {#if mode === "list" && author}
      <Avatar animal={author.animal as AnimalName} color={author.color} size={24} name={author.display_name} />
    {/if}
  </span>
  {#if menu && mode === "list"}
    <div class="menu">
      <div class="sections" role="group" aria-label="Abteilung">
        {#each SECTIONS as s (s.id)}
          <button class="chip" class:sel={item.section === s.id} disabled={!mayChange}
            onclick={() => run(() => shopping.setSection(item, s.id as Section))}>{s.label}</button>
        {/each}
      </div>
      <div class="row acts">
        <button disabled={!mayChange} onclick={() => run(() => shopping.setUrgent(item, !item.urgent))}>
          {item.urgent ? "Nicht mehr dringend" : "Dringend"}
        </button>
        <button class="del" disabled={!mayChange} onclick={() => run(() => shopping.remove(item))}><Icon name="trash" size={16} />Streichen</button>
      </div>
      {#if author}<p class="muted small">Aufgeschrieben von {author.display_name}.</p>{/if}
    </div>
  {/if}
  {#if error}<p class="err" role="alert">{error}</p>{/if}
</li>

<style>
  .item {
    list-style: none;
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: var(--space-2) var(--space-3);
    padding: var(--space-2) var(--space-3);
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
  }

  .item.fresh {
    border: 2px solid var(--coin-bg);
    background: color-mix(in srgb, var(--coin-bg) 10%, var(--surface));
  }

  .check {
    width: 30px;
    min-height: 30px;
    height: 30px;
    padding: 0;
    border-radius: 50%;
    border: 2px solid var(--line);
    background: transparent;
    color: #fff;
    display: grid;
    place-items: center;
  }

  .cart .check {
    width: 38px;
    height: 38px;
  }

  .check.on {
    background: var(--fresh);
    border-color: var(--fresh);
  }

  .body {
    min-height: var(--tap);
    padding: 0;
    background: transparent;
    text-align: left;
    display: flex;
    flex-direction: column;
    justify-content: center;
    font-weight: 400;
  }

  .cart .body b {
    font-size: 1.08rem;
  }

  .done .body b {
    text-decoration: line-through;
  }

  .done {
    opacity: 0.65;
  }

  .note {
    font-size: 0.85rem;
    color: var(--muted);
  }

  .side {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .tag {
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 0.75rem;
    font-weight: 800;
    white-space: nowrap;
  }

  .tag.urgent {
    background: var(--over-soft);
    color: var(--over);
  }

  .tag.new {
    background: var(--coin-soft);
    color: var(--coin);
  }

  .menu {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding-top: var(--space-2);
    border-top: 1px solid var(--line);
  }

  .sections {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    min-height: 34px;
    padding: 0 12px;
    border-radius: 999px;
    font-size: 0.85rem;
    font-weight: 700;
  }

  .chip.sel {
    background: var(--primary);
    color: var(--primary-fg);
  }

  .acts button {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }

  .del {
    color: var(--over);
  }

  .err {
    grid-column: 1 / -1;
    color: var(--over);
    font-size: 0.85rem;
  }

  .small {
    font-size: 0.85rem;
  }
</style>
