<script lang="ts">
  import "../app.css";
  import { page } from "$app/state";
  import Icon, { type IconName } from "$lib/ui/Icon.svelte";
  import { applyTheme, settings } from "$lib/settings.svelte";

  let { children } = $props();

  // Load per-device settings once, then keep theme and storage in sync.
  settings.init();
  $effect(() => {
    applyTheme(settings.theme);
    settings.save();
    // track sound/vibration too so toggles persist
    void settings.sound;
    void settings.vibration;
  });

  const tabs: { href: string; label: string; icon: IconName }[] = [
    { href: "/", label: "Heute", icon: "sun" },
    { href: "/plan", label: "Plan", icon: "plan" },
    { href: "/punkte", label: "Punkte", icon: "star" },
    { href: "/familie", label: "Familie", icon: "family" },
  ];

  const isActive = (href: string) => (href === "/" ? page.url.pathname === "/" : page.url.pathname.startsWith(href));
</script>

<div class="shell">
  <main>
    {@render children()}
  </main>

  <nav aria-label="Hauptnavigation">
    {#each tabs as tab (tab.href)}
      <a href={tab.href} class:active={isActive(tab.href)} aria-current={isActive(tab.href) ? "page" : undefined}>
        <Icon name={tab.icon} size={22} />
        <span>{tab.label}</span>
      </a>
    {/each}
  </nav>
</div>

<style>
  .shell {
    min-height: 100dvh;
    display: flex;
    flex-direction: column;
  }

  main {
    flex: 1;
    width: 100%;
    max-width: 640px;
    margin: 0 auto;
    padding: calc(env(safe-area-inset-top) + var(--space-4)) var(--space-4)
      calc(var(--nav-h) + env(safe-area-inset-bottom) + var(--space-5));
  }

  nav {
    position: fixed;
    inset: auto 0 0 0;
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    height: calc(var(--nav-h) + env(safe-area-inset-bottom));
    padding-bottom: env(safe-area-inset-bottom);
    background: color-mix(in srgb, var(--surface) 94%, transparent);
    backdrop-filter: blur(12px);
    border-top: 1px solid var(--line);
    z-index: 10;
  }

  nav a {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    color: var(--muted);
    text-decoration: none;
    font-size: 0.74rem;
    font-weight: 700;
  }

  nav a.active {
    color: var(--primary);
  }
</style>
