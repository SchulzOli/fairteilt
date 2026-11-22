<script lang="ts">
  import "../app.css";
  import { page } from "$app/state";

  let { children } = $props();

  const tabs = [
    { href: "/", label: "Heute", icon: "☀️" },
    { href: "/kalender", label: "Kalender", icon: "📅" },
    { href: "/board", label: "Board", icon: "🧩" },
    { href: "/punkte", label: "Punkte", icon: "⭐" },
    { href: "/mehr", label: "Mehr", icon: "☰" },
  ];

  const isActive = (href: string) =>
    href === "/" ? page.url.pathname === "/" : page.url.pathname.startsWith(href);
</script>

<div class="shell">
  <main>
    {@render children()}
  </main>

  <nav aria-label="Hauptnavigation">
    {#each tabs as tab (tab.href)}
      <a href={tab.href} class:active={isActive(tab.href)} aria-current={isActive(tab.href) ? "page" : undefined}>
        <span class="icon" aria-hidden="true">{tab.icon}</span>
        <span class="label">{tab.label}</span>
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
    grid-template-columns: repeat(5, 1fr);
    height: calc(var(--nav-h) + env(safe-area-inset-bottom));
    padding-bottom: env(safe-area-inset-bottom);
    background: color-mix(in srgb, var(--surface) 92%, transparent);
    backdrop-filter: blur(12px);
    border-top: 1px solid var(--border);
    z-index: 10;
  }

  nav a {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    color: var(--text-muted);
    text-decoration: none;
    font-size: 0.72rem;
    font-weight: 600;
  }

  nav a .icon {
    font-size: 1.3rem;
    filter: grayscale(1);
    opacity: 0.7;
  }

  nav a.active {
    color: var(--accent);
  }

  nav a.active .icon {
    filter: none;
    opacity: 1;
  }
</style>
