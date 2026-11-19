// What the popup, pages and content scripts may ask the service worker. The
// worker answers with `Reply`. Only extension pages may ask for anything that
// returns more than titles and usernames (see router.ts).

import type {
  DropdownChoice,
  DropdownView,
  PickResult,
  TabFillResult,
} from "../background/autofill";
import type { ItemContentJson, Summary, VaultInfo } from "../background/keyring";
import type { Suggestion } from "../background/matcher";
import type {
  PasskeyAnswer,
  PasskeyOutcome,
  PasskeyRequest,
  PasskeyStart,
  PasskeyView,
} from "../background/passkeys";
import type { PromptAnswer, PromptView } from "../background/prompts";
import type { FormKind } from "../content/detect";
import type { FillValues } from "../content/fill";
import type { ExtensionSettings } from "./settings";

export type Status =
  | { state: "signed_out" }
  | { state: "locked"; email: string; pin: boolean; desktop?: boolean }
  | { state: "unlocked"; email: string; pin: boolean };

export type GeneratorOptions =
  | { mode: "random"; length: number; uppercase: boolean; numbers: boolean; symbols: boolean }
  | { mode: "memorable"; words: number; uppercase: boolean; numbers: boolean; symbols: boolean };

export interface Generated {
  password: string;
  bits: number;
  /** 0 to 4, as the strength meter shows it. */
  score: number;
}

export interface NewLogin {
  vault_id?: string;
  title: string;
  username: string;
  password: string;
  url: string;
}

export type Request =
  | { type: "status" }
  | { type: "sign-in" }
  | { type: "unlock"; password: string }
  | { type: "unlock-pin"; pin: string }
  | { type: "pin-attempts" }
  | { type: "set-pin"; pin: string }
  | { type: "clear-pin" }
  | { type: "lock" }
  | { type: "sign-out" }
  | { type: "sync" }
  | { type: "vaults" }
  | { type: "items" }
  | { type: "suggestions"; url: string }
  | { type: "item"; id: string }
  | { type: "totp"; id: string }
  | { type: "generate"; options: GeneratorOptions }
  | { type: "strength"; password: string }
  | { type: "create-login"; login: NewLogin }
  | { type: "update-login"; id: string; login: NewLogin }
  // From a content script: a field of this kind got focus in its frame.
  | { type: "dropdown-open"; kind: FormKind }
  | { type: "dropdown-key"; token: string; key: DropdownKey }
  | { type: "dropdown-close"; token: string }
  // From the dropdown page.
  | { type: "dropdown-view"; token: string }
  | { type: "dropdown-pick"; token: string; choice: DropdownChoice }
  | { type: "dropdown-size"; token: string; height: number }
  | { type: "open-popup" }
  // From the popup: fill this item into the form on the tab (E-03, E-06, E-07).
  | { type: "fill-tab"; tabId: number; itemId: string }
  // From a content script: a form was submitted with these values (E-23, E-24).
  | { type: "login-captured"; username: string; password: string }
  // From a top frame's content script: is a save or update prompt waiting here?
  | { type: "prompt-check" }
  // From the prompt page.
  | { type: "prompt-view"; token: string }
  | { type: "prompt-answer"; token: string; answer: PromptAnswer }
  | { type: "prompt-size"; token: string; height: number }
  // From a top frame's content script: the page called navigator.credentials (E-25, E-26).
  | { type: "passkey-start"; call: PasskeyRequest }
  | { type: "passkey-abort"; token: string }
  // From the prompt page, for a passkey request.
  | { type: "passkey-view"; token: string }
  | { type: "passkey-answer"; token: string; answer: PasskeyAnswer }
  // From the settings and welcome pages (E-27 to E-31).
  | { type: "settings-get" }
  | { type: "settings-set"; patch: Partial<ExtensionSettings> }
  // The desktop app link (E-02, E-31).
  | { type: "desktop-status" }
  | { type: "desktop-pair" }
  | { type: "desktop-unpair" }
  | { type: "unlock-desktop" };

/** E-31: the desktop app link as the settings page shows it. */
export interface DesktopLinkStatus {
  /** The desktop app answered through native messaging. */
  available: boolean;
  /** It is signed in to this extension's account. */
  sameAccount: boolean;
  unlocked: boolean;
  /** "Unlock the browser extension with this app" is on in D-45. */
  unlockEnabled: boolean;
  paired: boolean;
  /** Waiting for the person to accept in the app: the code to compare. */
  pairingCode: string | null;
}

/** Keys the content script passes on from the focused field (trusted key events only). */
export type DropdownKey = "up" | "down" | "enter" | "escape";

export type ErrorCode =
  | "signed_out"
  | "locked"
  | "wrong_password"
  | "wrong_pin"
  | "pin_discarded"
  | "bad_pin"
  | "desktop_unavailable"
  | "desktop_locked"
  | "desktop_other_account"
  | "desktop_off"
  | "desktop_declined"
  | "no_keys"
  | "no_vault"
  | "sign_in_cancelled"
  | "sign_in_failed"
  | "offline"
  | "not_allowed"
  | "failed";

export type Reply<T> = { ok: true; value: T } | { ok: false; error: ErrorCode };

export interface Responses {
  status: Status;
  "sign-in": Status;
  unlock: Status;
  "unlock-pin": Status;
  "pin-attempts": number;
  "set-pin": Status;
  "clear-pin": Status;
  lock: Status;
  "sign-out": Status;
  sync: { changed: boolean };
  vaults: VaultInfo[];
  items: Summary[];
  suggestions: Suggestion[];
  item: ItemContentJson;
  totp: string;
  generate: Generated;
  strength: number;
  "create-login": string;
  "update-login": null;
  "dropdown-open": string | null;
  "dropdown-key": null;
  "dropdown-close": null;
  "dropdown-view": DropdownView;
  "dropdown-pick": PickResult;
  "dropdown-size": null;
  "open-popup": null;
  "fill-tab": TabFillResult;
  "login-captured": null;
  "prompt-check": string | null;
  "prompt-view": PromptView;
  "prompt-answer": null;
  "prompt-size": null;
  "passkey-start": PasskeyStart;
  "passkey-abort": null;
  "passkey-view": PasskeyView;
  "passkey-answer": null;
  "settings-get": ExtensionSettings;
  "settings-set": ExtensionSettings;
  "desktop-status": DesktopLinkStatus;
  "desktop-pair": { code: string };
  "desktop-unpair": null;
  "unlock-desktop": Status;
}

export const REQUEST_TYPES: ReadonlySet<string> = new Set<Request["type"]>([
  "status",
  "sign-in",
  "unlock",
  "unlock-pin",
  "pin-attempts",
  "set-pin",
  "clear-pin",
  "lock",
  "sign-out",
  "sync",
  "vaults",
  "items",
  "suggestions",
  "item",
  "totp",
  "generate",
  "strength",
  "create-login",
  "update-login",
  "dropdown-open",
  "dropdown-key",
  "dropdown-close",
  "dropdown-view",
  "dropdown-pick",
  "dropdown-size",
  "open-popup",
  "fill-tab",
  "login-captured",
  "prompt-check",
  "prompt-view",
  "prompt-answer",
  "prompt-size",
  "passkey-start",
  "passkey-abort",
  "passkey-view",
  "passkey-answer",
  "settings-get",
  "settings-set",
  "desktop-status",
  "desktop-pair",
  "desktop-unpair",
  "unlock-desktop",
]);

/**
 * Requests a content script (inside a web page) may make. None returns a list or
 * a secret: the content script only says a field was focused, and passes on keys.
 */
export const CONTENT_SCRIPT_ALLOWED: ReadonlySet<Request["type"]> = new Set([
  "status",
  "dropdown-open",
  "dropdown-key",
  "dropdown-close",
  "login-captured",
  "prompt-check",
  "passkey-start",
  "passkey-abort",
]);

/** Messages the worker sends to a content script (in one frame) or to the dropdown pages. */
export type WorkerMessage =
  | { type: "kandado-fill"; values: FillValues }
  /** Fill the first form of this kind in the frame; the reply is how many inputs were written. */
  | { type: "kandado-fill-page"; kind: FormKind; values: FillValues }
  | { type: "kandado-dropdown-size"; token: string; height: number }
  | { type: "kandado-dropdown-close"; token: string }
  | { type: "kandado-dropdown-key"; token: string; key: DropdownKey }
  | { type: "kandado-prompt-size"; token: string; height: number }
  | { type: "kandado-prompt-close"; token: string }
  /** To the top frame: show the passkey prompt for this request. */
  | { type: "kandado-passkey-outcome"; token: string; outcome: PasskeyOutcome };
