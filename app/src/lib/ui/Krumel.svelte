<script lang="ts">
  // Krümel, the mascot the family races against (KONZEPT §6).
  let { size = 28 }: { size?: number } = $props();
</script>

<svg class="krumel" viewBox="0 0 48 48" width={size} height={size} role="img" aria-label="Krümel">
  <path
    d="M24 5c3 0 4 3 6.5 3.5S35 6.5 37 8.5s.5 4.5 2 6.5 4 2.5 4 5.5-2.5 3.5-2.5 6 2.5 4 1 6.5-4.5 1.5-6.5 3-1 4.5-4 5.5-4.5-1.5-7-1.5-4 2.5-7 1.5-2-4-4-5.5-5 0-6.5-3 1-4 1-6.5-2.5-3-2.5-6 2.5-3 4-5.5-.5-4.5 2-6.5 4.5 0 7-.5S21 5 24 5z"
    fill="currentColor"
  />
  <circle cx="18" cy="21" r="5" fill="#fff" />
  <circle cx="30" cy="21" r="5" fill="#fff" />
  <circle cx="19" cy="22" r="2.4" fill="#1b1b26" />
  <circle cx="31" cy="22" r="2.4" fill="#1b1b26" />
  <path d="M17 31c2.5 3 11.5 3 14 0" stroke="#1b1b26" stroke-width="2" fill="none" stroke-linecap="round" />
</svg>

<style>
  .krumel {
    color: var(--krumel);
    flex: none;
  }
</style>
