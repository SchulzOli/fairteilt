<script lang="ts">
  import { ApiError, isPhone, parseInviteCode, session } from "$lib/session.svelte";
  import AnimalPicker from "$lib/ui/AnimalPicker.svelte";

  type Mode = "create" | "join";
  let mode = $state<Mode>("create");

  let householdName = $state("");
  let code = $state("");
  let animal = $state("fox");
  let color = $state("#2563eb");

  let busy = $state(false);
  let error = $state<string | null>(null);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    error = null;
    busy = true;
    try {
      if (mode === "create") await session.createHousehold(householdName, animal, color);
      else await session.acceptInvite(code, animal, color);
    } catch (err) {
      error = err instanceof ApiError ? err.message : "Das hat nicht geklappt. Bitte versuch es noch einmal.";
    } finally {
      busy = false;
    }
  }

  async function scan() {
    error = null;
    try {
      const { scan, Format } = await import("@tauri-apps/plugin-barcode-scanner");
      const result = await scan({ formats: [Format.QRCode], windowed: false });
      code = parseInviteCode(result.content);
    } catch {
      error = "Scannen ging nicht. Tipp den Code einfach ab.";
    }
  }
</script>

<h1 class="page-title">Hallo {session.me?.user.display_name}!</h1>
<p class="muted">Lege einen Haushalt an oder tritt einem bei.</p>

<div class="seg" role="tablist" aria-label="Haushalt">
  <button role="tab" aria-selected={mode === "create"} class:sel={mode === "create"} onclick={() => (mode = "create")}>Neu anlegen</button>
  <button role="tab" aria-selected={mode === "join"} class:sel={mode === "join"} onclick={() => (mode = "join")}>Mit Code beitreten</button>
</div>

<form class="card stack" onsubmit={submit}>
  {#if mode === "create"}
    <label class="field"><span>Name des Haushalts</span><input id="hh-name" type="text" bind:value={householdName} maxlength="60" placeholder="z. B. Familie Sonnenschein" required /></label>
    <p class="muted small">Startet mit den Standardregeln: monatlicher Ausgleich, 20 % Toleranz, Bestätigung nur bei Kindern. Alles später änderbar.</p>
  {:else}
    <label class="field"><span>Einladungscode</span><input id="join-code" type="text" bind:value={code} placeholder="K7QF-M2XA" autocomplete="off" required /></label>
    {#if isPhone()}<button type="button" onclick={scan}>QR-Code scannen</button>{/if}
  {/if}

  <h2 class="sub">Dein Tier und deine Farbe</h2>
  <AnimalPicker bind:animal bind:color name={session.me?.user.display_name ?? ""} />

  {#if error}<p class="err" role="alert">{error}</p>{/if}
  <button class="primary" type="submit" disabled={busy}>{busy ? "Einen Moment …" : mode === "create" ? "Haushalt anlegen" : "Beitreten"}</button>
</form>

<button class="link" onclick={() => session.signOut()}>Abmelden</button>

<style>
  .seg {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 4px;
    padding: 4px;
    margin: var(--space-4) 0 var(--space-3);
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

  .sub {
    font-size: 1.05rem;
    margin-top: var(--space-2);
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

  .link {
    margin: var(--space-4) auto 0;
    display: block;
    background: transparent;
    color: var(--muted);
  }
</style>
