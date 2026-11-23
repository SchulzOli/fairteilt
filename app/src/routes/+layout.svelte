<script lang="ts">
  import "../app.css";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import Icon, { type IconName } from "$lib/ui/Icon.svelte";
  import { applyTheme, settings } from "$lib/settings.svelte";
  import { session } from "$lib/session.svelte";
  import CheckIn from "$lib/CheckIn.svelte";
  import NoticeBar from "$lib/NoticeBar.svelte";
  import { taskStore } from "$lib/tasks.svelte";
  import * as reminders from "$lib/reminders";

  let { children } = $props();

  // Per-device settings, then the session (refresh token from secure storage).
  settings.init();
  $effect(() => {
    applyTheme(settings.theme);
    void settings.sound;
    void settings.vibration;
    settings.save();
  });
  void session.init();

  // Route guard: signed out → Willkommen; signed in without household → Einrichten.
  const OPEN = ["/willkommen"];
  $effect(() => {
    const path = page.url.pathname;
    if (session.status === "signed_out" && !OPEN.includes(path)) void goto("/willkommen", { replaceState: true });
    if (session.status === "ready" && !session.membership && path !== "/einrichten") void goto("/einrichten", { replaceState: true });
    if (session.status === "ready" && session.membership && (path === "/willkommen" || path === "/einrichten")) void goto("/", { replaceState: true });
  });

  // Tasks and rooms of the active household, live over the WebSocket.
  $effect(() => {
    const hid = session.status === "ready" ? session.membership?.household_id : undefined;
    if (hid) taskStore.start(hid, session.membership?.membership_id);
    else taskStore.stop();
  });

  // Local reminders for my own tasks (bald fällig, check-in), planned after every reload.
  $effect(() => {
    void taskStore.version;
    const me = session.membership?.membership_id;
    if (settings.reminders && session.status === "ready" && !session.profile) void reminders.sync(taskStore.tasks, me);
    else void reminders.clear();
  });

  // Buttons on a reminder: like the in-app check-in; a tap opens the task.
  reminders.setActionHandler(async (actionId, taskId) => {
    try {
      if (actionId === "snooze") await taskStore.action(taskId, "snooze", 10);
      else if (actionId === "later") await taskStore.action(taskId, "pause");
      else if (actionId === "done") {
        const t = taskStore.tasks.find((x) => x.id === taskId);
        const open = t?.steps.filter((s) => s.done_at === null) ?? [];
        if (open.length === 0) await taskStore.action(taskId, "done");
        for (const s of open) await taskStore.checkStep(taskId, s.id);
      } else void goto(`/aufgabe/${taskId}`);
    } catch {
      void goto(`/aufgabe/${taskId}`);
    }
  });

  const tabs: { href: string; label: string; icon: IconName }[] = [
    { href: "/", label: "Heute", icon: "sun" },
    { href: "/plan", label: "Plan", icon: "plan" },
    { href: "/punkte", label: "Punkte", icon: "star" },
    { href: "/familie", label: "Familie", icon: "family" },
  ];

  const isActive = (href: string) => (href === "/" ? page.url.pathname === "/" : page.url.pathname.startsWith(href));
  const showNav = $derived(session.status === "ready" && !!session.membership && !["/willkommen", "/einrichten"].includes(page.url.pathname));
</script>

<svelte:window onpointerdown={() => session.touch()} onkeydown={() => session.touch()} onfocus={() => taskStore.wake()} />
<svelte:document onvisibilitychange={() => document.visibilityState === "visible" && taskStore.wake()} />

<div class="shell">
  {#if session.profile}
    <div class="profile-bar" role="status">
      <span>Angemeldet als <b>{session.profile.name}</b></span>
      <button onclick={() => session.returnToDevice()}>Zurück zu {session.profile.deviceUser}</button>
    </div>
  {/if}

  <main class:bare={!showNav}>
    {#if session.status === "loading"}
      <p class="muted loading">Fairteilt lädt …</p>
    {:else}
      {@render children()}
    {/if}
  </main>

  {#if showNav}
    <CheckIn />
    <NoticeBar />
    <nav aria-label="Hauptnavigation">
      {#each tabs as tab (tab.href)}
        <a href={tab.href} class:active={isActive(tab.href)} aria-current={isActive(tab.href) ? "page" : undefined}>
          <Icon name={tab.icon} size={22} />
          <span>{tab.label}</span>
        </a>
      {/each}
    </nav>
  {/if}
</div>

<style>
  .shell {
    min-height: 100dvh;
    display: flex;
    flex-direction: column;
  }

  .profile-bar {
    position: sticky;
    top: 0;
    z-index: 20;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: calc(env(safe-area-inset-top) + var(--space-2)) var(--space-4) var(--space-2);
    background: var(--primary);
    color: var(--primary-fg);
    font-size: 0.9rem;
  }

  .profile-bar button {
    min-height: 36px;
    background: var(--primary-fg);
    color: var(--primary);
    font-size: 0.85rem;
  }

  main {
    flex: 1;
    width: 100%;
    max-width: 640px;
    margin: 0 auto;
    padding: calc(env(safe-area-inset-top) + var(--space-4)) var(--space-4)
      calc(var(--nav-h) + env(safe-area-inset-bottom) + var(--space-5));
  }

  main.bare {
    padding-bottom: calc(env(safe-area-inset-bottom) + var(--space-5));
  }

  .loading {
    text-align: center;
    margin-top: 30vh;
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
