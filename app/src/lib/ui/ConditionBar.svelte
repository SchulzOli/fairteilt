<script lang="ts">
  // How much time a task has left (KONZEPT §7.1): four steps Gut → Hat Zeit → Fällig →
  // Überfällig. `value` is the share of the round that has passed (0…1, ≥ 1 = overdue). The
  // bar fills up and its colour runs smoothly from green to red; the tag names the step.
  import { STATUS_LABEL, statusOf } from "$lib/tasks.svelte";

  let {
    value,
    tag = true,
    none = false,
  }: {
    value: number;
    /** Show the status word next to the bar. */
    tag?: boolean;
    /** No due date: grey, "Kein Termin". */
    none?: boolean;
  } = $props();

  const status = $derived(statusOf(value));
  const v = $derived(Math.max(0, Math.min(1, value)));
  const width = $derived(none ? 0 : Math.max(5, Math.round(v * 100)));
  // oklch hue 150 (green) → 25 (red); a little darker on the way, so yellow stays readable.
  const color = $derived(none ? "var(--muted)" : `oklch(${0.68 - 0.06 * v} 0.17 ${150 - 125 * v})`);
  const text = $derived(none ? "Kein Termin" : STATUS_LABEL[status]);
</script>

<span class="cond {none ? 'none' : status}" style="--c: {color}">
  <span class="track" role="meter" aria-valuemin="0" aria-valuemax="100" aria-valuenow={width} aria-label="Zustand: {text}">
    <i style="width: {width}%"></i>
  </span>
  {#if tag}<span class="tag"><b aria-hidden="true"></b>{text}</span>{/if}
</span>

<style>
  .cond {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .track {
    flex: 1;
    min-width: 40px;
    height: 8px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--c) 14%, var(--surface-2));
    overflow: hidden;
  }

  i {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--c);
    transition:
      width 0.5s ease,
      background-color 0.5s ease;
  }

  /* Overdue: the full bar gets a slow stripe, so it reads even without colour. */
  .over i {
    background: repeating-linear-gradient(-45deg, var(--c) 0 8px, color-mix(in srgb, var(--c) 78%, #fff) 8px 14px);
    background-size: 20px 20px;
    animation: crawl 1.6s linear infinite;
  }

  .tag {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 1px 8px 1px 6px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--c) 14%, var(--surface));
    color: color-mix(in srgb, var(--c) 75%, var(--fg));
    font-size: 0.74rem;
    font-weight: 800;
    white-space: nowrap;
  }

  .tag b {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--c);
  }

  .none .tag {
    background: var(--surface-2);
    color: var(--muted);
  }

  @keyframes crawl {
    to {
      background-position: 20px 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .over i {
      animation: none;
    }
  }
</style>
