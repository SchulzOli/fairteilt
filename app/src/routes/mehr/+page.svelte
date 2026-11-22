<script lang="ts">
  import { api, errorMessage, isTauri, type ServerHealth } from "$lib/api";

  const STORAGE_KEY = "punkte.serverUrl";

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
      if (!isTauri()) throw new Error("Nur in der Tauri-App verfügbar");
      health = await api.serverHealth(serverUrl);
    } catch (e) {
      error = errorMessage(e);
    } finally {
      checking = false;
    }
  }

  const sections = [
    { icon: "👨‍👩‍👧", title: "Haushalt & Mitglieder", hint: "Einladen, Rollen, Gewichte (M1)" },
    { icon: "⚖️", title: "Punkteregeln", hint: "Grenzen, Eskalation, Boni, Übertragungen (M3)" },
    { icon: "🔁", title: "Ausgleich", hint: "Periode, Toleranz, Folgen (M4)" },
    { icon: "🎁", title: "Belohnungen", hint: "Katalog und Einlösungen (M4)" },
    { icon: "🔔", title: "Erinnerungen", hint: "Ruhezeiten, Tagesübersicht (M5)" },
  ];
</script>

<header>
  <h1>Mehr</h1>
</header>

<section class="card stack">
  <h2 class="h2">Server</h2>
  <label class="stack tight">
    <span class="muted small">Adresse</span>
    <input type="url" bind:value={serverUrl} placeholder="https://punkte.example.org" autocomplete="off" />
  </label>
  <button class="primary" onclick={check} disabled={checking}>
    {checking ? "Prüfe …" : "Verbindung testen"}
  </button>
  {#if health}
    <p class="ok small">✓ Verbunden · Server {health.version} · Datenbank {health.database ? "ok" : "nicht erreichbar"}</p>
  {/if}
  {#if error}
    <p class="err small">✗ {error}</p>
  {/if}
</section>

<h2 class="section-title">Einstellungen</h2>
<div class="list card">
  {#each sections as s (s.title)}
    <div class="item">
      <span class="icon" aria-hidden="true">{s.icon}</span>
      <div>
        <div class="title">{s.title}</div>
        <div class="muted small">{s.hint}</div>
      </div>
      <span class="muted">›</span>
    </div>
  {/each}
</div>

<style>
  header {
    margin-bottom: var(--space-4);
  }

  h1 {
    font-size: 1.9rem;
  }

  .h2 {
    font-size: 1.1rem;
  }

  .small {
    font-size: 0.85rem;
    margin: 0;
  }

  .tight {
    gap: var(--space-1);
  }

  .ok {
    color: var(--success);
  }

  .err {
    color: var(--danger);
  }

  .section-title {
    margin: var(--space-5) 0 var(--space-3);
  }

  .list {
    padding: 0;
  }

  .item {
    display: grid;
    grid-template-columns: 32px 1fr auto;
    align-items: center;
    gap: var(--space-3);
    min-height: 60px;
    padding: var(--space-2) var(--space-4);
  }

  .item + .item {
    border-top: 1px solid var(--border);
  }

  .icon {
    font-size: 1.3rem;
  }

  .title {
    font-weight: 600;
  }
</style>
