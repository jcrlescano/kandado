// Passkeys (E-25, E-26): `navigator.credentials.create` and `.get` answered
// from the vault.
//
// The page's call reaches the worker through the main-world shim and the
// content script of the top frame. The worker decides at once whether Kandado
// takes part; if not, the shim calls the browser's own WebAuthn and nothing
// else happens. If it does, the request waits here under a token, the prompt
// page shows it, and the person's answer comes back. The origin is always the
// one the browser reported for the frame, never one the page supplied. Every
// key stays in the WebAssembly core: it makes the passkey and signs; this file
// only routes.
//
// The worker hands Kandado's part to the browser (the "native" outcome) when:
// the frame is not the top frame; passkeys are off (E-29) or the site is
// excluded (E-30); the account is signed out; the site accepts no ES256; or,
// for a sign-in, no passkey in the vault fits. A conditional request (the
// browser's own autofill UI) is never intercepted.

import type { Keyring, Summary, VaultInfo } from "./keyring";
import { isExcluded } from "./matcher";

/** What the shim sends for `create`. Binary values are base64url. */
export interface PageCreate {
  rp_id?: string;
  rp_name?: string;
  user_id: string;
  user_name?: string;
  user_display_name?: string;
  challenge: string;
  algorithms?: number[];
  exclude_credentials?: string[];
}

/** What the shim sends for `get`. */
export interface PageGet {
  rp_id?: string;
  challenge: string;
  allow_credentials?: string[];
}

export type PasskeyRequest =
  | { op: "create"; request: PageCreate }
  | { op: "get"; request: PageGet };

/** The immediate answer to the content script. */
export type PasskeyStart = { native: true } | { native: false; token: string };

/** The final answer, sent to the top frame and on to the shim. */
export type PasskeyOutcome =
  | { kind: "native" }
  | { kind: "error"; name: "NotAllowedError" | "InvalidStateError" | "AbortError"; message: string }
  | { kind: "created"; response: Record<string, unknown> }
  | { kind: "asserted"; response: Record<string, unknown> };

export type PasskeyView =
  | { mode: "closed" }
  /** Kandado is locked: unlock from the toolbar, or use another device. */
  | { mode: "passkey-locked"; host: string; op: "create" | "get" }
  | {
      mode: "passkey-create";
      host: string;
      username: string;
      vaults: VaultInfo[];
      /** The site already has one of these passkeys: it will say so (InvalidStateError). */
      already: boolean;
    }
  | {
      mode: "passkey-get";
      host: string;
      passkeys: { id: string; username: string; vault: VaultInfo | null }[];
    };

export type PasskeyAnswer =
  | { answer: "native" }
  | { answer: "cancel" }
  | { answer: "save"; vault_id?: string }
  | { answer: "use"; item_id: string };

export interface PasskeyFrame {
  tabId: number;
  frameId: number;
  url: string;
}

export interface PasskeyContext {
  /** The unlocked vault, or null while locked. */
  keyring(): Keyring | null;
  signedIn(): boolean;
  /** "Use Kandado for passkeys" (E-29). */
  enabled(): Promise<boolean>;
  excludedSites(): Promise<string[]>;
  /** Makes the passkey in the core and stores it as a new item; the response JSON. */
  create(keyring: Keyring, vaultId: string, requestJson: string): Promise<string>;
  /** Signs in with a stored passkey in the core; the response JSON. */
  assert(keyring: Keyring, itemId: string, requestJson: string): Promise<string>;
  /** Sends the outcome to the top frame of the tab. */
  deliver(tabId: number, token: string, outcome: PasskeyOutcome): Promise<void>;
  now(): number;
}

interface Pending {
  token: string;
  tabId: number;
  /** As the browser reported it for the frame. */
  origin: string;
  host: string;
  call: PasskeyRequest;
  at: number;
}

/** A waiting request is dropped after this long, as browsers time out WebAuthn calls. */
export const PASSKEY_LIFETIME_MS = 5 * 60_000;

const ES256 = -7;

function randomToken(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function originOf(url: string): { origin: string; host: string } | null {
  try {
    const parsed = new URL(url);
    if (parsed.protocol !== "https:" && parsed.protocol !== "http:") return null;
    return { origin: parsed.origin, host: parsed.hostname };
  } catch {
    return null;
  }
}

/** The relying party a request is for: `rp.id`, or the page's host. */
function rpOf(call: PasskeyRequest, host: string): string {
  return (call.request.rp_id ?? "").trim().toLowerCase().replace(/\.$/, "") || host;
}

const text = (value: unknown, max = 512) =>
  typeof value === "string" ? value.slice(0, max) : undefined;
const list = (value: unknown) =>
  Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string").slice(0, 64)
    : [];

/** Only the fields the core reads, with the types it expects: the page wrote these. */
export function cleanRequest(call: PasskeyRequest): PasskeyRequest | null {
  const r = call.request as unknown as Record<string, unknown>;
  const challenge = text(r.challenge, 1024);
  if (!challenge) return null;
  if (call.op === "get") {
    return {
      op: "get",
      request: {
        rp_id: text(r.rp_id, 253),
        challenge,
        allow_credentials: list(r.allow_credentials),
      },
    };
  }
  const user_id = text(r.user_id, 128);
  if (!user_id) return null;
  return {
    op: "create",
    request: {
      rp_id: text(r.rp_id, 253),
      rp_name: text(r.rp_name, 256),
      user_id,
      user_name: text(r.user_name, 256),
      user_display_name: text(r.user_display_name, 256),
      challenge,
      algorithms: Array.isArray(r.algorithms)
        ? r.algorithms.filter((alg): alg is number => Number.isInteger(alg)).slice(0, 32)
        : [],
      exclude_credentials: list(r.exclude_credentials),
    },
  };
}

export class Passkeys {
  private readonly pending = new Map<string, Pending>();

  constructor(private readonly context: PasskeyContext) {}

  private sweep(): void {
    const now = this.context.now();
    for (const [token, entry] of this.pending) {
      if (now - entry.at > PASSKEY_LIFETIME_MS) this.pending.delete(token);
    }
  }

  /** The vault's passkeys for a relying party, optionally only those the site allows. */
  private async passkeysFor(rpId: string, allow: string[]): Promise<Summary[]> {
    const keyring = this.context.keyring();
    if (!keyring) return [];
    return (await keyring.items()).filter(
      (item) =>
        item.kind === "passkey" &&
        !item.trashed &&
        item.passkey?.rp_id === rpId &&
        (allow.length === 0 || allow.includes(item.passkey.credential_id)),
    );
  }

  /** A page asked. Decides whether Kandado answers, and if so keeps the request. */
  async start(frame: PasskeyFrame, raw: PasskeyRequest): Promise<PasskeyStart> {
    this.sweep();
    const call = cleanRequest(raw);
    const where = originOf(frame.url);
    if (!call || !where || frame.frameId !== 0) return { native: true };
    if (!this.context.signedIn() || !(await this.context.enabled())) return { native: true };
    if (isExcluded(frame.url, await this.context.excludedSites())) return { native: true };
    if (call.op === "create") {
      const algorithms = call.request.algorithms ?? [];
      if (algorithms.length > 0 && !algorithms.includes(ES256)) return { native: true };
    } else if (
      this.context.keyring() &&
      (await this.passkeysFor(rpOf(call, where.host), call.request.allow_credentials ?? []))
        .length === 0
    ) {
      // Nothing of ours fits: do not stand between the person and their other passkeys.
      return { native: true };
    }
    // One request per tab: a newer one replaces an older one, which the page abandoned.
    for (const [token, entry] of this.pending) {
      if (entry.tabId === frame.tabId) {
        this.pending.delete(token);
        await this.context
          .deliver(entry.tabId, token, {
            kind: "error",
            name: "AbortError",
            message: "Replaced by a newer request.",
          })
          .catch(() => {});
      }
    }
    const token = randomToken();
    this.pending.set(token, {
      token,
      tabId: frame.tabId,
      origin: where.origin,
      host: where.host,
      call,
      at: this.context.now(),
    });
    return { native: false, token };
  }

  tabOf(token: string): number | null {
    this.sweep();
    return this.pending.get(token)?.tabId ?? null;
  }

  /** What the prompt shows. */
  async view(token: string): Promise<PasskeyView> {
    this.sweep();
    const entry = this.pending.get(token);
    if (!entry) return { mode: "closed" };
    const rpId = rpOf(entry.call, entry.host);
    const keyring = this.context.keyring();
    if (!keyring) return { mode: "passkey-locked", host: rpId, op: entry.call.op };
    const vaults = await keyring.vaults();
    if (entry.call.op === "create") {
      const exclude = entry.call.request.exclude_credentials ?? [];
      const already = exclude.length > 0 && (await this.passkeysFor(rpId, exclude)).length > 0;
      return {
        mode: "passkey-create",
        host: rpId,
        username: entry.call.request.user_name || entry.call.request.user_display_name || "",
        vaults,
        already,
      };
    }
    const found = await this.passkeysFor(rpId, entry.call.request.allow_credentials ?? []);
    return {
      mode: "passkey-get",
      host: rpId,
      passkeys: found.map((item) => ({
        id: item.id,
        username: item.subtitle || item.title,
        vault: vaults.find((vault) => vault.id === item.vault_id) ?? null,
      })),
    };
  }

  /** The page gave up (its AbortSignal fired). */
  async abort(frame: PasskeyFrame, token: string): Promise<void> {
    const entry = this.pending.get(token);
    if (!entry || entry.tabId !== frame.tabId || frame.frameId !== 0) return;
    this.pending.delete(token);
    await this.context
      .deliver(entry.tabId, token, { kind: "error", name: "AbortError", message: "Aborted." })
      .catch(() => {});
  }

  /** The person answered in the prompt. Returns the tab, so the prompt can close. */
  async answer(token: string, answer: PasskeyAnswer): Promise<number | null> {
    this.sweep();
    const entry = this.pending.get(token);
    if (!entry) return null;
    this.pending.delete(token);
    const outcome = await this.outcome(entry, answer);
    await this.context.deliver(entry.tabId, token, outcome).catch(() => {});
    return entry.tabId;
  }

  private async outcome(entry: Pending, answer: PasskeyAnswer): Promise<PasskeyOutcome> {
    const refused: PasskeyOutcome = {
      kind: "error",
      name: "NotAllowedError",
      message: "The operation either timed out or was not allowed.",
    };
    if (answer.answer === "native") return { kind: "native" };
    const keyring = this.context.keyring();
    if (answer.answer === "cancel" || !keyring) {
      if (entry.call.op === "create" && keyring) {
        const exclude = entry.call.request.exclude_credentials ?? [];
        const rpId = rpOf(entry.call, entry.host);
        if (exclude.length > 0 && (await this.passkeysFor(rpId, exclude)).length > 0) {
          return {
            kind: "error",
            name: "InvalidStateError",
            message: "The authenticator already has a credential for this account.",
          };
        }
      }
      return refused;
    }
    try {
      if (entry.call.op === "create" && answer.answer === "save") {
        const vaults = await keyring.vaults();
        const vault = vaults.find((v) => v.id === answer.vault_id) ?? vaults[0];
        if (!vault) return refused;
        const { exclude_credentials: _, ...request } = entry.call.request;
        const response = await this.context.create(
          keyring,
          vault.id,
          JSON.stringify({ ...request, origin: entry.origin }),
        );
        return { kind: "created", response: JSON.parse(response) };
      }
      if (entry.call.op === "get" && answer.answer === "use") {
        const rpId = rpOf(entry.call, entry.host);
        const allowed = await this.passkeysFor(rpId, entry.call.request.allow_credentials ?? []);
        if (!allowed.some((item) => item.id === answer.item_id)) return refused;
        const { allow_credentials: _, ...request } = entry.call.request;
        const response = await this.context.assert(
          keyring,
          answer.item_id,
          JSON.stringify({ ...request, origin: entry.origin }),
        );
        return { kind: "asserted", response: JSON.parse(response) };
      }
    } catch {
      return refused;
    }
    return refused;
  }

  clear(): void {
    this.pending.clear();
  }
}
