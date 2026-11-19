// The service worker's logic, apart from the `chrome.*` calls (which come in as a
// `Platform`), so the whole lifecycle can be tested without a browser.
//
//   signed out ──sign in──► locked ──unlock──► unlocked ──lock / auto-lock──► locked
//
// MV3 service workers are stopped when idle. While unlocked, the user key is kept
// in `chrome.storage.session` (memory-backed, cleared when the browser closes) so
// a restarted worker stays unlocked until the auto-lock time passes. It is never
// written anywhere else.

import { fromBase64, toBase64, uuidv7 } from "../shared/bytes";
import type {
  DesktopLinkStatus,
  ErrorCode,
  Generated,
  GeneratorOptions,
  NewLogin,
  Request,
  Responses,
  Status,
  WorkerMessage,
} from "../shared/messages";
import { type ExtensionSettings, normaliseSite, readSettings } from "../shared/settings";
import { type Api, ApiError } from "./api";
import { type IdentityFlow, SignInError, signIn } from "./auth";
import { Autofill, type FrameRef } from "./autofill";
import { BREACH_LIST_FRESH_MS, type BreachSite, type SiteBreach, siteBreach } from "./breaches";
import { type Core, loadCore } from "./core";
import { type ConnectNative, DesktopLink } from "./desktop";
import { type ItemContentJson, Keyring, WrongPassword } from "./keyring";
import { suggestionsFor } from "./matcher";
import { Passkeys } from "./passkeys";
import { Prompts } from "./prompts";
import type { Account, Store } from "./store";
import { sync } from "./sync";

export const SYNC_EVERY_MINUTES = 5;
export const MAX_PIN_ATTEMPTS = 5;

const DEFAULT_GENERATOR: GeneratorOptions = {
  mode: "random",
  length: 20,
  uppercase: true,
  numbers: true,
  symbols: true,
};

/** A pairing with the desktop app (`chrome.storage.local`). */
interface StoredPairing {
  id: string;
  user_id: string;
  /** The pairing private key, base64. */
  private_key: string;
  /** The person accepted it in the app. */
  paired: boolean;
}

interface StoredPin {
  /** The user key wrapped by the PIN, base64 (see `Session.wrapWithPin`). */
  blob: string;
  wrong: number;
}

function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return "";
  }
}

export interface KeyValue {
  get<T>(key: string): Promise<T | undefined>;
  set(key: string, value: unknown): Promise<void>;
  remove(key: string): Promise<void>;
}

export interface Platform {
  /** `chrome.storage.session`: memory only, cleared when the browser closes. */
  session: KeyValue;
  /** `chrome.storage.local`: settings. */
  local: KeyValue;
  identity: IdentityFlow;
  device: { name: string; platform: string };
  alarms: {
    set(name: "auto-lock" | "sync", delayMinutes: number, periodic?: boolean): void;
    clear(name: "auto-lock" | "sync"): void;
  };
  now(): number;
  /** Sends a message to one frame's content script. */
  sendToFrame(frame: FrameRef, message: WorkerMessage): Promise<unknown>;
  /** Every frame of a tab, top first. */
  frames(tabId: number): Promise<{ frameId: number; url: string }[]>;
  /** Sends a message to every extension page (the dropdowns pick theirs by token). */
  broadcast(message: WorkerMessage): Promise<void>;
  /** The top page's address of a tab, if the extension may read it. */
  tabUrl(tabId: number): Promise<string | undefined>;
  openPopup(): Promise<void>;
  /** `chrome.runtime.connectNative` for the desktop app's host; absent in tests that do not need it. */
  connectNative?: ConnectNative;
  /** "Chrome", "Edge" or "Firefox", as the desktop app shows it. */
  browserName?: string;
}

export class WorkerError extends Error {
  constructor(readonly code: ErrorCode) {
    super(code);
    this.name = "WorkerError";
  }
}

type Settings = ExtensionSettings;

interface StoredSession {
  /** The user key, base64. */
  key: string;
  /** Unix milliseconds: past this the session is locked. */
  lock_at: number;
}

export class Worker {
  private keyring: Keyring | null = null;
  private account: Account | undefined;
  private core: Core | null = null;
  private syncing: Promise<{ changed: boolean }> | null = null;

  constructor(
    private readonly store: Store,
    private readonly api: Api,
    private readonly baseUrl: string,
    private readonly platform: Platform,
    private readonly loadCoreFn: () => Promise<Core> = loadCore,
  ) {
    this.autofill = new Autofill(
      {
        keyring: () => (this.keyring && !this.keyring.locked ? this.keyring : null),
        signedIn: () => this.account !== undefined,
        excludedSites: async () => (await this.settings()).excludedSites,
        enabled: async (kind) => {
          const settings = await this.settings();
          return settings.showInFields && (kind !== "new-password" || settings.suggestPasswords);
        },
        generate: async () => {
          const result = await this.generate(DEFAULT_GENERATOR);
          return { password: result.password, bits: result.bits };
        },
      },
      {
        sendToFrame: (frame, message) => this.platform.sendToFrame(frame, message as WorkerMessage),
        tabUrl: (tabId) => this.platform.tabUrl(tabId),
        frames: (tabId) => this.platform.frames(tabId),
        openPopup: () => this.platform.openPopup(),
        now: () => this.platform.now(),
      },
    );
  }

  readonly autofill: Autofill;

  readonly prompts = new Prompts({
    keyring: () => (this.keyring && !this.keyring.locked ? this.keyring : null),
    excludedSites: async () => (await this.settings()).excludedSites,
    enabled: async () => (await this.settings()).offerToSave,
    exclude: async (host) => {
      const site = normaliseSite(host);
      const settings = await this.settings();
      if (!site || settings.excludedSites.includes(site)) return;
      await this.changeSettings({ excludedSites: [...settings.excludedSites, site] });
    },
    save: async (keyring, vaultId, url, captured) => {
      let origin = url;
      try {
        origin = new URL(url).origin;
      } catch {
        // Keep the address as it came.
      }
      await this.createLogin(keyring, {
        vault_id: vaultId,
        title: "",
        username: captured.username,
        password: captured.password,
        url: origin,
      });
    },
    updatePassword: async (keyring, itemId, password) => {
      const content = await keyring.content(itemId);
      const now = this.platform.now();
      await keyring.updateItem(
        itemId,
        {
          ...content,
          data: { ...content.data, password, password_changed_at: Math.floor(now / 1000) },
        },
        Math.floor(now / 1000),
        uuidv7(now),
      );
      void this.sync().catch(() => {});
    },
    now: () => this.platform.now(),
  });

  readonly passkeys = new Passkeys({
    keyring: () => (this.keyring && !this.keyring.locked ? this.keyring : null),
    signedIn: () => this.account !== undefined,
    enabled: async () => (await this.settings()).passkeys,
    excludedSites: async () => (await this.settings()).excludedSites,
    create: async (keyring, vaultId, requestJson) => {
      const now = this.platform.now();
      const response = await keyring.createPasskey(
        vaultId,
        uuidv7(now),
        requestJson,
        Math.floor(now / 1000),
        uuidv7(now),
      );
      void this.sync().catch(() => {});
      return response;
    },
    assert: (keyring, itemId, requestJson) => keyring.assertPasskey(itemId, requestJson),
    deliver: async (tabId, token, outcome) => {
      await this.platform.sendToFrame(
        { tabId, frameId: 0, url: "" },
        { type: "kandado-passkey-outcome", token, outcome },
      );
    },
    now: () => this.platform.now(),
  });

  /** Starts (or restarts) the worker: finds out whether it is still unlocked. */
  async start(): Promise<Status> {
    this.account = await this.store.account();
    if (!this.account) return { state: "signed_out" };
    await this.loadPin();
    await this.loadPairing();
    const saved = await this.platform.session.get<StoredSession>("session");
    if (saved && saved.lock_at > this.platform.now() && !this.keyring) {
      try {
        this.keyring = await Keyring.resume(
          await this.coreOnce(),
          this.store,
          this.account,
          fromBase64(saved.key),
        );
      } catch {
        await this.platform.session.remove("session");
      }
    } else if (saved) {
      await this.platform.session.remove("session");
    }
    return this.status();
  }

  private async coreOnce(): Promise<Core> {
    this.core ??= await this.loadCoreFn();
    return this.core;
  }

  status(): Status {
    if (!this.account) return { state: "signed_out" };
    if (this.keyring && !this.keyring.locked)
      return { state: "unlocked", email: this.account.email, pin: this.hasPin };
    return {
      state: "locked",
      email: this.account.email,
      pin: this.hasPin,
      desktop: this.pairing?.paired === true,
    };
  }

  // ---- PIN (E-02) ---------------------------------------------------------------
  //
  // The PIN-wrapped user key lives in session storage only, so the first unlock
  // after the browser starts always takes the master password. Five wrong PINs
  // discard it.

  private hasPin = false;

  private async loadPin(): Promise<void> {
    this.hasPin = (await this.platform.session.get<StoredPin>("pin")) !== undefined;
  }

  /** Sets the PIN for the rest of this browser session. */
  async setPin(pin: string): Promise<Status> {
    const keyring = this.unlockedKeyring();
    let blob: Uint8Array;
    try {
      blob = keyring.wrapWithPin(pin);
    } catch {
      throw new WorkerError("bad_pin");
    }
    await this.platform.session.set("pin", { blob: toBase64(blob), wrong: 0 } satisfies StoredPin);
    this.hasPin = true;
    return this.status();
  }

  async clearPin(): Promise<Status> {
    await this.platform.session.remove("pin");
    this.hasPin = false;
    return this.status();
  }

  async unlockWithPin(pin: string): Promise<Status> {
    if (!this.account) throw new WorkerError("signed_out");
    const stored = await this.platform.session.get<StoredPin>("pin");
    if (!stored) {
      this.hasPin = false;
      throw new WorkerError("pin_discarded");
    }
    try {
      this.keyring = await Keyring.unlockWithPin(
        await this.coreOnce(),
        this.store,
        this.account,
        pin,
        fromBase64(stored.blob),
      );
    } catch {
      const wrong = stored.wrong + 1;
      if (wrong >= MAX_PIN_ATTEMPTS) {
        await this.clearPin();
        throw new WorkerError("pin_discarded");
      }
      await this.platform.session.set("pin", { ...stored, wrong } satisfies StoredPin);
      throw new WorkerError("wrong_pin");
    }
    await this.platform.session.set("pin", { ...stored, wrong: 0 } satisfies StoredPin);
    return this.afterUnlock();
  }

  /** Attempts left before the PIN is discarded. */
  async pinAttemptsLeft(): Promise<number> {
    const stored = await this.platform.session.get<StoredPin>("pin");
    return stored ? MAX_PIN_ATTEMPTS - stored.wrong : 0;
  }

  private unlockedKeyring(): Keyring {
    if (!this.account) throw new WorkerError("signed_out");
    if (!this.keyring || this.keyring.locked) throw new WorkerError("locked");
    return this.keyring;
  }

  // ---- account ----------------------------------------------------------------

  /** E-01: Google sign-in. Leaves the worker locked; the master password unlocks it. */
  async signIn(): Promise<Status> {
    try {
      const ids = await signIn(
        this.api,
        this.baseUrl,
        this.platform.identity,
        this.platform.device,
      );
      const me = await this.api.me();
      if (!me.has_keys) {
        // The master password and the recovery phrase are made in the desktop app (D-02, D-03).
        await this.api.logout().catch(() => {});
        throw new WorkerError("no_keys");
      }
      const account: Account = {
        user_id: ids.user_id,
        email: me.email,
        display_name: me.display_name,
        device_id: ids.device_id,
        keys_json: await this.api.accountKeys(),
      };
      await this.store.setKv("account", account);
      this.account = account;
      return this.status();
    } catch (error) {
      if (error instanceof WorkerError) throw error;
      if (error instanceof SignInError) {
        throw new WorkerError(
          error.reason === "cancelled" ? "sign_in_cancelled" : "sign_in_failed",
        );
      }
      if (error instanceof ApiError) throw new WorkerError("sign_in_failed");
      throw new WorkerError("offline");
    }
  }

  /** E-02: the master password. */
  async unlock(password: string): Promise<Status> {
    if (!this.account) throw new WorkerError("signed_out");
    try {
      this.keyring = await Keyring.unlock(
        await this.coreOnce(),
        this.store,
        this.account,
        password,
      );
    } catch (error) {
      if (error instanceof WrongPassword) throw new WorkerError("wrong_password");
      throw new WorkerError("failed");
    }
    return this.afterUnlock();
  }

  private async afterUnlock(): Promise<Status> {
    await this.remember();
    this.platform.alarms.set("sync", SYNC_EVERY_MINUTES, true);
    void this.sync().catch(() => {});
    return this.status();
  }

  /** Writes the user key to session storage and arms the auto-lock alarm. */
  private async remember(): Promise<void> {
    const keyring = this.unlockedKeyring();
    const minutes = (await this.settings()).autoLockMinutes;
    await this.platform.session.set("session", {
      key: toBase64(keyring.exportUserKey()),
      lock_at: this.platform.now() + minutes * 60_000,
    } satisfies StoredSession);
    this.platform.alarms.set("auto-lock", minutes);
  }

  /** Activity pushes the auto-lock time back. */
  async touch(): Promise<void> {
    if (this.keyring && !this.keyring.locked) await this.remember();
  }

  async lock(): Promise<Status> {
    this.prompts.clear();
    this.keyring?.lock();
    this.keyring = null;
    await this.platform.session.remove("session");
    this.platform.alarms.clear("auto-lock");
    this.platform.alarms.clear("sync");
    return this.status();
  }

  /** Sign out: tells the server, locks, and removes every local row. */
  async signOut(): Promise<Status> {
    await this.lock();
    await this.clearPin();
    await this.unpairDesktop();
    await this.api.logout().catch(() => {});
    await this.store.wipe();
    this.account = undefined;
    return this.status();
  }

  // ---- sync -------------------------------------------------------------------

  /** One sync round. Two calls at once share one round. */
  sync(): Promise<{ changed: boolean }> {
    const keyring = this.unlockedKeyring();
    this.syncing ??= sync(this.store, keyring, this.api)
      .then((report) => ({ changed: report.changed }))
      .catch((error: unknown) => {
        if (error instanceof ApiError && error.sessionEnded) {
          // Another computer signed this device out: back to E-01.
          void this.signOut();
          throw new WorkerError("signed_out");
        }
        throw new WorkerError("offline");
      })
      .finally(() => {
        this.syncing = null;
      });
    return this.syncing;
  }

  // ---- reading ----------------------------------------------------------------

  async settings(): Promise<Settings> {
    return readSettings(await this.platform.local.get<unknown>("settings"));
  }

  /** E-28 to E-31: changes some settings. Anything not valid is replaced by its default. */
  async changeSettings(patch: Partial<Settings>): Promise<Settings> {
    const before = await this.settings();
    const next = readSettings({ ...before, ...patch });
    await this.platform.local.set("settings", next);
    // A new lock time counts from now.
    if (next.autoLockMinutes !== before.autoLockMinutes) await this.touch();
    return next;
  }

  /** The toolbar badge for a tab's page: how many logins fit it (E-28). Empty when off or locked. */
  async badgeFor(url: string | undefined): Promise<string> {
    const keyring = this.keyring && !this.keyring.locked ? this.keyring : null;
    if (!keyring || !url || !(await this.settings()).toolbarCount) return "";
    const settings = await this.settings();
    const count = suggestionsFor(
      keyring,
      await keyring.items(),
      url,
      settings.excludedSites,
    ).length;
    return count > 0 ? String(count) : "";
  }

  /** Answers a request from an extension page. */
  async handle<R extends Request>(request: R, frame?: FrameRef): Promise<Responses[R["type"]]> {
    type Out = Responses[R["type"]];
    switch (request.type) {
      case "dropdown-open":
        if (!frame) throw new WorkerError("not_allowed");
        return (await this.autofill.open(frame, request.kind)) as Out;
      case "dropdown-key": {
        // Only the frame that owns the dropdown may steer it.
        const owner = this.autofill.frameOf(request.token);
        if (!frame || !owner || owner.tabId !== frame.tabId || owner.frameId !== frame.frameId) {
          return null as Out;
        }
        await this.platform.broadcast({
          type: "kandado-dropdown-key",
          token: request.token,
          key: request.key,
        });
        return null as Out;
      }
      case "dropdown-close": {
        const owner = this.autofill.frameOf(request.token);
        if (frame && owner && (owner.tabId !== frame.tabId || owner.frameId !== frame.frameId)) {
          return null as Out;
        }
        this.autofill.close(request.token);
        // Closed from the dropdown itself: tell the page to remove it.
        if (!frame && owner) {
          await this.platform
            .sendToFrame(owner, { type: "kandado-dropdown-close", token: request.token })
            .catch(() => {});
        }
        return null as Out;
      }
      case "dropdown-view":
        return (await this.autofill.view(request.token)) as Out;
      case "dropdown-pick":
        return (await this.autofill.pick(request.token, request.choice)) as Out;
      case "dropdown-size": {
        const owner = this.autofill.frameOf(request.token);
        if (owner) {
          await this.platform
            .sendToFrame(owner, {
              type: "kandado-dropdown-size",
              token: request.token,
              height: Math.max(0, Math.min(480, Math.round(request.height))),
            })
            .catch(() => {});
        }
        return null as Out;
      }
      case "open-popup":
        await this.platform.openPopup();
        return null as Out;
      case "login-captured":
        if (!frame) throw new WorkerError("not_allowed");
        await this.prompts.capture(frame, {
          username: String(request.username ?? ""),
          password: String(request.password ?? ""),
        });
        return null as Out;
      case "prompt-check":
        if (!frame) throw new WorkerError("not_allowed");
        return (await this.prompts.check(frame)) as Out;
      case "prompt-view":
        return (await this.prompts.view(request.token)) as Out;
      case "prompt-answer": {
        const tabId = await this.prompts.answer(request.token, request.answer);
        if (tabId !== null) {
          await this.platform
            .sendToFrame(
              { tabId, frameId: 0, url: "" },
              { type: "kandado-prompt-close", token: request.token },
            )
            .catch(() => {});
        }
        return null as Out;
      }
      case "passkey-start":
        if (!frame) throw new WorkerError("not_allowed");
        return (await this.passkeys.start(frame, request.call)) as Out;
      case "passkey-abort":
        if (!frame) throw new WorkerError("not_allowed");
        await this.passkeys.abort(frame, String(request.token));
        return null as Out;
      case "passkey-view":
        return (await this.passkeys.view(request.token)) as Out;
      case "passkey-answer":
        // The outcome goes to the top frame, which closes the prompt itself.
        await this.passkeys.answer(request.token, request.answer);
        return null as Out;
      case "prompt-size": {
        const tabId = this.prompts.tabOf(request.token) ?? this.passkeys.tabOf(request.token);
        if (tabId !== null) {
          await this.platform
            .sendToFrame(
              { tabId, frameId: 0, url: "" },
              {
                type: "kandado-prompt-size",
                token: request.token,
                height: Math.max(0, Math.min(480, Math.round(request.height))),
              },
            )
            .catch(() => {});
        }
        return null as Out;
      }
      case "desktop-status":
        return (await this.desktopStatus()) as Out;
      case "desktop-unpair":
        await this.unpairDesktop();
        return null as Out;
      case "unlock-desktop":
        return (await this.unlockWithDesktop()) as Out;
      case "desktop-pair":
        return (await this.pairDesktop()) as Out;
      case "settings-get":
        return (await this.settings()) as Out;
      case "settings-set":
        return (await this.changeSettings(request.patch)) as Out;
      case "fill-tab":
        return (await this.autofill.fillTab(request.tabId, request.itemId)) as Out;
      case "status":
        return this.status() as Out;
      case "sign-in":
        return (await this.signIn()) as Out;
      case "unlock":
        return (await this.unlock(request.password)) as Out;
      case "lock":
        return (await this.lock()) as Out;
      case "sign-out":
        return (await this.signOut()) as Out;
      case "sync":
        return (await this.sync()) as Out;
      case "unlock-pin":
        return (await this.unlockWithPin(request.pin)) as Out;
      case "pin-attempts":
        return (await this.pinAttemptsLeft()) as Out;
      case "generate":
        return (await this.generate(request.options)) as Out;
      case "strength":
        return (await this.coreOnce()).passwordScore(request.password) as Out;
    }
    const keyring = this.unlockedKeyring();
    await this.touch();
    switch (request.type) {
      case "set-pin":
        return (await this.setPin(request.pin)) as Out;
      case "clear-pin":
        return (await this.clearPin()) as Out;
      case "create-login":
        return (await this.createLogin(keyring, request.login)) as Out;
      case "update-login":
        await this.updateLogin(keyring, request.id, request.login);
        return null as Out;
      case "vaults":
        return (await keyring.vaults()) as Out;
      case "items":
        return (await keyring.items()).filter((item) => !item.trashed) as Out;
      case "site-breach":
        return (await this.siteBreach(keyring, request.url)) as Out;
      case "suggestions":
        return suggestionsFor(
          keyring,
          await keyring.items(),
          request.url,
          (await this.settings()).excludedSites,
        ) as Out;
      case "item":
        return (await keyring.content(request.id)) as Out;
      case "totp":
        return (await keyring.totp(request.id, this.platform.now())) as Out;
    }
  }

  /** E-11 and E-10: a password from the shared generator, with its strength. */
  private async generate(options: GeneratorOptions): Promise<Generated> {
    const core = await this.coreOnce();
    const result =
      options.mode === "memorable"
        ? core.generateMemorablePassword(
            options.words,
            options.uppercase,
            options.numbers,
            options.symbols,
          )
        : core.generateRandomPassword(
            options.length,
            options.uppercase,
            options.numbers,
            options.symbols,
          );
    const out = { password: result.password, bits: result.bits, score: result.score };
    result.free();
    return out;
  }

  // ---- E-15: the breached-site banner ------------------------------------------

  private breachList: { at: number; breaches: BreachSite[] } | null = null;

  /** Have I Been Pwned's breach list, kept for twelve hours. Empty when unreachable. */
  private async breaches(): Promise<BreachSite[]> {
    const now = this.platform.now();
    if (this.breachList && now - this.breachList.at < BREACH_LIST_FRESH_MS) {
      return this.breachList.breaches;
    }
    try {
      const list = await this.api.breaches();
      this.breachList = { at: now, breaches: list.breaches };
    } catch {
      // Offline: keep an older list if there is one, and try again next time.
      return this.breachList?.breaches ?? [];
    }
    return this.breachList.breaches;
  }

  private async siteBreach(keyring: Keyring, url: string): Promise<SiteBreach | null> {
    const settings = await this.settings();
    const suggestions = suggestionsFor(keyring, await keyring.items(), url, settings.excludedSites);
    if (suggestions.length === 0) return null;
    return siteBreach(keyring, await this.breaches(), url, suggestions);
  }

  /** E-10: a new login, saved here and sent with the next sync. */
  private async createLogin(keyring: Keyring, login: NewLogin): Promise<string> {
    const vaults = await keyring.vaults();
    const vault = vaults.find((v) => v.id === login.vault_id) ?? vaults[0];
    if (!vault) throw new WorkerError("no_vault");
    const id = uuidv7(this.platform.now());
    const content: ItemContentJson = {
      title: login.title.trim() || hostOf(login.url) || "Login",
      notes: "",
      favourite: false,
      custom_fields: [],
      kind: "login",
      data: {
        username: login.username,
        password: login.password,
        websites: login.url.trim() ? [{ url: login.url.trim(), match_mode: "domain" }] : [],
        // Monitor compares it with breach dates (E-15, D-36).
        ...(login.password ? { password_changed_at: Math.floor(this.platform.now() / 1000) } : {}),
      },
    };
    const now = Math.floor(this.platform.now() / 1000);
    await keyring.createItem(vault.id, id, content, now, uuidv7(this.platform.now()));
    void this.sync().catch(() => {});
    return id;
  }

  /** E-06 › Edit: changes title, username, password and the first website, keeping the rest. */
  private async updateLogin(keyring: Keyring, id: string, login: NewLogin): Promise<void> {
    const content = await keyring.content(id);
    if (content.kind !== "login") throw new WorkerError("failed");
    const websites = Array.isArray(content.data.websites)
      ? [...(content.data.websites as { url: string }[])]
      : [];
    const url = login.url.trim();
    if (url && websites[0]) websites[0] = { ...websites[0], url };
    else if (url) websites.push({ url, match_mode: "domain" } as { url: string });
    else websites.shift();
    const now = this.platform.now();
    await keyring.updateItem(
      id,
      {
        ...content,
        title: login.title.trim() || content.title,
        data: {
          ...content.data,
          username: login.username,
          password: login.password,
          websites,
          ...(login.password !== content.data.password
            ? { password_changed_at: Math.floor(now / 1000) }
            : {}),
        },
      },
      Math.floor(now / 1000),
      uuidv7(now),
    );
    void this.sync().catch(() => {});
  }

  // ---- the desktop app link (E-02, E-31, D-45) ------------------------------------
  //
  // Pairing makes a key pair here (in WebAssembly); the desktop app keeps the
  // public half after the person accepts the six-digit code there. Unlocking asks
  // the app, which seals the user key to that public half. The private half is
  // kept in `chrome.storage.local`: it opens only what the desktop app seals for
  // this account, and only while the desktop app is unlocked and allows it.

  private pairing: StoredPairing | undefined;
  private pairingCode: string | null = null;
  private desktopLink: DesktopLink | null = null;

  private desktop(): DesktopLink {
    this.desktopLink ??= new DesktopLink(
      this.platform.connectNative,
      (event) => void this.onDesktopEvent(event),
    );
    return this.desktopLink;
  }

  private async loadPairing(): Promise<void> {
    const stored = await this.platform.local.get<StoredPairing>("pairing");
    this.pairing = stored && stored.user_id === this.account?.user_id ? stored : undefined;
    // Stay connected while paired, to lock and unlock with the app.
    if (this.pairing?.paired) void this.desktop().status();
  }

  async desktopStatus(): Promise<DesktopLinkStatus> {
    const status = await this.desktop().status();
    return {
      available: status.available,
      sameAccount: status.userId !== undefined && status.userId === this.account?.user_id,
      unlocked: status.state === "unlocked",
      unlockEnabled: status.unlockEnabled === true,
      paired: this.pairing?.paired === true,
      pairingCode: this.pairingCode,
    };
  }

  /** E-31 turned on: starts pairing and returns the code to compare. */
  async pairDesktop(): Promise<{ code: string }> {
    if (!this.account) throw new WorkerError("signed_out");
    const status = await this.desktopStatus();
    if (!status.available) throw new WorkerError("desktop_unavailable");
    if (!status.sameAccount) throw new WorkerError("desktop_other_account");
    if (!status.unlocked) throw new WorkerError("desktop_locked");
    if (!status.unlockEnabled) throw new WorkerError("desktop_off");
    const core = await this.coreOnce();
    const key = core.createPairingKey();
    const pairing: StoredPairing = {
      id: uuidv7(this.platform.now()),
      user_id: this.account.user_id,
      private_key: toBase64(key.private_key),
      paired: false,
    };
    const code = key.code;
    const publicKey = toBase64(key.public_key);
    key.free();
    this.pairingCode = code;
    // The answer comes when the person accepts or declines in the app.
    void this.desktop()
      .pair(pairing.id, publicKey, this.platform.browserName ?? "Browser")
      .then(async (result) => {
        this.pairingCode = null;
        if (result !== "paired") return;
        this.pairing = { ...pairing, paired: true };
        await this.platform.local.set("pairing", this.pairing);
      });
    return { code };
  }

  async unpairDesktop(): Promise<void> {
    const pairing = this.pairing;
    this.pairing = undefined;
    this.pairingCode = null;
    await this.platform.local.remove("pairing");
    if (pairing)
      await this.desktop()
        .unpair(pairing.id)
        .catch(() => {});
    this.desktopLink?.close();
  }

  /** E-02: "Unlock with desktop app". */
  async unlockWithDesktop(): Promise<Status> {
    if (!this.account) throw new WorkerError("signed_out");
    const pairing = this.pairing;
    if (!pairing?.paired) throw new WorkerError("desktop_unavailable");
    const answer = await this.desktop().unlock(pairing.id);
    if (answer === "unavailable") throw new WorkerError("desktop_unavailable");
    if (answer === "locked") throw new WorkerError("desktop_locked");
    if (answer === "turned_off") throw new WorkerError("desktop_off");
    if (typeof answer !== "object") {
      // The app no longer knows this pairing: forget it here too.
      if (answer === "not_paired") await this.unpairDesktop();
      throw new WorkerError("desktop_declined");
    }
    try {
      this.keyring = await Keyring.fromDesktop(
        await this.coreOnce(),
        this.store,
        this.account,
        fromBase64(pairing.private_key),
        pairing.id,
        fromBase64(answer.sealed),
      );
    } catch {
      throw new WorkerError("desktop_declined");
    }
    return this.afterUnlock();
  }

  /** The desktop app locked or unlocked: so does this extension, when paired. */
  private async onDesktopEvent(event: "locked" | "unlocked"): Promise<void> {
    if (!this.pairing?.paired || !this.account) return;
    const unlocked = this.keyring !== null && !this.keyring.locked;
    if (event === "locked" && unlocked) await this.lock();
    if (event === "unlocked" && !unlocked) await this.unlockWithDesktop().catch(() => {});
  }

  /** An alarm fired. */
  async onAlarm(name: string): Promise<void> {
    if (name === "auto-lock") await this.lock();
    if (name === "sync" && this.keyring && !this.keyring.locked) {
      await this.sync().catch(() => {});
    }
  }
}
