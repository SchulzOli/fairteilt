// Per-device preferences (KONZEPT §7.5): theme, sounds, vibration, local reminders.
// Stored locally on the device, not on the server.
import { playSound, type SoundId } from "./sound";

export type Theme = "system" | "light" | "dark";

const KEY = "fairteilt.settings";

interface Stored {
  theme: Theme;
  sound: boolean;
  vibration: boolean;
  reminders: boolean;
}

function load(): Stored {
  const fallback: Stored = { theme: "system", sound: true, vibration: true, reminders: true };
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? { ...fallback, ...JSON.parse(raw) } : fallback;
  } catch {
    return fallback;
  }
}

class Settings {
  theme = $state<Theme>("system");
  sound = $state(true);
  vibration = $state(true);
  /** Reminders on this device (bald fällig, check-in). */
  reminders = $state(true);

  init() {
    const s = load();
    this.theme = s.theme;
    this.sound = s.sound;
    this.vibration = s.vibration;
    this.reminders = s.reminders;
  }

  save() {
    try {
      localStorage.setItem(KEY, JSON.stringify({ theme: this.theme, sound: this.sound, vibration: this.vibration, reminders: this.reminders }));
    } catch {
      /* storage unavailable: keep in memory */
    }
  }
}

export const settings = new Settings();

export function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", theme);
}

/** Feedback for a moment: sound (if enabled) and a short vibration (if enabled and supported). */
export function feedback(id: SoundId) {
  if (settings.sound) playSound(id);
  // Browsers only allow vibration after the user has touched the page once.
  const touched = typeof navigator !== "undefined" && (navigator as Navigator & { userActivation?: { hasBeenActive: boolean } }).userActivation?.hasBeenActive !== false;
  if (settings.vibration && touched && "vibrate" in navigator) {
    navigator.vibrate(id === "step" ? 15 : id === "goal" || id === "overtake" ? [30, 40, 60] : 35);
  }
}
