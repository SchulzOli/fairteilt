<script lang="ts">
  import { notices } from "$lib/notice.svelte";
  import Icon from "$lib/ui/Icon.svelte";

  const icons = { xp: "check", coin: "star", streak: "flame", info: "bell" } as const;
</script>

<div role="status" aria-live="polite">
{#if notices.current}
  {#key notices.current.id}
    <button class="notice {notices.current.tone}" title="Antippen zum Schließen" onclick={() => notices.dismiss()}>
      <span class="ic" aria-hidden="true">
        <Icon name={icons[notices.current.tone]} size={18} />
      </span>
      <span class="text">
        <b>{notices.current.text}</b>
        {#if notices.current.detail}<span>{notices.current.detail}</span>{/if}
      </span>
    </button>
  {/key}
{/if}
</div>

<style>
  .notice {
    position: fixed;
    left: 50%;
    top: calc(env(safe-area-inset-top) + var(--space-3));
    transform: translateX(-50%);
    z-index: 45;
    width: min(560px, calc(100vw - 2 * var(--space-4)));
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius);
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--fg);
    text-align: left;
    box-shadow: 0 8px 28px rgb(0 0 0 / 0.18);
    animation: drop 0.35s cubic-bezier(0.2, 0.8, 0.3, 1.2);
  }

  .ic {
    flex: none;
    width: 34px;
    height: 34px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font-family: var(--font-display);
    font-weight: 800;
    color: #fff;
    background: var(--primary);
  }

  .xp .ic {
    background: var(--xp);
  }

  .coin .ic {
    background: var(--coin-bg);
    color: #5a3a04;
  }

  .streak .ic {
    background: var(--over);
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .text span {
    font-style: italic;
    color: var(--muted);
  }

  @keyframes drop {
    from { transform: translate(-50%, -24px); opacity: 0; }
  }

  @media (prefers-reduced-motion: reduce) {
    .notice { animation: none; }
  }
</style>
