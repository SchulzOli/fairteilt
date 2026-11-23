// Push registration (KONZEPT §8.2, stage 2): the Android app gets an FCM token from Firebase
// (see Push.kt) and announces it to the server, which then pushes to this phone. Elsewhere
// (desktop, browser) there is no token and nothing happens.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { session } from "$lib/session.svelte";

const KEY = "fairteilt.push";

interface Registered {
  token: string;
  user: string;
  at: number;
}

function read(): Registered | null {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "null");
  } catch {
    return null;
  }
}

function write(r: Registered | null) {
  try {
    if (r) localStorage.setItem(KEY, JSON.stringify(r));
    else localStorage.removeItem(KEY);
  } catch {
    /* register again next time */
  }
}

async function token(): Promise<string | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<string | null>("push_token");
  } catch {
    return null;
  }
}

/** Tells the server this phone's token (again once a day, or when token or account changed). */
export async function registerPush(attempt = 0): Promise<void> {
  const user = session.me?.user.id;
  if (!user || session.profile) return;
  const t = await token();
  if (!t) {
    // Firebase delivers the first token a moment after the app started.
    if (isTauri() && attempt < 4) setTimeout(() => void registerPush(attempt + 1), 4000 * (attempt + 1));
    return;
  }
  const last = read();
  if (last && last.token === t && last.user === user && Date.now() - last.at < 86_400_000) return;
  try {
    // Pushes are shown in the system: ask for permission (Android 13+) once.
    const n = await import("@tauri-apps/plugin-notification");
    if (!(await n.isPermissionGranted())) await n.requestPermission();
  } catch {
    /* no notification plugin here */
  }
  await session.request("POST", "/devices", { platform: "android", push_provider: "fcm", push_token: t });
  write({ token: t, user, at: Date.now() });
}

/** Before signing out: no more pushes for this account on this phone. */
export async function unregisterPush(): Promise<void> {
  const last = read();
  if (!last) return;
  await session.request("DELETE", "/devices", { push_token: last.token }).catch(() => {});
  write(null);
}
