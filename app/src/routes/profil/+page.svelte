<script lang="ts">
  // Family tablet: pick your animal, enter your PIN, use Fairteilt as yourself.
  import { goto } from "$app/navigation";
  import type { MemberDto } from "$lib/generated";
  import { ApiError, session } from "$lib/session.svelte";
  import Avatar, { type AnimalName } from "$lib/ui/Avatar.svelte";
  import PinPad from "$lib/ui/PinPad.svelte";

  let chosen = $state<MemberDto | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);

  $effect(() => {
    if (session.membership && session.members.length === 0) void session.loadMembers().catch(() => {});
  });

  const candidates = $derived(session.members.filter((m) => m.has_pin && m.membership_id !== session.membership?.membership_id));

  async function enter(pin: string) {
    if (!chosen) return;
    error = null;
    busy = true;
    try {
      await session.switchProfile(chosen.membership_id, pin);
      await goto("/");
    } catch (e) {
      error = e instanceof ApiError ? e.message : "Das hat nicht geklappt.";
    } finally {
      busy = false;
    }
  }
</script>

<h1 class="page-title center">Wer bist du?</h1>
<p class="muted center">Familien-Tablet · nach 2 Minuten ohne Eingabe geht es zurück zu {session.me?.user.display_name}.</p>

<div class="profiles" role="radiogroup" aria-label="Profil">
  {#each candidates as m (m.membership_id)}
    <button role="radio" aria-checked={chosen?.membership_id === m.membership_id} class:sel={chosen?.membership_id === m.membership_id}
      onclick={() => { chosen = m; error = null; }}>
      <Avatar animal={m.animal as AnimalName} color={m.color} size={64} />
      <span>{m.display_name}</span>
    </button>
  {:else}
    <p class="muted center">Noch niemand hat eine PIN. Admins vergeben sie unter Familie.</p>
  {/each}
</div>

{#if chosen}
  <p class="center"><b>PIN für {chosen.display_name}</b></p>
  <PinPad oncomplete={enter} {error} {busy} />
{/if}

<p class="center"><a href="/familie">Abbrechen</a></p>

<style>
  .center {
    text-align: center;
  }

  .profiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(100px, 1fr));
    gap: var(--space-3);
    margin: var(--space-5) 0;
  }

  .profiles button {
    min-height: 110px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }

  .profiles button.sel {
    border: 3px solid var(--primary);
  }

  a {
    color: var(--muted);
  }
</style>
