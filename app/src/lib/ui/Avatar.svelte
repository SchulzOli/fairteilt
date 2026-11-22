<script lang="ts" module>
  // Nine animal avatars (KONZEPT §7.5). Body uses currentColor = the person's colour;
  // eyes and muzzle are fixed ink/white so they read on any tint.
  const INK = "#1b1b26";
  const SAND = "#fde7c4";
  const mane = Array.from({ length: 12 }, (_, i) => {
    const a = (i * 30 * Math.PI) / 180;
    return `<circle cx="${(20 + 14 * Math.cos(a)).toFixed(1)}" cy="${(21 + 14 * Math.sin(a)).toFixed(1)}" r="4.2" fill="currentColor"/>`;
  }).join("");

  export const ANIMALS = {
    fox: { label: "Fuchs", svg: `<path d="M7 7l9 7h8l9-7-2 15c-1.5 6-6 10-11 10S10.5 28 9 22z" fill="currentColor"/><path d="M13 25c2 4 4.5 5.5 7 5.5s5-1.5 7-5.5c-2.5 1.2-4.7 1.5-7 1.5s-4.5-.3-7-1.5z" fill="#fff"/><circle cx="15" cy="20" r="1.8" fill="${INK}"/><circle cx="25" cy="20" r="1.8" fill="${INK}"/><circle cx="20" cy="27.5" r="1.6" fill="${INK}"/>` },
    owl: { label: "Eule", svg: `<path d="M9 8l5 4h12l5-4v18c0 6-5 9-11 9S9 32 9 26z" fill="currentColor"/><circle cx="15" cy="18" r="5" fill="#fff"/><circle cx="25" cy="18" r="5" fill="#fff"/><circle cx="15" cy="18" r="2.2" fill="${INK}"/><circle cx="25" cy="18" r="2.2" fill="${INK}"/><path d="M18 23l2 3 2-3z" fill="#f4c246"/>` },
    otter: { label: "Otter", svg: `<circle cx="11" cy="12" r="3.5" fill="currentColor"/><circle cx="29" cy="12" r="3.5" fill="currentColor"/><circle cx="20" cy="21" r="13" fill="currentColor"/><ellipse cx="20" cy="26" rx="7" ry="5" fill="#fff"/><circle cx="15" cy="18" r="1.8" fill="${INK}"/><circle cx="25" cy="18" r="1.8" fill="${INK}"/><ellipse cx="20" cy="23.5" rx="2.4" ry="1.6" fill="${INK}"/>` },
    hedgehog: { label: "Igel", svg: `<path d="M6 27l2-5-3-3 4-2-1-5 5 1 2-5 3 4 4-4 2 5 5-2v5l5 1-3 4 3 3-4 3z" fill="currentColor"/><path d="M14 22c0-4 3-6 6-6s6 2 6 6-2.5 9-6 9-6-5-6-9z" fill="#fff"/><circle cx="17.5" cy="22" r="1.5" fill="${INK}"/><circle cx="22.5" cy="22" r="1.5" fill="${INK}"/><circle cx="20" cy="27.5" r="1.6" fill="${INK}"/>` },
    bear: { label: "Bär", svg: `<circle cx="10.5" cy="11" r="5" fill="currentColor"/><circle cx="29.5" cy="11" r="5" fill="currentColor"/><circle cx="20" cy="22" r="13" fill="currentColor"/><ellipse cx="20" cy="27" rx="6.5" ry="5" fill="#fff"/><circle cx="15" cy="19.5" r="1.8" fill="${INK}"/><circle cx="25" cy="19.5" r="1.8" fill="${INK}"/><ellipse cx="20" cy="25.5" rx="2.4" ry="1.7" fill="${INK}"/>` },
    cat: { label: "Katze", svg: `<path d="M8 6l7 7h10l7-7v17c0 7-5.5 11-12 11S8 30 8 23z" fill="currentColor"/><circle cx="15" cy="20" r="2" fill="#fff"/><circle cx="25" cy="20" r="2" fill="#fff"/><circle cx="15" cy="20" r="1" fill="${INK}"/><circle cx="25" cy="20" r="1" fill="${INK}"/><path d="M18.5 25h3L20 27z" fill="#f2a0b8"/>` },
    dog: { label: "Hund", svg: `<path d="M10.5 9.5C6.5 10 4.5 16 5.5 23c.4 2.8 3 3.6 4.6 1.6l2.6-10.5z" fill="currentColor" opacity=".7"/><path d="M29.5 9.5c4 .5 6 6.5 5 13.5-.4 2.8-3 3.6-4.6 1.6l-2.6-10.5z" fill="currentColor" opacity=".7"/><ellipse cx="20" cy="21" rx="11.5" ry="12.5" fill="currentColor"/><ellipse cx="20" cy="27" rx="7" ry="5.5" fill="#fff"/><circle cx="15" cy="19" r="1.8" fill="${INK}"/><circle cx="25" cy="19" r="1.8" fill="${INK}"/><ellipse cx="20" cy="24.4" rx="2.7" ry="1.9" fill="${INK}"/>` },
    whale: { label: "Wal", svg: `<path d="M4.5 23c0-7 6-11.5 14-11.5 7.5 0 11.5 4 13.3 8.3l3-4.2c1.2-1.3 3.2-.4 2.8 1.3L36 22.5l2.2 4c.6 1.6-1.2 2.6-2.6 1.6L32.3 25C30.5 30.5 25 33 19 33 10 33 4.5 29 4.5 23z" fill="currentColor"/><path d="M6.5 26c3 4.5 8 5.7 12.5 5.7 5 0 9-1.6 11.5-4.6-6.5 1.6-16.5 1.6-24-1.1z" fill="#fff"/><circle cx="12.5" cy="20.5" r="1.8" fill="${INK}"/>` },
    lion: { label: "Löwe", svg: `${mane}<circle cx="20" cy="21" r="14" fill="currentColor"/><circle cx="13.2" cy="14.2" r="2.6" fill="${SAND}"/><circle cx="26.8" cy="14.2" r="2.6" fill="${SAND}"/><circle cx="20" cy="22" r="9" fill="${SAND}"/><circle cx="16.8" cy="20.6" r="1.5" fill="${INK}"/><circle cx="23.2" cy="20.6" r="1.5" fill="${INK}"/><path d="M18.1 24.2h3.8L20 26.4z" fill="${INK}"/>` },
  } as const;
  export type AnimalName = keyof typeof ANIMALS;
</script>

<script lang="ts">
  let { animal, color, size = 34, name }: { animal: AnimalName; color: string; size?: number; name?: string } = $props();
</script>

<span
  class="avatar"
  style="--c: {color}; width: {size}px; height: {size}px"
  role="img"
  aria-label={name ? `${name} (${ANIMALS[animal].label})` : ANIMALS[animal].label}
>
  <svg viewBox="0 0 40 40" aria-hidden="true">{@html ANIMALS[animal].svg}</svg>
</span>

<style>
  .avatar {
    display: grid;
    place-items: center;
    flex: none;
    border-radius: 50%;
    background: color-mix(in srgb, var(--c) 18%, var(--surface));
    color: var(--c);
  }

  svg {
    width: 82%;
    height: 82%;
  }
</style>
