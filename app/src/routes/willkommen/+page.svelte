<script lang="ts">
  import { ApiError, session } from "$lib/session.svelte";
  import AnimalPicker from "$lib/ui/AnimalPicker.svelte";

  type Mode = "login" | "register" | "child";
  let mode = $state<Mode>("login");

  let email = $state("");
  let password = $state("");
  let name = $state("");
  let code = $state("");
  let animal = $state("otter");
  let color = $state("#e8660f");
  let server = $state(session.serverUrl);

  let busy = $state(false);
  let error = $state<string | null>(null);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    error = null;
    busy = true;
    session.setServerUrl(server);
    try {
      if (mode === "login") await session.login(email, password);
      else if (mode === "register") await session.register(email, password, name);
      else await session.joinWithoutAccount(code, name, animal, color);
    } catch (err) {
      error = err instanceof ApiError ? err.message : "Das hat nicht geklappt. Bitte versuch es noch einmal.";
    } finally {
      busy = false;
    }
  }
</script>

<div class="hero">
  <svg class="logo" viewBox="0 0 100 100" aria-hidden="true">
    <rect width="100" height="100" rx="23" fill="#4338ca" />
    <circle cx="47" cy="52" r="30" fill="#f4c246" />
    <path d="M55 16L47 26 57 42 44 58 53 74 44 86" stroke="#4338ca" stroke-width="6" fill="none" stroke-linejoin="round" />
    <circle cx="36" cy="40" r="3.6" fill="#5b3415" /><circle cx="64" cy="42" r="3.4" fill="#5b3415" /><circle cx="60" cy="64" r="3" fill="#5b3415" />
  </svg>
  <h1 class="page-title">Fairteilt</h1>
  <p class="muted">Haushalt fair teilen, gemeinsam gegen Krümel.</p>
</div>

<div class="seg" role="tablist" aria-label="Anmeldung">
  <button role="tab" aria-selected={mode === "login"} class:sel={mode === "login"} onclick={() => (mode = "login")}>Anmelden</button>
  <button role="tab" aria-selected={mode === "register"} class:sel={mode === "register"} onclick={() => (mode = "register")}>Registrieren</button>
  <button role="tab" aria-selected={mode === "child"} class:sel={mode === "child"} onclick={() => (mode = "child")}>Kinder-Code</button>
</div>

<form class="card stack" onsubmit={submit}>
  {#if mode === "child"}
    <p class="muted small">Für Kinder ohne E-Mail: Lass dir von deinen Eltern einen Kinder-Code geben.</p>
    <label class="field"><span>Einladungscode</span><input id="child-code" type="text" bind:value={code} placeholder="K7QF-M2XA" autocomplete="off" required /></label>
    <label class="field"><span>Dein Name</span><input id="child-name" type="text" bind:value={name} maxlength="40" required /></label>
    <AnimalPicker bind:animal bind:color {name} />
  {:else}
    {#if mode === "register"}
      <label class="field"><span>Dein Name</span><input id="reg-name" type="text" bind:value={name} maxlength="40" autocomplete="name" required /></label>
    {/if}
    <label class="field"><span>E-Mail</span><input id="email" type="email" bind:value={email} autocomplete="email" required /></label>
    <label class="field">
      <span>Passwort</span>
      <input id="password" type="password" bind:value={password} minlength={mode === "register" ? 10 : undefined}
        autocomplete={mode === "register" ? "new-password" : "current-password"} required />
      {#if mode === "register"}<small class="muted">Mindestens 10 Zeichen.</small>{/if}
    </label>
  {/if}

  {#if error}<p class="err" role="alert">{error}</p>{/if}

  <button class="primary" type="submit" disabled={busy}>
    {busy ? "Einen Moment …" : mode === "login" ? "Anmelden" : mode === "register" ? "Konto anlegen" : "Beitreten"}
  </button>

  <details>
    <summary class="muted small">Server: {server}</summary>
    <label class="field"><span>Server-Adresse</span><input id="server" type="url" bind:value={server} placeholder="https://fairteilt.oli-tech.eu" /></label>
  </details>
</form>

<style>
  .hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    margin: var(--space-5) 0;
    text-align: center;
  }

  .logo {
    width: 72px;
    height: 72px;
  }

  .seg {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 4px;
    padding: 4px;
    margin-bottom: var(--space-3);
    border-radius: 14px;
    background: var(--surface-2);
  }

  .seg button {
    min-height: 38px;
    padding: 0;
    background: transparent;
    color: var(--muted);
    border-radius: 10px;
  }

  .seg button.sel {
    background: var(--surface);
    color: var(--fg);
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.12);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-weight: 700;
    font-size: 0.9rem;
  }

  .small {
    font-size: 0.85rem;
    margin: 0;
  }

  .err {
    color: var(--over);
    font-weight: 700;
    margin: 0;
  }

  details summary {
    cursor: pointer;
  }

  details .field {
    margin-top: var(--space-2);
  }
</style>
