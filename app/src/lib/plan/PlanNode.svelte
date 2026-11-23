<script lang="ts" module>
  export type NodeState = "open" | "running" | "submitted" | "done" | "late" | "projected";
</script>

<script lang="ts">
  // One entry in the plan views (calendar, board, lists), in the look of the task cards: a
  // symbol tile tinted in the colour of whoever is on it, the title, a short state line, the
  // animal and the XP. Draggable when `drag` is given and enabled.
  import type { MemberDto } from "$lib/generated";
  import Avatar from "$lib/ui/Avatar.svelte";
  import type { AnimalName } from "$lib/ui/Avatar.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import type { IconName } from "$lib/ui/Icon.svelte";
  import { draggable, type DragOptions } from "./drag";

  let {
    title,
    icon,
    member = undefined,
    xp = null,
    meta = "",
    state = "open",
    size = "m",
    drag = undefined,
    onopen = undefined,
    label = undefined,
  }: {
    title: string;
    icon: IconName;
    member?: MemberDto;
    xp?: number | null;
    /** Short line under the title: time, "überfällig", … */
    meta?: string;
    state?: NodeState;
    /** s: one line (week columns, trays); m: with state line. */
    size?: "s" | "m";
    drag?: DragOptions;
    onopen?: () => void;
    label?: string;
  } = $props();

  const color = $derived(member?.color ?? "var(--muted)");
  const movable = $derived(!!drag?.enabled);
  const noDrag: DragOptions = { enabled: false, ondrop: () => {} };
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex (role and tabindex only with onopen) -->
<div
  class="node {size} {state}"
  class:movable
  class:clickable={!!onopen}
  style="--c: {color}"
  role={onopen ? "button" : undefined}
  tabindex={onopen ? 0 : undefined}
  aria-label={label ?? title}
  onclick={onopen}
  onkeydown={(ev) => onopen && (ev.key === "Enter" || ev.key === " ") && (ev.preventDefault(), onopen())}
  use:draggable={drag ?? noDrag}
>
  <span class="tile" aria-hidden="true"><Icon name={state === "done" ? "check" : icon} size={size === "s" ? 14 : 16} /></span>
  <span class="text">
    <span class="title">{title}</span>
    {#if size === "m" && meta}<span class="meta">{meta}</span>{/if}
  </span>
  {#if size === "s" && meta}<span class="meta inline">{meta}</span>{/if}
  {#if xp !== null && size === "m"}<span class="xp-chip">{xp}</span>{/if}
  {#if member}
    <span class="who"><Avatar animal={member.animal as AnimalName} color={member.color} size={size === "s" ? 18 : 22} name={member.display_name} /></span>
  {:else}
    <span class="who open" title="Noch offen" aria-label="noch offen"></span>
  {/if}
</div>

<style>
  .node {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    min-height: 48px;
    padding: 6px 8px 6px 6px;
    border-radius: 14px;
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--fg);
    text-align: left;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
    transition: box-shadow 0.15s, border-color 0.15s, opacity 0.15s;
  }

  .node.s {
    min-height: 36px;
    gap: 6px;
    padding: 4px 6px 4px 4px;
    border-radius: 11px;
  }

  .clickable {
    cursor: pointer;
  }

  .movable {
    cursor: grab;
    touch-action: manipulation;
  }

  .clickable:hover {
    border-color: color-mix(in srgb, var(--c) 45%, var(--line));
  }

  .tile {
    flex: none;
    width: 32px;
    height: 32px;
    border-radius: 10px;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--c) 16%, var(--surface));
    color: color-mix(in srgb, var(--c) 80%, var(--fg));
  }

  .s .tile {
    width: 26px;
    height: 26px;
    border-radius: 8px;
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 700;
    font-size: 0.9rem;
    line-height: 1.2;
  }

  .s .title {
    font-size: 0.84rem;
  }

  .meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.76rem;
    color: var(--muted);
  }

  .meta.inline {
    flex: none;
    font-variant-numeric: tabular-nums;
  }

  .xp-chip {
    flex: none;
    padding: 1px 6px;
    font-size: 0.74rem;
  }

  .who {
    flex: none;
    display: grid;
  }

  .who.open {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 2px dashed var(--line);
  }

  .s .who.open {
    width: 16px;
    height: 16px;
  }

  /* ---- states, as on the task cards ---- */
  .running {
    border-color: color-mix(in srgb, var(--primary) 45%, var(--line));
  }

  .running .meta {
    color: var(--primary);
    font-weight: 700;
  }

  .submitted .meta {
    color: var(--coin);
    font-weight: 700;
  }

  .late {
    border-color: color-mix(in srgb, var(--over) 45%, var(--line));
  }

  .late .tile {
    background: var(--over-soft);
    color: var(--over);
  }

  .late .meta {
    color: var(--over);
    font-weight: 700;
  }

  .done {
    background: color-mix(in srgb, var(--fresh) 6%, var(--surface));
  }

  .done .tile {
    background: var(--ok-soft);
    color: var(--ok);
  }

  .done .title {
    text-decoration: line-through;
    color: var(--muted);
  }

  .projected {
    background: transparent;
    border-style: dashed;
    border-color: color-mix(in srgb, var(--c) 40%, var(--line));
  }

  .projected .tile {
    background: transparent;
    border: 1.5px dashed color-mix(in srgb, var(--c) 55%, var(--line));
  }

  .projected .title {
    font-weight: 600;
    color: var(--muted);
  }

  .projected .who {
    opacity: 0.55;
  }

  /* ---- drag and drop (see drag.ts) ---- */
  :global(.node.drag-source) {
    opacity: 0.35;
  }

  :global(.node.drag-ghost) {
    opacity: 0.95;
    border-color: var(--primary);
    box-shadow: 0 14px 30px rgb(0 0 0 / 0.25);
  }
</style>
