<script lang="ts">
  // Condition of a task or room: fresh → due → over (KONZEPT §7.1). `value` is 0…1+
  // (share of the interval that has passed); ≥ 1 means overdue.
  let { value, label }: { value: number; label?: string } = $props();

  const state = $derived(value >= 1 ? "over" : value >= 0.75 ? "due" : "fresh");
  const width = $derived(Math.max(4, Math.min(100, Math.round(value * 100))));
  const text = $derived(label ?? (state === "over" ? "überfällig" : state === "due" ? "bald fällig" : "frisch"));
</script>

<span class="bar {state}" role="meter" aria-valuemin="0" aria-valuemax="100" aria-valuenow={width} aria-label="Zustand: {text}">
  <i style="width: {width}%"></i>
</span>

<style>
  .bar {
    display: block;
    height: 6px;
    border-radius: 999px;
    background: var(--surface-2);
    overflow: hidden;
  }

  i {
    display: block;
    height: 100%;
    border-radius: inherit;
    transition: width 0.4s ease;
  }

  .fresh i {
    background: var(--fresh);
  }

  .due i {
    background: var(--due);
  }

  .over i {
    background: var(--over);
  }
</style>
