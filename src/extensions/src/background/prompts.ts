// Save and update prompts (E-23, E-24).
//
// When a form with a password is submitted, the content script sends what was
// typed. The worker keeps it in memory for at most a minute: long enough for the
// next page to load and show the prompt, and dropped if the person ignores it.
// It is never written anywhere until the person says Save or Update.
//
// What happens with a capture, once unlocked:
// - the site is excluded (E-30, or "Never for this site"): nothing;
// - a login for the site has this username and this password: nothing;
// - a login for the site has this username (or is the only one, when the form
//   had no username) and another password: the update prompt;
// - otherwise: the save prompt.
// While locked nothing is compared or shown, and the capture is dropped.

import type { FrameRef } from "./autofill";
import type { ItemContentJson, Keyring, Summary, VaultInfo } from "./keyring";
import { isExcluded, suggestionsFor } from "./matcher";

export interface Captured {
  username: string;
  password: string;
}

export type PromptView =
  | { mode: "closed" }
  | {
      mode: "save";
      host: string;
      username: string;
      vaults: VaultInfo[];
    }
  | {
      mode: "update";
      host: string;
      title: string;
      username: string;
      vault: string;
    };

export type PromptAnswer =
  | { answer: "save"; vault_id?: string }
  | { answer: "update" }
  | { answer: "not-now" }
  | { answer: "never" };

export interface PromptContext {
  keyring(): Keyring | null;
  excludedSites(): Promise<string[]>;
  /** "Offer to save and update logins" (E-29). */
  enabled(): Promise<boolean>;
  exclude(host: string): Promise<void>;
  save(
    keyring: Keyring,
    vaultId: string | undefined,
    url: string,
    captured: Captured,
  ): Promise<void>;
  updatePassword(keyring: Keyring, itemId: string, password: string): Promise<void>;
  now(): number;
}

interface Pending {
  token: string;
  tabId: number;
  url: string;
  captured: Captured;
  at: number;
  /** Decided when the prompt is first asked for. */
  decision?: { mode: "save" } | { mode: "update"; item: Summary };
  shown: boolean;
}

/** How long a captured password is kept waiting for the prompt. */
export const CAPTURE_LIFETIME_MS = 60_000;

function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return "";
  }
}

const same = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();
const text = (value: unknown) => (typeof value === "string" ? value : "");

function randomToken(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export class Prompts {
  /** One capture per tab: a newer submit replaces an older one. */
  private readonly pending = new Map<number, Pending>();

  constructor(private readonly context: PromptContext) {}

  private sweep(): void {
    const now = this.context.now();
    for (const [tab, entry] of this.pending) {
      if (now - entry.at > CAPTURE_LIFETIME_MS) this.pending.delete(tab);
    }
  }

  private byToken(token: string): Pending | undefined {
    this.sweep();
    for (const entry of this.pending.values()) if (entry.token === token) return entry;
    return undefined;
  }

  /** A form was submitted in this frame with these values. */
  async capture(frame: FrameRef, captured: Captured): Promise<void> {
    if (!captured.password || !(await this.context.enabled())) return;
    if (isExcluded(frame.url, await this.context.excludedSites())) return;
    this.pending.set(frame.tabId, {
      token: randomToken(),
      tabId: frame.tabId,
      url: frame.url,
      captured: { username: captured.username.trim(), password: captured.password },
      at: this.context.now(),
      shown: false,
    });
  }

  /**
   * The top frame of a tab asks whether a prompt is waiting: after a page loads,
   * or a moment after a submit that did not leave the page. Returns a token once.
   */
  async check(frame: FrameRef): Promise<string | null> {
    this.sweep();
    if (frame.frameId !== 0) return null;
    const entry = this.pending.get(frame.tabId);
    if (!entry || entry.shown) return null;
    const keyring = this.context.keyring();
    if (!keyring) {
      this.pending.delete(frame.tabId); // Locked: nothing is compared or kept.
      return null;
    }
    const decision = await this.decide(keyring, entry);
    if (!decision) {
      this.pending.delete(frame.tabId);
      return null;
    }
    entry.decision = decision;
    entry.shown = true;
    return entry.token;
  }

  private async decide(keyring: Keyring, entry: Pending): Promise<Pending["decision"] | null> {
    const items = (await keyring.items()).filter((item) => !item.trashed);
    const logins = suggestionsFor(keyring, items, entry.url).map((hit) => hit.item);
    const { username, password } = entry.captured;
    const contents = new Map<string, ItemContentJson>();
    for (const login of logins) contents.set(login.id, await keyring.content(login.id));

    const sameUser = logins.filter((login) => {
      const saved = text(contents.get(login.id)?.data.username);
      return username ? same(saved, username) : true;
    });
    if (sameUser.some((login) => text(contents.get(login.id)?.data.password) === password)) {
      return null; // Already saved as it is.
    }
    // No username in the form: only an unambiguous login is updated.
    const target = username ? sameUser[0] : sameUser.length === 1 ? sameUser[0] : undefined;
    if (target) return { mode: "update", item: target };
    return { mode: "save" };
  }

  async view(token: string): Promise<PromptView> {
    const entry = this.byToken(token);
    const keyring = this.context.keyring();
    if (!entry?.decision || !keyring) return { mode: "closed" };
    const host = hostOf(entry.url);
    const vaults = await keyring.vaults();
    if (entry.decision.mode === "save") {
      return { mode: "save", host, username: entry.captured.username, vaults };
    }
    const item = entry.decision.item;
    return {
      mode: "update",
      host,
      title: item.title,
      username: item.subtitle || entry.captured.username,
      vault: vaults.find((vault) => vault.id === item.vault_id)?.name ?? "",
    };
  }

  /** The person answered. Returns the tab whose prompt should close. */
  async answer(token: string, answer: PromptAnswer): Promise<number | null> {
    const entry = this.byToken(token);
    if (!entry) return null;
    // Whatever the answer, the captured password leaves memory now.
    this.pending.delete(entry.tabId);
    const keyring = this.context.keyring();
    switch (answer.answer) {
      case "not-now":
        break;
      case "never":
        await this.context.exclude(hostOf(entry.url));
        break;
      case "save":
        if (keyring && entry.decision?.mode === "save") {
          await this.context.save(keyring, answer.vault_id, entry.url, entry.captured);
        }
        break;
      case "update":
        if (keyring && entry.decision?.mode === "update") {
          await this.context.updatePassword(
            keyring,
            entry.decision.item.id,
            entry.captured.password,
          );
        }
        break;
    }
    return entry.tabId;
  }

  /** The tab a prompt belongs to, for relaying its size. */
  tabOf(token: string): number | null {
    return this.byToken(token)?.tabId ?? null;
  }

  /** Lock and sign out drop every capture. */
  clear(): void {
    this.pending.clear();
  }
}
