<script lang="ts">
  import { unregisterPush } from "$lib/push";
  import QRCode from "qrcode";
  import type { InviteResponse, MemberDto } from "$lib/generated";
  import { ApiError, session } from "$lib/session.svelte";
  import { feedback, settings, type Theme } from "$lib/settings.svelte";
  import AnimalPicker from "$lib/ui/AnimalPicker.svelte";
  import Avatar, { type AnimalName } from "$lib/ui/Avatar.svelte";
  import Icon, { type IconName } from "$lib/ui/Icon.svelte";

  type Panel = null | { kind: "invite" } | { kind: "child" } | { kind: "member"; member: MemberDto };
  let panel = $state<Panel>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    if (session.membership) void session.loadMembers().catch(() => {});
  });

  const roleLabel: Record<string, string> = { admin: "Admin", member: "Mitglied", child: "Kind" };
  const themes: { value: Theme; label: string; icon: IconName }[] = [
    { value: "light", label: "Hell", icon: "sun" },
    { value: "dark", label: "Dunkel", icon: "moon" },
    { value: "system", label: "System", icon: "auto" },
  ];

  async function run(fn: () => Promise<void>) {
    error = null;
    busy = true;
    try {
      await fn();
    } catch (e) {
      error = e instanceof ApiError ? e.message : "Das hat nicht geklappt.";
    } finally {
      busy = false;
    }
  }

  // ---------- invite ----------
  let inviteRole = $state<"member" | "child" | "admin">("member");
  let invite = $state<InviteResponse | null>(null);
  let qrSvg = $state("");
  async function createInvite() {
    await run(async () => {
      invite = await session.createInvite(inviteRole);
      qrSvg = await QRCode.toString(`FAIRTEILT:${invite.code}`, { type: "svg", margin: 1, errorCorrectionLevel: "M" });
    });
  }
  const expires = (iso: string) => new Intl.DateTimeFormat("de-DE", { day: "numeric", month: "long" }).format(new Date(iso));

  // ---------- child profile ----------
  let childName = $state("");
  let childAnimal = $state("otter");
  let childColor = $state("#e8660f");
  let childPin = $state("");
  async function createChild(e: SubmitEvent) {
    e.preventDefault();
    await run(async () => {
      await session.createChild(childName, childAnimal, childColor, childPin || null);
      panel = null;
      childName = childPin = "";
    });
  }

  // ---------- member editor ----------
  let editAnimal = $state("fox");
  let editColor = $state("#2563eb");
  let editWeight = $state(1);
  let editRole = $state("member");
  let editPin = $state("");
  function openMember(m: MemberDto) {
    editAnimal = m.animal;
    editColor = m.color;
    editWeight = m.weight;
    editRole = m.role;
    editPin = "";
    error = null;
    panel = { kind: "member", member: m };
  }
  const isMe = (m: MemberDto) => m.membership_id === session.membership?.membership_id;
  async function saveMember(m: MemberDto) {
    await run(async () => {
      const patch: { animal?: string; color?: string; weight?: number; role?: string } = {};
      if (editAnimal !== m.animal) patch.animal = editAnimal;
      if (editColor !== m.color) patch.color = editColor;
      if (session.isAdmin && editWeight !== m.weight) patch.weight = editWeight;
      if (session.isAdmin && editRole !== m.role) patch.role = editRole;
      if (Object.keys(patch).length) await session.updateMember(m.membership_id, patch);
      if (editPin) await session.setPin(m.membership_id, editPin);
      panel = null;
    });
  }
  const canEdit = (m: MemberDto) => session.isAdmin || isMe(m);
</script>

<h1 class="page-title">Familie</h1>
<p class="muted">{session.membership?.household_name}</p>

<h2 class="section-title">Mitglieder</h2>
<div class="card list">
  {#each session.members as m (m.membership_id)}
    <button class="item" onclick={() => canEdit(m) && openMember(m)} disabled={!canEdit(m)}>
      <Avatar animal={m.animal as AnimalName} color={m.color} size={40} name={m.display_name} />
      <span class="who">
        <span class="title">{m.display_name}{isMe(m) ? " (du)" : ""}</span>
        <span class="muted small">{roleLabel[m.role] ?? m.role} · Gewicht {String(m.weight).replace(".", ",")}{m.has_pin ? " · PIN" : ""}</span>
      </span>
      {#if canEdit(m)}<Icon name="chevron" size={18} />{/if}
    </button>
  {:else}
    <p class="muted pad">Mitglieder werden geladen …</p>
  {/each}
</div>

<div class="actions">
  {#if session.isAdmin}
    <button class="primary" onclick={() => { invite = null; error = null; panel = { kind: "invite" }; }}>Einladen</button>
    <button onclick={() => { error = null; panel = { kind: "child" }; }}>Kinderprofil anlegen</button>
  {/if}
  {#if session.members.some((m) => m.has_pin && !isMe(m))}
    <a class="btnlike" href="/profil">Profil wechseln (Tablet)</a>
  {/if}
  <a class="btnlike" href="/regeln">Regeln des Haushalts</a>
  {#if session.isAdmin}<a class="btnlike" href="/wand">Wandanzeige starten</a>{/if}
</div>

{#if panel}
  <section class="card stack panel" aria-live="polite">
    {#if panel.kind === "invite"}
      <h2 class="sub">Jemanden einladen</h2>
      {#if !invite}
        <div class="seg" role="radiogroup" aria-label="Rolle">
          <button role="radio" aria-checked={inviteRole === "member"} class:sel={inviteRole === "member"} onclick={() => (inviteRole = "member")}>Erwachsene</button>
          <button role="radio" aria-checked={inviteRole === "admin"} class:sel={inviteRole === "admin"} onclick={() => (inviteRole = "admin")}>Admin</button>
          <button role="radio" aria-checked={inviteRole === "child"} class:sel={inviteRole === "child"} onclick={() => (inviteRole = "child")}>Kind</button>
        </div>
        <p class="muted small">
          {inviteRole === "child"
            ? "Kinder treten auf ihrem eigenen Gerät mit diesem Code bei, ohne E-Mail („Kinder-Code“)."
            : "Die Person registriert sich mit E-Mail und tritt dann mit dem Code bei oder scannt den QR-Code."}
        </p>
        <button class="primary" onclick={createInvite} disabled={busy}>Code erzeugen</button>
      {:else}
        <div class="qr" aria-label="QR-Code für {invite.code}">{@html qrSvg}</div>
        <p class="code">{invite.code}</p>
        <p class="muted small center">Gilt einmal, bis {expires(invite.expires_at)}.</p>
        <button onclick={() => (panel = null)}>Fertig</button>
      {/if}
    {:else if panel.kind === "child"}
      <h2 class="sub">Kinderprofil anlegen</h2>
      <p class="muted small">Für Kinder ohne eigenes Handy: Sie melden sich am Familien-Tablet mit ihrer PIN an.</p>
      <form class="stack" onsubmit={createChild}>
        <label class="field"><span>Name</span><input id="child-name" type="text" bind:value={childName} maxlength="40" required /></label>
        <AnimalPicker bind:animal={childAnimal} bind:color={childColor} name={childName} />
        <label class="field"><span>PIN (4 Ziffern)</span><input id="child-pin" type="password" inputmode="numeric" pattern={"[0-9]{4,6}"} bind:value={childPin} placeholder="optional" /></label>
        <button class="primary" type="submit" disabled={busy}>Anlegen</button>
      </form>
    {:else if panel.kind === "member"}
      {@const m = panel.member}
      <h2 class="sub">{m.display_name}</h2>
      <AnimalPicker bind:animal={editAnimal} bind:color={editColor} name={m.display_name} />
      {#if session.isAdmin}
        <label class="field"><span>Gewicht: {String(editWeight).replace(".", ",")}</span>
          <input id="weight" type="range" min="0" max="1.5" step="0.1" bind:value={editWeight} />
          <small class="muted">1 = Erwachsener, 0,3 = Kind. Bestimmt den fairen Anteil.</small>
        </label>
        <label class="field"><span>Rolle</span>
          <select id="role" bind:value={editRole}>
            <option value="admin">Admin</option><option value="member">Mitglied</option><option value="child">Kind</option>
          </select>
        </label>
      {/if}
      <label class="field"><span>{m.has_pin ? "Neue PIN" : "PIN für das Familien-Tablet"}</span>
        <input id="edit-pin" type="password" inputmode="numeric" pattern={"[0-9]{4,6}"} bind:value={editPin} placeholder="4 Ziffern, leer lassen = unverändert" />
      </label>
      <button class="primary" onclick={() => saveMember(m)} disabled={busy}>Speichern</button>
    {/if}
    {#if error}<p class="err" role="alert">{error}</p>{/if}
    <button class="link" onclick={() => (panel = null)}>Schließen</button>
  </section>
{/if}

<h2 class="section-title">Auf diesem Gerät</h2>
<div class="card stack">
  <div class="seg" role="radiogroup" aria-label="Darstellung">
    {#each themes as t (t.value)}
      <button role="radio" aria-checked={settings.theme === t.value} class:sel={settings.theme === t.value} onclick={() => (settings.theme = t.value)}>
        <Icon name={t.icon} size={16} />{t.label}
      </button>
    {/each}
  </div>
  <label class="toggle"><span class="row"><Icon name="speaker" size={20} /><b>Töne</b></span><input type="checkbox" bind:checked={settings.sound} /></label>
  <label class="toggle"><span class="row"><Icon name="bell" size={20} /><b>Vibration</b></span><input type="checkbox" bind:checked={settings.vibration} /></label>
  <div class="row wrap">
    <button onclick={() => feedback("done")}>Erledigt</button>
    <button onclick={() => feedback("overtake")}>Krümel überholt</button>
    <button onclick={() => feedback("goal")}>Familienziel</button>
  </div>
</div>

<h2 class="section-title">Konto</h2>
<div class="card stack">
  <p class="small">Angemeldet als <b>{session.me?.user.display_name}</b>{session.me?.user.email ? ` (${session.me.user.email})` : ""}</p>
  <p class="muted small">Server: {session.serverUrl}</p>
  <button onclick={async () => { await unregisterPush(); await session.signOut(); }}>Abmelden</button>
</div>

<style>
  .small {
    font-size: 0.85rem;
    margin: 0;
  }

  .list {
    padding: 0;
  }

  .item {
    width: 100%;
    display: grid;
    grid-template-columns: 40px 1fr auto;
    align-items: center;
    gap: var(--space-3);
    min-height: 64px;
    padding: var(--space-2) var(--space-4);
    background: transparent;
    border-radius: 0;
    color: var(--muted);
    text-align: left;
    font-weight: 400;
  }

  .item:disabled {
    opacity: 1;
    cursor: default;
  }

  .item + .item {
    border-top: 1px solid var(--line);
  }

  .who {
    display: flex;
    flex-direction: column;
  }

  .title {
    font-weight: 700;
    color: var(--fg);
  }

  .pad {
    padding: var(--space-4);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }

  .btnlike {
    display: inline-flex;
    align-items: center;
    min-height: var(--tap);
    padding: 0 var(--space-4);
    border-radius: var(--radius-sm);
    background: var(--surface-2);
    color: var(--fg);
    font-weight: 700;
    text-decoration: none;
  }

  .panel {
    margin-top: var(--space-3);
    border: 2px solid var(--primary);
  }

  .sub {
    font-size: 1.15rem;
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

  .qr {
    width: min(240px, 70vw);
    margin: 0 auto;
    padding: var(--space-2);
    background: #fff;
    border-radius: var(--radius-sm);
  }

  .qr :global(svg) {
    display: block;
    width: 100%;
    height: auto;
  }

  .code {
    font-family: var(--font-display);
    font-size: 2rem;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-align: center;
    margin: 0;
    font-variant-numeric: tabular-nums;
  }

  .center {
    text-align: center;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-weight: 700;
    font-size: 0.9rem;
  }

  .toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
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

  .err {
    color: var(--over);
    font-weight: 700;
    margin: 0;
  }

  .link {
    background: transparent;
    color: var(--muted);
  }
</style>
