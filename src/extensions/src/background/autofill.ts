// In-page autofill (E-16 to E-22): what the dropdown shows for a focused field,
// and the fill itself.
//
// The content script never sees a list or a secret it did not ask to fill. It
// says "this frame has a field of this kind focused"; the worker remembers the
// frame (its tab, frame id and real address, taken from the browser, never from
// the page) under a random token. The dropdown, an extension page inside the
// page, asks the worker what to show for that token, and the person's pick comes
// back from it. Only then does the worker decrypt the item and send the values to
// that one frame.
//
// Rules (docs/architecture/extensions.md):
// - fill only after a pick, never on page load;
// - a login fills only into a frame whose own address matches it, so a
//   cross-origin iframe gets only logins saved for that iframe's site;
// - a login saved for HTTPS fills into an HTTP page only after a second,
//   explicit confirmation;
// - cards and identities fill only into the top page or a frame on the same
//   origin as the top page;
// - excluded sites get nothing.

import type { FormKind } from "../content/detect";
import type { FillValues } from "../content/fill";
import type { ItemContentJson, Keyring, Summary } from "./keyring";
import { isExcluded, suggestionsFor } from "./matcher";

export interface FrameRef {
  tabId: number;
  frameId: number;
  /** The frame's address, from the browser (`sender.url`). */
  url: string;
}

export interface DropdownRow {
  id: string;
  kind: Summary["kind"];
  title: string;
  subtitle: string;
  favourite: boolean;
  /** HTTP page, login saved for HTTPS: picking asks once more. */
  insecure?: boolean;
}

export type DropdownView =
  | { mode: "closed" }
  | { mode: "locked"; host: string }
  | { mode: "logins"; host: string; rows: DropdownRow[] }
  | { mode: "new-password"; host: string; password: string; bits: number }
  | { mode: "cards"; host: string; rows: DropdownRow[] }
  | { mode: "identities"; host: string; rows: DropdownRow[] }
  | {
      mode: "totp";
      host: string;
      codes: { id: string; title: string; code: string; remaining: number }[];
    };

export type DropdownChoice =
  | { item: string; confirmInsecure?: boolean }
  | { generated: true }
  | { search: true };

export type PickResult = "filled" | "confirm-insecure" | "opened-popup" | "refused";

/** Filling from the popup: done, no form for it on the page, or not allowed there. */
export type TabFillResult = "filled" | "no-form" | "refused";

export interface AutofillPlatform {
  /** Sends a message to one frame's content script. */
  sendToFrame(frame: FrameRef, message: unknown): Promise<unknown>;
  /** Every frame of a tab, top first (`webNavigation.getAllFrames`). */
  frames(tabId: number): Promise<{ frameId: number; url: string }[]>;
  /** The address of a tab's top page, if the extension may read it. */
  tabUrl(tabId: number): Promise<string | undefined>;
  openPopup(): Promise<void>;
  now(): number;
}

export interface AutofillContext {
  /** The unlocked keyring, or null when locked or signed out. */
  keyring(): Keyring | null;
  signedIn(): boolean;
  excludedSites(): Promise<string[]>;
  /** The settings allow this dropdown (E-29). */
  enabled(kind: FormKind): Promise<boolean>;
  generate(): Promise<{ password: string; bits: number }>;
}

interface Pending {
  frame: FrameRef;
  kind: FormKind;
  opened: number;
  /** The password offered in the new-password dropdown, kept until it is used or closed. */
  generated?: string;
}

/** How long a dropdown's token stays good. */
const TOKEN_LIFETIME_MS = 5 * 60_000;

function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return "";
  }
}

function originOf(url: string | undefined): string | null {
  if (!url) return null;
  try {
    return new URL(url).origin;
  } catch {
    return null;
  }
}

function randomToken(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function row(item: Summary, insecure?: boolean): DropdownRow {
  return {
    id: item.id,
    kind: item.kind,
    title: item.title,
    subtitle: item.subtitle,
    favourite: item.favourite,
    ...(insecure ? { insecure } : {}),
  };
}

const text = (value: unknown) => (typeof value === "string" ? value : "");

export class Autofill {
  private readonly pending = new Map<string, Pending>();

  constructor(
    private readonly context: AutofillContext,
    private readonly platform: AutofillPlatform,
  ) {}

  private live(token: string): Pending | undefined {
    const entry = this.pending.get(token);
    if (!entry) return undefined;
    if (this.platform.now() - entry.opened > TOKEN_LIFETIME_MS) {
      this.pending.delete(token);
      return undefined;
    }
    return entry;
  }

  /**
   * A field was focused. Returns a token for the dropdown, or null when there is
   * nothing to offer (signed out, excluded site, no matching items).
   */
  async open(frame: FrameRef, kind: FormKind): Promise<string | null> {
    if (!this.context.signedIn()) return null;
    if (isExcluded(frame.url, await this.context.excludedSites())) return null;
    if (!(await this.context.enabled(kind))) return null;
    // One dropdown per frame: a new focus replaces the old one.
    for (const [token, entry] of this.pending) {
      if (entry.frame.tabId === frame.tabId && entry.frame.frameId === frame.frameId) {
        this.pending.delete(token);
      }
    }
    const entry: Pending = { frame, kind, opened: this.platform.now() };
    const view = await this.viewFor(entry);
    if (view.mode === "closed") return null;
    const token = randomToken();
    this.pending.set(token, entry);
    return token;
  }

  close(token: string): void {
    this.pending.delete(token);
  }

  /** The frame a token belongs to, for relaying keys and sizes to it. */
  frameOf(token: string): FrameRef | undefined {
    return this.live(token)?.frame;
  }

  /** What the dropdown shows. Titles and usernames only, no secrets (except the offered password and codes). */
  async view(token: string): Promise<DropdownView> {
    const entry = this.live(token);
    return entry ? this.viewFor(entry) : { mode: "closed" };
  }

  private async sameOriginAsTop(frame: FrameRef): Promise<boolean> {
    if (frame.frameId === 0) return true;
    const top = originOf(await this.platform.tabUrl(frame.tabId));
    return top !== null && top === originOf(frame.url);
  }

  private async viewFor(entry: Pending): Promise<DropdownView> {
    const host = hostOf(entry.frame.url);
    const keyring = this.context.keyring();
    if (!keyring) return { mode: "locked", host };
    const items = (await keyring.items()).filter((item) => !item.trashed);

    switch (entry.kind) {
      case "login": {
        const hits = suggestionsFor(keyring, items, entry.frame.url);
        if (hits.length === 0) return { mode: "closed" };
        return { mode: "logins", host, rows: hits.map((hit) => row(hit.item, hit.insecure)) };
      }
      case "new-password": {
        if (!entry.generated) {
          const generated = await this.context.generate();
          entry.generated = generated.password;
          return { mode: "new-password", host, ...generated };
        }
        return { mode: "new-password", host, password: entry.generated, bits: 0 };
      }
      case "card":
      case "identity": {
        if (!(await this.sameOriginAsTop(entry.frame))) return { mode: "closed" };
        const kind = entry.kind === "card" ? "card" : "identity";
        const rows = items.filter((item) => item.kind === kind).map((item) => row(item));
        // An identity is told apart by its address (E-21), shown only in this list.
        if (kind === "identity") {
          for (const identity of rows) {
            const content = await keyring.content(identity.id);
            const address = (content.data.address ?? {}) as Record<string, unknown>;
            const place = [text(address.line1), text(address.city)].filter(Boolean).join(", ");
            identity.subtitle = place || text(content.data.email) || identity.subtitle;
          }
        }
        if (rows.length === 0) return { mode: "closed" };
        return { mode: entry.kind === "card" ? "cards" : "identities", host, rows };
      }
      case "totp": {
        const hits = suggestionsFor(keyring, items, entry.frame.url).filter(
          (hit) => hit.item.has_totp && !hit.insecure,
        );
        if (hits.length === 0) return { mode: "closed" };
        const now = this.platform.now();
        const remaining = 30 - (Math.floor(now / 1000) % 30);
        const codes = [];
        for (const hit of hits) {
          codes.push({
            id: hit.item.id,
            title: hit.item.title,
            code: await keyring.totp(hit.item.id, now),
            remaining,
          });
        }
        return { mode: "totp", host, codes };
      }
    }
  }

  /** The person picked something in the dropdown. */
  async pick(token: string, choice: DropdownChoice): Promise<PickResult> {
    const entry = this.live(token);
    if (!entry) return "refused";
    if ("search" in choice) {
      await this.platform.openPopup();
      return "opened-popup";
    }
    const keyring = this.context.keyring();
    if (!keyring) {
      await this.platform.openPopup();
      return "opened-popup";
    }
    const values = await this.valuesFor(entry, keyring, choice);
    if (values === "confirm-insecure" || values === "refused") return values;
    this.pending.delete(token);
    await this.platform.sendToFrame(entry.frame, { type: "kandado-fill", values });
    return "filled";
  }

  /**
   * Fills an item picked in the popup into the tab. The same rules as the
   * dropdown: a login goes only into frames whose own address matches it (never
   * over plain HTTP from here; the popup opens the item instead), cards and
   * identities only into the top page or frames on its origin.
   */
  async fillTab(tabId: number, itemId: string): Promise<TabFillResult> {
    const keyring = this.context.keyring();
    if (!keyring) return "refused";
    const item = (await keyring.items()).find(
      (candidate) => candidate.id === itemId && !candidate.trashed,
    );
    if (!item || (item.kind !== "login" && item.kind !== "card" && item.kind !== "identity")) {
      return "refused";
    }
    const frames = (await this.platform.frames(tabId)).sort((a, b) => a.frameId - b.frameId);
    const top = originOf(frames.find((frame) => frame.frameId === 0)?.url);
    if (isExcluded(frames[0]?.url ?? "", await this.context.excludedSites())) return "refused";

    let values: FillValues | null = null;
    for (const { frameId, url } of frames) {
      if (item.kind === "login") {
        const hit = suggestionsFor(keyring, [item], url)[0];
        if (!hit || hit.insecure) continue;
      } else if (frameId !== 0 && originOf(url) !== top) {
        continue;
      }
      if (!values) {
        const content = await keyring.content(item.id);
        values =
          item.kind === "login"
            ? { username: text(content.data.username), password: text(content.data.password) }
            : item.kind === "card"
              ? { card: cardValues(content) }
              : { identity: identityValues(content) };
      }
      const written = await this.platform
        .sendToFrame(
          { tabId, frameId, url },
          { type: "kandado-fill-page", kind: item.kind, values },
        )
        .catch(() => 0);
      if (typeof written === "number" && written > 0) return "filled";
    }
    return "no-form";
  }

  private async valuesFor(
    entry: Pending,
    keyring: Keyring,
    choice: DropdownChoice,
  ): Promise<FillValues | "confirm-insecure" | "refused"> {
    if ("generated" in choice) {
      if (entry.kind !== "new-password" || !entry.generated) return "refused";
      return { newPassword: entry.generated };
    }
    if (!("item" in choice)) return "refused";
    const items = (await keyring.items()).filter((item) => !item.trashed);
    const item = items.find((candidate) => candidate.id === choice.item);
    if (!item) return "refused";

    if (entry.kind === "login" || entry.kind === "totp") {
      // Checked again here: the dropdown only names an item, the worker decides.
      const hit = suggestionsFor(keyring, [item], entry.frame.url)[0];
      if (!hit) return "refused";
      if (hit.insecure && !choice.confirmInsecure) return "confirm-insecure";
      if (entry.kind === "totp") {
        if (hit.insecure) return "refused";
        return { totp: await keyring.totp(item.id, this.platform.now()) };
      }
      const content = await keyring.content(item.id);
      return { username: text(content.data.username), password: text(content.data.password) };
    }
    if (entry.kind === "card" || entry.kind === "identity") {
      if (item.kind !== entry.kind || !(await this.sameOriginAsTop(entry.frame))) return "refused";
      const content = await keyring.content(item.id);
      return entry.kind === "card"
        ? { card: cardValues(content) }
        : { identity: identityValues(content) };
    }
    return "refused";
  }
}

function cardValues(content: ItemContentJson): NonNullable<FillValues["card"]> {
  const d = content.data;
  const expiry = (d.expiry ?? null) as { month?: number; year?: number } | null;
  return {
    number: text(d.number).replace(/\s+/g, ""),
    name: text(d.cardholder),
    month: expiry?.month ?? null,
    year: expiry?.year ?? null,
    cvc: text(d.cvv),
  };
}

function identityValues(content: ItemContentJson): NonNullable<FillValues["identity"]> {
  const d = content.data;
  const address = (d.address ?? {}) as Record<string, unknown>;
  const givenName = text(d.first_name);
  const familyName = text(d.last_name);
  return {
    name: [givenName, text(d.middle_name), familyName].filter(Boolean).join(" "),
    givenName,
    familyName,
    email: text(d.email),
    tel: text(d.phone),
    street: text(address.line1),
    line2: text(address.line2),
    city: text(address.city),
    region: text(address.region),
    postalCode: text(address.postal_code),
    country: text(address.country),
    organization: text(d.company),
  };
}
