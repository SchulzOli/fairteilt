<script lang="ts" module>
  // Person colours (KONZEPT §7.5), separate from system colours.
  export const COLORS = ["#2563eb", "#d63384", "#e8660f", "#0b7f6f", "#4338ca", "#8a5a2b"];
</script>

<script lang="ts">
  import Avatar, { ANIMALS, type AnimalName } from "./Avatar.svelte";

  let { animal = $bindable("fox"), color = $bindable("#2563eb"), name = "" }: { animal?: string; color?: string; name?: string } =
    $props();

  const names = Object.keys(ANIMALS) as AnimalName[];
</script>

<div class="picker">
  <div class="preview"><Avatar animal={animal as AnimalName} {color} size={72} name={name || undefined} /></div>
  <div class="grid" role="radiogroup" aria-label="Tier">
    {#each names as a (a)}
      <button type="button" role="radio" aria-checked={animal === a} class:sel={animal === a} onclick={() => (animal = a)}>
        <Avatar animal={a} {color} size={36} />
        <span>{ANIMALS[a].label}</span>
      </button>
    {/each}
  </div>
  <div class="swatches" role="radiogroup" aria-label="Farbe">
    {#each COLORS as c (c)}
      <button type="button" role="radio" aria-checked={color === c} aria-label="Farbe {c}" class:sel={color === c} style="background: {c}" onclick={() => (color = c)}></button>
    {/each}
  </div>
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .preview {
    display: grid;
    place-items: center;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--space-2);
  }

  .grid button {
    min-height: 64px;
    padding: var(--space-2) 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 0.8rem;
  }

  .grid button.sel {
    border: 2px solid var(--primary);
  }

  .swatches {
    display: flex;
    justify-content: center;
    gap: var(--space-3);
  }

  .swatches button {
    width: 30px;
    height: 30px;
    min-height: 30px;
    padding: 0;
    border-radius: 50%;
  }

  .swatches button.sel {
    outline: 3px solid var(--fg);
    outline-offset: 2px;
  }
</style>
