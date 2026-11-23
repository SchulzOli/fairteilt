// Session and API client for the Fairteilt server (M1).
//
// - Access token: in memory only. Refresh token: secure storage (OS keychain / app-private
//   file on Android, see src-tauri/src/secure_store.rs); in a plain browser (dev preview)
//   it falls back to localStorage.
// - Every request retries once after renewing the access token.
// - Family tablet: a child can switch to their profile with a PIN. The device's own session
//   is kept aside and restored on "Zurück" or after 2 minutes without interaction.
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./api";
import type {
  AcceptInviteResponse,
  CreatedMember,
  CreateHouseholdResponse,
  InviteResponse,
  JoinResponse,
  MeResponse,
  MemberDto,
  MembershipDto,
  TokenResponse,
} from "./generated";

const STORE_KEY = "session";
const SERVER_KEY = "fairteilt.serverUrl";
const ACTIVE_KEY = "fairteilt.activeHousehold";
const PROFILE_IDLE_MS = 2 * 60 * 1000;
/** Last known account (not secret): lets the app start without a connection (M7). */
const ME_KEY = "fairteilt.me";
const RETRY_MS = 15_000;
/** Release builds talk to the family server; `pnpm dev` and VITE_SERVER_URL override it. */
const DEFAULT_SERVER: string = import.meta.env.VITE_SERVER_URL ?? (import.meta.env.DEV ? "http://localhost:8080" : "https://fairteilt.oli-tech.eu");

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

// ---------- secure storage ----------
async function storeGet(): Promise<string | null> {
  try {
    if (isTauri()) return (await invoke<string | null>("secure_get", { key: STORE_KEY })) ?? null;
    return localStorage.getItem(`fairteilt.dev.${STORE_KEY}`);
  } catch {
    return null;
  }
}
async function storeSet(value: string): Promise<void> {
  try {
    if (isTauri()) await invoke("secure_set", { key: STORE_KEY, value });
    else localStorage.setItem(`fairteilt.dev.${STORE_KEY}`, value);
  } catch {
    /* keep the session in memory only */
  }
}
async function storeDelete(): Promise<void> {
  try {
    if (isTauri()) await invoke("secure_delete", { key: STORE_KEY });
    else localStorage.removeItem(`fairteilt.dev.${STORE_KEY}`);
  } catch {
    /* nothing stored */
  }
}

function readLocal(key: string, fallback: string): string {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
}
function writeLocal(key: string, value: string | null) {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    /* storage unavailable */
  }
}

function deviceName(): string {
  const ua = typeof navigator === "undefined" ? "" : navigator.userAgent;
  if (/Android/i.test(ua)) return "Android-Gerät";
  if (/iPhone|iPad/i.test(ua)) return "iPhone/iPad";
  if (/Windows/i.test(ua)) return "Windows-PC";
  if (/Mac OS/i.test(ua)) return "Mac";
  return "Fairteilt-App";
}

export const isPhone = () => typeof navigator !== "undefined" && /Android|iPhone|iPad/i.test(navigator.userAgent);

interface Tokens {
  access: string;
  refresh: string;
}

class Session {
  status = $state<"loading" | "signed_out" | "ready">("loading");
  /** No connection to the server right now (the app shows what it knows and queues changes). */
  offline = $state(false);
  serverUrl = $state(readLocal(SERVER_KEY, DEFAULT_SERVER));
  me = $state<MeResponse | null>(null);
  activeHouseholdId = $state<string | null>(readLocal(ACTIVE_KEY, "") || null);
  members = $state<MemberDto[]>([]);
  /** Set while a child uses their profile on the family tablet. */
  profile = $state<{ name: string; deviceUser: string } | null>(null);

  private tokens: Tokens | null = null;
  private deviceTokens: Tokens | null = null;
  private refreshing: Promise<boolean> | null = null;
  private idleTimer: ReturnType<typeof setTimeout> | null = null;
  private lastRenewOffline = false;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  /** Called when the connection comes back (the stores reload, the queue is sent). */
  onlineAgain: (() => void) | null = null;

  get membership(): MembershipDto | null {
    const ms = this.me?.memberships ?? [];
    return ms.find((m) => m.household_id === this.activeHouseholdId) ?? ms[0] ?? null;
  }
  get isAdmin(): boolean {
    return this.membership?.role === "admin";
  }

  setServerUrl(url: string) {
    this.serverUrl = url.trim().replace(/\/+$/, "");
    writeLocal(SERVER_KEY, this.serverUrl);
  }

  // ---------- lifecycle ----------
  async init() {
    const raw = await storeGet();
    if (raw) {
      try {
        const saved = JSON.parse(raw) as { serverUrl?: string; refresh?: string };
        if (saved.serverUrl) this.serverUrl = saved.serverUrl;
        if (saved.refresh) {
          this.tokens = { access: "", refresh: saved.refresh };
          if (await this.renew()) {
            await this.loadMe();
            this.status = "ready";
            return;
          }
          // No connection (not a rejected token): start with what we know and try again later.
          const cached = readLocal(ME_KEY, "");
          if (this.lastRenewOffline && cached) {
            this.me = JSON.parse(cached) as MeResponse;
            this.status = "ready";
            this.goneOffline();
            return;
          }
        }
      } catch {
        /* corrupt entry: start fresh */
      }
    }
    this.tokens = null;
    this.status = "signed_out";
  }

  private async persist() {
    if (this.tokens && !this.profile) {
      await storeSet(JSON.stringify({ serverUrl: this.serverUrl, refresh: this.tokens.refresh }));
    }
  }

  private async adopt(res: TokenResponse) {
    this.tokens = { access: res.access_token, refresh: res.refresh_token };
    await this.persist();
    await this.loadMe();
    this.status = "ready";
  }

  /** Renews the access token with the refresh token (rotating). */
  private renew(): Promise<boolean> {
    if (this.refreshing) return this.refreshing;
    this.refreshing = (async () => {
      if (!this.tokens) return false;
      try {
        this.lastRenewOffline = false;
        const res = await this.raw<TokenResponse>("POST", "/auth/refresh", { refresh_token: this.tokens.refresh }, false);
        this.tokens = { access: res.access_token, refresh: res.refresh_token };
        await this.persist();
        return true;
      } catch (e) {
        this.lastRenewOffline = e instanceof ApiError && e.status === 0;
        return false;
      } finally {
        this.refreshing = null;
      }
    })();
    return this.refreshing;
  }

  // ---------- HTTP ----------
  private async raw<T>(method: string, path: string, body?: unknown, auth = true, bearer?: string, extra?: Record<string, string>): Promise<T> {
    let res: Response;
    try {
      res = await fetch(`${this.serverUrl}/api/v1${path}`, {
        method,
        headers: {
          ...(body !== undefined ? { "content-type": "application/json" } : {}),
          ...(bearer ? { authorization: `Bearer ${bearer}` } : auth && this.tokens?.access ? { authorization: `Bearer ${this.tokens.access}` } : {}),
          ...extra,
        },
        body: body !== undefined ? JSON.stringify(body) : undefined,
      });
    } catch {
      this.goneOffline();
      throw new ApiError(0, "Der Server ist nicht erreichbar. Prüfe die Adresse und deine Verbindung.");
    }
    if (this.offline) this.backOnline();
    if (res.status === 204) return undefined as T;
    const data = await res.json().catch(() => null);
    if (!res.ok) throw new ApiError(res.status, data?.error?.message ?? `Fehler ${res.status}`);
    return data as T;
  }

  async request<T>(method: string, path: string, body?: unknown, headers?: Record<string, string>): Promise<T> {
    this.touch();
    try {
      // Offline start: the access token is renewed with the first request that gets through.
      if (this.tokens && !this.tokens.access && !(await this.renew())) {
        throw this.lastRenewOffline
          ? new ApiError(0, "Gerade keine Verbindung zum Server. Das klappt wieder, sobald du online bist.")
          : new ApiError(401, "Bitte neu anmelden.");
      }
      return await this.raw<T>(method, path, body, true, undefined, headers);
    } catch (e) {
      if (e instanceof ApiError && e.status === 401 && (await this.renew())) {
        return this.raw<T>(method, path, body, true, undefined, headers);
      }
      // Only a rejected token signs out; a missing connection does not.
      if (e instanceof ApiError && e.status === 401 && !this.lastRenewOffline) await this.forget();
      throw e;
    }
  }

  private goneOffline() {
    if (!this.offline) this.offline = true;
    if (this.retryTimer) return;
    this.retryTimer = setTimeout(() => {
      this.retryTimer = null;
      void this.raw("GET", "/meta", undefined, false).catch(() => {});
    }, RETRY_MS);
  }

  private backOnline() {
    this.offline = false;
    if (this.retryTimer) clearTimeout(this.retryTimer);
    this.retryTimer = null;
    this.onlineAgain?.();
  }

  /** A valid access token for the live WebSocket (renewed when it expires within a minute). */
  async liveToken(): Promise<string | null> {
    if (!this.tokens) return null;
    if (tokenExpiry(this.tokens.access) - Date.now() / 1000 < 60 && !(await this.renew())) return null;
    return this.tokens?.access || null;
  }

  // ---------- account ----------
  async register(email: string, password: string, displayName: string) {
    await this.adopt(
      await this.raw<TokenResponse>("POST", "/auth/register", { email, password, display_name: displayName, device_name: deviceName() }, false),
    );
  }

  async login(email: string, password: string) {
    await this.adopt(await this.raw<TokenResponse>("POST", "/auth/login", { email, password, device_name: deviceName() }, false));
  }

  /** Child without e-mail joins on their own device with a child invite code. */
  async joinWithoutAccount(code: string, displayName: string, animal: string, color: string) {
    const res = await this.raw<JoinResponse>(
      "POST",
      "/invites/join",
      { code, display_name: displayName, animal, color, device_name: deviceName() },
      false,
    );
    this.selectHousehold(res.household_id);
    await this.adopt(res);
  }

  async loadMe() {
    this.me = await this.request<MeResponse>("GET", "/me");
    writeLocal(ME_KEY, JSON.stringify(this.me));
    if (!this.me.memberships.some((m) => m.household_id === this.activeHouseholdId)) {
      this.selectHousehold(this.me.memberships[0]?.household_id ?? null);
    }
  }

  selectHousehold(id: string | null) {
    this.activeHouseholdId = id;
    writeLocal(ACTIVE_KEY, id);
  }

  private async forget() {
    this.tokens = null;
    this.deviceTokens = null;
    this.profile = null;
    this.me = null;
    this.members = [];
    this.status = "signed_out";
    writeLocal(ME_KEY, null);
    // The offline copy of the household stays on the device only while someone is signed in.
    try {
      for (const key of Object.keys(localStorage)) {
        if (key.startsWith("fairteilt.cache.")) localStorage.removeItem(key);
      }
    } catch {
      /* storage unavailable */
    }
    await storeDelete();
  }

  async signOut() {
    if (this.profile) await this.returnToDevice();
    try {
      await this.raw("POST", "/auth/logout");
    } catch {
      /* already invalid */
    }
    await this.forget();
  }

  // ---------- household ----------
  async createHousehold(name: string, animal: string, color: string) {
    const res = await this.request<CreateHouseholdResponse>("POST", "/households", { name, animal, color });
    this.selectHousehold(res.household_id);
    await this.loadMe();
  }

  async acceptInvite(code: string, animal: string, color: string) {
    const res = await this.request<AcceptInviteResponse>("POST", "/invites/accept", { code, animal, color });
    this.selectHousehold(res.household_id);
    await this.loadMe();
  }

  async loadMembers() {
    const hid = this.membership?.household_id;
    if (!hid) return;
    this.members = await this.request<MemberDto[]>("GET", `/households/${hid}/members`);
  }

  async createInvite(role: "admin" | "member" | "child") {
    return this.request<InviteResponse>("POST", `/households/${this.membership!.household_id}/invites`, { role });
  }

  async createChild(displayName: string, animal: string, color: string, pin: string | null) {
    const res = await this.request<CreatedMember>("POST", `/households/${this.membership!.household_id}/children`, {
      display_name: displayName,
      animal,
      color,
      pin,
    });
    await this.loadMembers();
    return res;
  }

  async updateMember(membershipId: string, patch: { color?: string; animal?: string; role?: string; weight?: number }) {
    await this.request("PATCH", `/households/${this.membership!.household_id}/members/${membershipId}`, patch);
    await Promise.all([this.loadMembers(), this.loadMe()]);
  }

  async setPin(membershipId: string, pin: string | null) {
    await this.request("PUT", `/households/${this.membership!.household_id}/members/${membershipId}/pin`, { pin });
    await this.loadMembers();
  }

  // ---------- family tablet ----------
  /**
   * Wall display: act once as another member (PIN), e.g. check off their task, then end that
   * short session again. The tablet stays signed in as before.
   */
  async asMember<T>(membershipId: string, pin: string, fn: (call: <R>(method: string, path: string, body?: unknown) => Promise<R>) => Promise<T>): Promise<T> {
    const res = await this.request<TokenResponse>("POST", "/auth/pin", { membership_id: membershipId, pin, device_name: "Wandanzeige" });
    const call = <R>(method: string, path: string, body?: unknown) => this.raw<R>(method, path, body, true, res.access_token);
    try {
      return await fn(call);
    } finally {
      await call("POST", "/auth/logout").catch(() => {});
    }
  }

  async switchProfile(membershipId: string, pin: string) {
    const deviceUser = this.me?.user.display_name ?? "";
    const res = await this.request<TokenResponse>("POST", "/auth/pin", { membership_id: membershipId, pin, device_name: "Familien-Tablet" });
    if (!this.profile) this.deviceTokens = this.tokens;
    this.tokens = { access: res.access_token, refresh: res.refresh_token };
    this.profile = { name: res.user.display_name, deviceUser };
    await this.loadMe();
    this.touch();
  }

  async returnToDevice() {
    if (!this.profile || !this.deviceTokens) return;
    try {
      await this.raw("POST", "/auth/logout");
    } catch {
      /* the child session ends either way */
    }
    this.tokens = this.deviceTokens;
    this.deviceTokens = null;
    this.profile = null;
    if (this.idleTimer) clearTimeout(this.idleTimer);
    await this.loadMe();
  }

  /** Any interaction keeps a child profile on the tablet alive for another 2 minutes. */
  touch() {
    if (!this.profile) return;
    if (this.idleTimer) clearTimeout(this.idleTimer);
    this.idleTimer = setTimeout(() => void this.returnToDevice(), PROFILE_IDLE_MS);
  }
}

export const session = new Session();

/** `exp` of a JWT (seconds), 0 when unreadable. */
function tokenExpiry(jwt: string): number {
  try {
    const part = jwt.split(".")[1].replace(/-/g, "+").replace(/_/g, "/");
    return Number(JSON.parse(atob(part)).exp) || 0;
  } catch {
    return 0;
  }
}

/** Reads "FAIRTEILT:K7QF-M2XA" from a scanned QR code, or returns typed input unchanged. */
export function parseInviteCode(input: string): string {
  const m = /FAIRTEILT:([A-Z0-9-]+)/i.exec(input);
  return (m ? m[1] : input).trim();
}
