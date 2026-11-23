<script lang="ts">
  import { ApiError, session } from "$lib/session.svelte";
  import { cancelDesktop, desktopCode, GoogleCancelled, googleDesktop, googleNative, googleWeb, nativeIdToken, renderGoogleButton } from "$lib/google";
  import type { GoogleCode } from "$lib/generated";
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

  // "Mit Google anmelden": Android shows our button (Google's sheet), the desktop app our button
  // (Google in the browser), the browser Google's own.
  let googleEl = $state<HTMLElement | null>(null);
  const ownButton = googleNative() || googleDesktop();
  const withGoogle = $derived(mode !== "child" && (ownButton || googleWeb()));
  /** Desktop: waiting for the browser. */
  let inBrowser = $state(false);

  async function google(idToken: string | GoogleCode) {
    error = null;
    busy = true;
    session.setServerUrl(server);
    try {
      await session.loginWithGoogle(idToken);
    } catch (err) {
      error = err instanceof ApiError ? err.message : "Die Anmeldung mit Google hat nicht geklappt.";
    } finally {
      busy = false;
    }
  }

  async function googleTap() {
    error = null;
    try {
      if (googleNative()) {
        await google(await nativeIdToken());
      } else {
        inBrowser = true;
        const code = await desktopCode().finally(() => (inBrowser = false));
        await google(code);
      }
    } catch (err) {
      if (!(err instanceof GoogleCancelled)) error = err instanceof Error ? err.message : String(err);
    }
  }

  $effect(() => {
    if (!googleEl || !googleWeb()) return;
    const dark = matchMedia("(prefers-color-scheme: dark)").matches && document.documentElement.dataset.theme !== "light";
    renderGoogleButton(googleEl, (t) => void google(t), dark).catch((e) => (error = e.message));
  });

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

{#if withGoogle}
  <section class="google card">
    {#if inBrowser}
      <p class="wait">Weiter im Browser: Melde dich dort bei Google an.</p>
      <button class="gbtn" onclick={cancelDesktop}>Abbrechen</button>
    {:else if ownButton}
      <button class="gbtn" disabled={busy} onclick={googleTap}>
        <svg viewBox="0 0 48 48" width="20" height="20" aria-hidden="true"><path fill="#EA4335" d="M24 9.5c3.5 0 6.6 1.2 9.1 3.6l6.8-6.8C35.8 2.4 30.3 0 24 0 14.6 0 6.6 5.4 2.6 13.3l7.9 6.1C12.4 13.6 17.7 9.5 24 9.5z"/><path fill="#4285F4" d="M46.1 24.6c0-1.6-.1-3.1-.4-4.6H24v9h12.4c-.5 2.9-2.2 5.3-4.6 6.9l7.4 5.7c4.3-4 6.9-9.9 6.9-17z"/><path fill="#FBBC05" d="M10.5 28.6c-.5-1.4-.8-2.9-.8-4.6s.3-3.2.8-4.6l-7.9-6.1C1 16.6 0 20.2 0 24s1 7.4 2.6 10.7l7.9-6.1z"/><path fill="#34A853" d="M24 48c6.5 0 11.9-2.1 15.9-5.8l-7.4-5.7c-2.1 1.4-4.8 2.3-8.5 2.3-6.3 0-11.6-4.1-13.5-9.9l-7.9 6.1C6.6 42.6 14.6 48 24 48z"/></svg>
        Mit Google fortfahren
      </button>
    {:else}
      <div class="gweb" bind:this={googleEl}></div>
    {/if}
    <p class="muted small">Neu hier? Dann legt das gleich dein Konto an. Hast du schon eins mit derselben E-Mail, wird es verbunden.</p>
  </section>
  <p class="or"><span>oder mit E-Mail</span></p>
{/if}

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
  .google {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }

  .gbtn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    min-height: 48px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--fg);
  }

  .gweb {
    display: flex;
    justify-content: center;
    min-height: 44px;
  }

  .google p {
    margin: 0;
    text-align: center;
  }

  .google .wait {
    font-weight: 700;
  }

  .or {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin: var(--space-3) 0;
    color: var(--muted);
    font-size: 0.8rem;
  }

  .or::before,
  .or::after {
    content: "";
    flex: 1;
    height: 1px;
    background: var(--line);
  }

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
