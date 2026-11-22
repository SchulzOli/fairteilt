<script lang="ts">
  import { api, errorMessage, isTauri, type ServerHealth } from "$lib/api";
  import { members } from "$lib/demo";
  import { feedback, settings, type Theme } from "$lib/settings.svelte";
  import Avatar from "$lib/ui/Avatar.svelte";
  import Icon, { type IconName } from "$lib/ui/Icon.svelte";

  const STORAGE_KEY = "fairteilt.serverUrl";
  function loadUrl(): string {
    try {
      return localStorage.getItem(STORAGE_KEY) ?? "http://localhost:8080";
    } catch {
      return "http://localhost:8080";
    }
  }

  let serverUrl = $state(loadUrl());
  let health = $state<ServerHealth | null>(null);
  let error = $state<string | null>(null);
  let checking = $state(false);

  async function check() {
    checking = true;
    health = null;
    error = null;
    try {
      localStorage.setItem(STORAGE_KEY, serverUrl);
    } catch {
      /* storage unavailable: keep in memory */
    }
    try {
      if (!isTauri()) throw new Error("Nur in der Fairteilt-App verfügbar");
      health = await api.serverHealth(serverUrl);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      checking = false;
    }
  }

  const themes: { value: Theme; label: string; icon: IconName }[] = [
    { value: "light", label: "Hell", icon: "sun" },
    { value: "dark", label: "Dunkel", icon: "moon" },
    { value: "system", label: "System", icon: "auto" },
  ];

  const roleLabel = { admin: "Admin", member: "Mitglied", child: "Kind" } as const;
</script>

<h1 class="page-title">Familie</h1>

<h2 class="section-title">Mitglieder</h2>
<div class="card list">
  {#each members as m (m.id)}
    <div class="item">
      <Avatar animal={m.animal} color={m.color} size={40} name={m.name} />
      <div>
        <div class="title">{m.name}</div>
        <div class="muted small">{roleLabel[m.role]} · Gewicht {String(m.weight).replace(".", ",")}</div>
      </div>
      <Icon name="chevron" size={18} />
    </div>
  {/each}
</div>

<h2 class="section-title">Auf diesem Gerät</h2>
<div class="card stack">
  <div class="stack tight">
    <span class="label">Darstellung</span>
    <div class="seg" role="radiogroup" aria-label="Darstellung">
      {#each themes as t (t.value)}
        <button role="radio" aria-checked={settings.theme === t.value} class:sel={settings.theme === t.value} onclick={() => (settings.theme = t.value)}>
          <Icon name={t.icon} size={16} />{t.label}
        </button>
      {/each}
    </div>
  </div>

  <label class="toggle">
    <span class="row"><Icon name="speaker" size={20} /><span><b>Töne</b><br /><span class="muted small">Leise Klänge beim Erledigen und bei großen Momenten</span></span></span>
    <input type="checkbox" bind:checked={settings.sound} />
  </label>
  <label class="toggle">
    <span class="row"><Icon name="bell" size={20} /><span><b>Vibration</b><br /><span class="muted small">Kurzes Brummen als Rückmeldung</span></span></span>
    <input type="checkbox" bind:checked={settings.vibration} />
  </label>
  <div class="row wrap">
    <button onclick={() => feedback("done")}>Erledigt</button>
    <button onclick={() => feedback("overtake")}>Krümel überholt</button>
    <button onclick={() => feedback("goal")}>Familienziel</button>
  </div>
</div>

<h2 class="section-title">Server</h2>
<section class="card stack">
  <label class="stack tight">
    <span class="label">Adresse</span>
    <input type="url" bind:value={serverUrl} placeholder="https://fairteilt.oli-tech.eu" autocomplete="off" />
  </label>
  <button class="primary" onclick={check} disabled={checking}>
    <span class="row center"><Icon name="server" size={18} />{checking ? "Prüfe …" : "Verbindung testen"}</span>
  </button>
  {#if health}
    <p class="ok small">Verbunden · Server {health.version} · Datenbank {health.database ? "ok" : "nicht erreichbar"}</p>
  {/if}
  {#if error}
    <p class="err small">Keine Verbindung: {error}</p>
  {/if}
</section>

<style>
  .small {
    font-size: 0.85rem;
  }

  .tight {
    gap: var(--space-2);
  }

  .label {
    font-size: 0.85rem;
    font-weight: 700;
    color: var(--muted);
  }

  .list {
    padding: 0;
  }

  .item {
    display: grid;
    grid-template-columns: 40px 1fr auto;
    align-items: center;
    gap: var(--space-3);
    min-height: 64px;
    padding: var(--space-2) var(--space-4);
    color: var(--muted);
  }

  .item + .item {
    border-top: 1px solid var(--line);
  }

  .title {
    font-weight: 700;
    color: var(--fg);
  }

  .seg {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 4px;
    padding: 4px;
    border-radius: 14px;
    background: var(--surface-2);
  }

  .seg button {
    min-height: 38px;
    padding: 0;
    background: transparent;
    color: var(--muted);
    border-radius: 10px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }

  .seg button.sel {
    background: var(--surface);
    color: var(--fg);
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.12);
  }

  .toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    cursor: pointer;
  }

  .toggle input {
    width: 22px;
    height: 22px;
    accent-color: var(--primary);
  }

  .wrap {
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .wrap button {
    min-height: 38px;
    font-size: 0.85rem;
  }

  .center {
    justify-content: center;
    gap: var(--space-2);
  }

  .ok {
    color: var(--ok);
    margin: 0;
  }

  .err {
    color: var(--over);
    margin: 0;
  }
</style>
