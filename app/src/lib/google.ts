// "Mit Google anmelden": get an ID token from Google, the server checks it (/auth/google).
// - Android app: Google's sign-in sheet via the Credential Manager (GoogleSignIn.kt)
// - Browser (development, web): the Google button of Google Identity Services
// - Desktop app: Google in the browser, the code comes back on a loopback port and the server
//   swaps it (google_desktop.rs, needs the "Desktop" OAuth client: VITE_GOOGLE_DESKTOP_CLIENT_ID)
// The OAuth client IDs are public (docs/GOOGLE-LOGIN.md). Without them the button is not shown.
// The iOS app does not offer it (yet).
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { GoogleCode } from "$lib/generated";

export const GOOGLE_CLIENT_ID: string = import.meta.env.VITE_GOOGLE_CLIENT_ID ?? "";
export const GOOGLE_DESKTOP_CLIENT_ID: string = import.meta.env.VITE_GOOGLE_DESKTOP_CLIENT_ID ?? "";

const ua = () => (typeof navigator === "undefined" ? "" : navigator.userAgent);
const isAndroid = () => /Android/i.test(ua());
const isPhone = () => /Android|iPhone|iPad|iPod/i.test(ua());

/** Android app: our own button opens Google's sheet. */
export const googleNative = () => !!GOOGLE_CLIENT_ID && isTauri() && isAndroid();
/** Desktop app: our own button opens Google in the browser. */
export const googleDesktop = () => !!GOOGLE_DESKTOP_CLIENT_ID && isTauri() && !isPhone();
/** Browser: Google renders its own button. */
export const googleWeb = () => !!GOOGLE_CLIENT_ID && !isTauri();

/** The user closed Google's sheet: nothing to report. */
export class GoogleCancelled extends Error {}

export async function nativeIdToken(): Promise<string> {
  try {
    const res = await invoke<{ idToken: string }>("plugin:google-signin|sign_in", { serverClientId: GOOGLE_CLIENT_ID });
    return res.idToken;
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    if (msg.includes("cancelled")) throw new GoogleCancelled(msg);
    throw new Error(msg);
  }
}

/** Desktop app: sign in in the browser; resolves with the code for the server. */
export async function desktopCode(): Promise<GoogleCode> {
  try {
    return await invoke<GoogleCode>("google_desktop_sign_in", { clientId: GOOGLE_DESKTOP_CLIENT_ID });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    if (msg.includes("cancelled")) throw new GoogleCancelled(msg);
    throw new Error(msg);
  }
}

/** Stops waiting for the browser (desktop). */
export const cancelDesktop = () => invoke("google_desktop_cancel").catch(() => {});

type Gsi = {
  accounts: {
    id: {
      initialize(o: { client_id: string; callback: (r: { credential: string }) => void; ux_mode?: string }): void;
      renderButton(el: HTMLElement, o: Record<string, unknown>): void;
    };
  };
};

let script: Promise<Gsi> | null = null;

function loadGsi(): Promise<Gsi> {
  script ??= new Promise((resolve, reject) => {
    const w = window as unknown as { google?: Gsi };
    if (w.google?.accounts) return resolve(w.google);
    const s = document.createElement("script");
    s.src = "https://accounts.google.com/gsi/client";
    s.async = true;
    s.onload = () => (w.google ? resolve(w.google) : reject(new Error("Google ist gerade nicht erreichbar.")));
    s.onerror = () => {
      script = null;
      reject(new Error("Google ist gerade nicht erreichbar."));
    };
    document.head.appendChild(s);
  });
  return script;
}

/** Browser: Google's button in `el`; `onToken` gets the ID token after sign-in. */
export async function renderGoogleButton(el: HTMLElement, onToken: (idToken: string) => void, dark: boolean): Promise<void> {
  const gsi = await loadGsi();
  gsi.accounts.id.initialize({ client_id: GOOGLE_CLIENT_ID, callback: (r) => onToken(r.credential), ux_mode: "popup" });
  gsi.accounts.id.renderButton(el, {
    type: "standard",
    theme: dark ? "filled_black" : "outline",
    size: "large",
    shape: "pill",
    text: "continue_with",
    logo_alignment: "left",
    locale: "de",
    width: Math.min(400, el.clientWidth || 320),
  });
}
