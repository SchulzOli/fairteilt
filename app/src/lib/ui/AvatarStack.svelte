<script lang="ts">
  // Everyone on a task, like co-authors on GitHub: the first animal in front, each next one
  // half behind the previous. More than `max`: "+N". The names are in the tooltip and label.
  import type { MemberDto } from "$lib/generated";
  import Avatar from "./Avatar.svelte";
  import type { AnimalName } from "./Avatar.svelte";

  let {
    members,
    size = 22,
    max = 3,
    ring = "var(--surface)",
  }: {
    members: MemberDto[];
    size?: number;
    max?: number;
    /** Colour of the gap ring around each animal (the background it sits on). */
    ring?: string;
  } = $props();

  const shown = $derived(members.slice(0, max));
  const rest = $derived(members.length - shown.length);
  const names = $derived(members.map((m) => m.display_name).join(", "));
</script>

{#if members.length}
  <span class="avstack" style="--s: {size}px; --ring: {ring}" title={names} role="img" aria-label={names}>
    {#each shown as m, i (m.membership_id)}
      <span class="av" style="z-index: {shown.length - i}">
        <Avatar animal={m.animal as AnimalName} color={m.color} {size} />
      </span>
    {/each}
    {#if rest > 0}<span class="more">+{rest}</span>{/if}
  </span>
{/if}

<style>
  .avstack {
    display: inline-flex;
    align-items: center;
    flex: none;
  }

  .av {
    position: relative;
    display: grid;
    border-radius: 50%;
    box-shadow: 0 0 0 2px var(--ring);
  }

  /* Each next animal half behind the previous one. */
  .av + .av {
    margin-left: calc(var(--s) * -0.45);
  }

  .more {
    position: relative;
    z-index: 0;
    margin-left: calc(var(--s) * -0.3);
    min-width: var(--s);
    height: var(--s);
    padding: 0 4px 0 calc(var(--s) * 0.3);
    border-radius: 999px;
    display: grid;
    place-items: center;
    background: var(--surface-2);
    box-shadow: 0 0 0 2px var(--ring);
    font-size: calc(var(--s) * 0.45);
    font-weight: 800;
    color: var(--muted);
  }
</style>
