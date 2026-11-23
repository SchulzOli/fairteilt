<script lang="ts">
  // Postfach (KONZEPT §8): everything that concerns you, newest first. Tapping an entry opens
  // what it is about. Below: which pushes you want, quiet hours, reminders on this device.
  import { goto } from "$app/navigation";
  import type { NotificationDto, NotificationPrefsDto } from "$lib/generated";
  import { session } from "$lib/session.svelte";
  import { settings } from "$lib/settings.svelte";
  import { taskStore } from "$lib/tasks.svelte";
  import Icon, { type IconName } from "$lib/ui/Icon.svelte";

  let items = $state<NotificationDto[]>([]);
  let more = $state(false);
  let prefs = $state<NotificationPrefsDto | null>(null);
  let quiet = $state(false);
  let saved = $state(false);
  let error = $state("");

  const hid = $derived(session.membership?.household_id);
  const PAGE = 30;

  async function load(append = false) {
    if (!hid) return;
    const before = append && items.length ? `&before=${encodeURIComponent(items[items.length - 1].created_at)}` : "";
    const rows = await session.request<NotificationDto[]>("GET", `/households/${hid}/inbox?limit=${PAGE}${before}`);
    items = append ? [...items, ...rows] : rows;
    more = rows.length === PAGE;
  }

  $effect(() => {
    void taskStore.version;
    void load().catch((e) => (error = e.message));
  });

  $effect(() => {
    if (!hid) return;
    session.request<NotificationPrefsDto>("GET", `/households/${hid}/notification-prefs`).then((p) => {
      prefs = p;
      quiet = p.quiet_from !== null;
    });
  });

  async function markRead(ids: string[] | null) {
    const res = await session.request<{ unread: number }>("POST", `/households/${hid}/inbox/read`, { ids });
    taskStore.unread = res.unread;
    const now = new Date().toISOString();
    items = items.map((n) => (ids === null || ids.includes(n.id) ? { ...n, read_at: n.read_at ?? now } : n));
  }

  async function open(n: NotificationDto) {
    if (!n.read_at) await markRead([n.id]).catch(() => {});
    if (n.ref_type === "task" && n.ref_id) void goto(`/aufgabe/${n.ref_id}`);
    else if (n.ref_type === "redemption") void goto("/belohnungen");
    else if (n.ref_type) void goto("/punkte");
  }

  async function savePrefs() {
    if (!prefs) return;
    error = "";
    saved = false;
    try {
      const body = quiet ? { ...prefs, quiet_from: prefs.quiet_from ?? "21:00", quiet_to: prefs.quiet_to ?? "07:00" } : { ...prefs, quiet_from: null, quiet_to: null };
      prefs = await session.request<NotificationPrefsDto>("PUT", `/households/${hid}/notification-prefs`, body);
      saved = true;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  const icons: Record<string, IconName> = { tasks: "plan", reviews: "check", points: "star", period: "flag", reminders: "timer" };
  const time = new Intl.DateTimeFormat("de-DE", { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  const categories: { key: keyof NotificationPrefsDto["push"]; label: string }[] = [
    { key: "tasks", label: "Neue Aufgaben, Bonus, Aufholaufgaben" },
    { key: "reviews", label: "Bestätigen, bestätigt, zurückgegeben" },
    { key: "points", label: "Guthaben, Übertragungen, Belohnungen" },
    { key: "period", label: "Abschluss und Familienziel" },
    { key: "reminders", label: "Eigene Aufgaben: bald fällig, überfällig" },
  ];
</script>

<div class="row between">
  <h1 class="page-title">Postfach</h1>
  {#if items.some((n) => !n.read_at)}
    <button class="link" onclick={() => markRead(null)}>Alle gelesen</button>
  {/if}
</div>

{#if items.length === 0}
  <p class="muted">Noch nichts da. Hier landet, was dich betrifft: neue Aufgaben, Bestätigungen, Guthaben, der Abschluss.</p>
{:else}
  <ul class="card list">
    {#each items as n (n.id)}
      <li>
        <button class:unread={!n.read_at} onclick={() => open(n)}>
          <span class="ic {n.category}" aria-hidden="true"><Icon name={icons[n.category] ?? "bell"} size={18} /></span>
          <span class="text">
            <b>{n.title}</b>
            {#if n.body}<span class="body">{n.body}</span>{/if}
            <span class="muted small">{time.format(new Date(n.created_at))}</span>
          </span>
          {#if !n.read_at}<span class="dot" aria-label="neu"></span>{/if}
        </button>
      </li>
    {/each}
  </ul>
  {#if more}<button class="more" onclick={() => load(true)}>Ältere laden</button>{/if}
{/if}

<h2 class="section-title">Benachrichtigungen</h2>
<section class="card stack">
  <label class="toggle"><span><b>Erinnerungen auf diesem Gerät</b><br /><span class="muted small">Wenn deine Aufgaben fällig werden, und die Nachfrage bei gestarteten Aufgaben</span></span>
    <input type="checkbox" bind:checked={settings.reminders} />
  </label>
  {#if prefs}
    <div class="field"><span>Push aufs Handy</span>
      {#each categories as c (c.key)}
        <label class="toggle small-toggle"><span>{c.label}</span><input type="checkbox" bind:checked={prefs.push[c.key]} onchange={savePrefs} /></label>
      {/each}
    </div>
    <label class="toggle"><span><b>Ruhezeit</b><br /><span class="muted small">Pushes warten, bis sie vorbei ist</span></span>
      <input type="checkbox" bind:checked={quiet} onchange={savePrefs} />
    </label>
    {#if quiet}
      <div class="row times">
        <label>von <input type="time" bind:value={prefs.quiet_from} onchange={savePrefs} /></label>
        <label>bis <input type="time" bind:value={prefs.quiet_to} onchange={savePrefs} /></label>
      </div>
    {/if}
    {#if saved}<p class="ok small" role="status">Gespeichert.</p>{/if}
    {#if error}<p class="err small" role="alert">{error}</p>{/if}
  {/if}
</section>

<style>
  .between {
    justify-content: space-between;
  }

  .link {
    background: transparent;
    border: none;
    color: var(--primary);
    font-weight: 700;
    min-height: 36px;
  }

  .list {
    list-style: none;
    margin: var(--space-3) 0 0;
    padding: 0;
    overflow: hidden;
  }

  .list li + li {
    border-top: 1px solid var(--line);
  }

  .list button {
    width: 100%;
    display: grid;
    grid-template-columns: 36px 1fr auto;
    align-items: start;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    background: transparent;
    border: none;
    border-radius: 0;
    color: inherit;
    text-align: left;
    font-weight: 400;
  }

  .list button.unread {
    background: color-mix(in srgb, var(--primary) 7%, transparent);
  }

  .ic {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--surface-2);
  }

  .ic.reviews {
    background: var(--xp-soft);
    color: var(--xp);
  }

  .ic.points {
    background: var(--coin-soft);
    color: var(--coin);
  }

  .ic.period {
    background: var(--primary-soft);
    color: var(--primary);
  }

  .ic.reminders {
    background: var(--over-soft);
    color: var(--over);
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .body {
    font-style: italic;
  }

  .dot {
    width: 9px;
    height: 9px;
    margin-top: 6px;
    border-radius: 50%;
    background: var(--primary);
  }

  .more {
    display: block;
    margin: var(--space-3) auto 0;
  }

  .toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .small-toggle {
    font-weight: 400;
    padding: 4px 0;
  }

  .times {
    gap: var(--space-4);
  }

  .times input {
    width: auto;
  }

  .ok {
    color: var(--ok);
  }

  .err {
    color: var(--over);
  }
</style>
