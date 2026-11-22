<script lang="ts">
  // 4-digit PIN entry for the family tablet. Calls `oncomplete` when 4 digits are in.
  let { oncomplete, error = null, busy = false }: { oncomplete: (pin: string) => void; error?: string | null; busy?: boolean } = $props();

  let pin = $state("");

  function press(d: string) {
    if (busy || pin.length >= 4) return;
    pin += d;
    if (pin.length === 4) {
      const p = pin;
      setTimeout(() => {
        pin = "";
        oncomplete(p);
      }, 120);
    }
  }
</script>

<div class="pinpad">
  <div class="dots" aria-label="{pin.length} von 4 Ziffern">
    {#each [0, 1, 2, 3] as i (i)}<i class:on={i < pin.length}></i>{/each}
  </div>
  {#if error}<p class="err" role="alert">{error}</p>{/if}
  <div class="keys">
    {#each ["1", "2", "3", "4", "5", "6", "7", "8", "9"] as d (d)}
      <button type="button" onclick={() => press(d)} disabled={busy}>{d}</button>
    {/each}
    <span></span>
    <button type="button" onclick={() => press("0")} disabled={busy}>0</button>
    <button type="button" aria-label="Löschen" onclick={() => (pin = pin.slice(0, -1))} disabled={busy}>⌫</button>
  </div>
</div>

<style>
  .pinpad {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    align-items: center;
  }

  .dots {
    display: flex;
    gap: 14px;
  }

  .dots i {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid var(--fg);
  }

  .dots i.on {
    background: var(--fg);
  }

  .keys {
    display: grid;
    grid-template-columns: repeat(3, 72px);
    gap: var(--space-2);
  }

  .keys button {
    height: 60px;
    font-family: var(--font-display);
    font-size: 1.4rem;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 16px;
  }

  .err {
    color: var(--over);
    margin: 0;
    font-weight: 700;
  }
</style>
