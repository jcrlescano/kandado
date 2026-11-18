// The unlocked vault: the WebAssembly session (which holds every key) together
// with the encrypted rows in IndexedDB.
//
// JavaScript never holds a key. It holds a handle, asks the core to decrypt a
// row, and gets back what the caller needs now: a summary for a list, or one
// item's whole content for a fill, a copy or a reveal.

import { sha256Hex } from "../shared/bytes";
import type { ItemKind } from "../shared/types";
import type { Core } from "./core";
import type { Account, ItemRow, Store } from "./store";
import type { Keys } from "./sync";

type CoreSession = import("../../wasm/pkg/kandado_wasm.js").Session;

/** What a list row shows. Holds no secret. */
export interface Summary {
  id: string;
  vault_id: string;
  kind: ItemKind;
  title: string;
  subtitle: string;
  websites: { url: string; match_mode: "domain" | "host" | "never" }[];
  favourite: boolean;
  has_totp: boolean;
  trashed: boolean;
  updated_at: number;
}

export interface VaultInfo {
  id: string;
  name: string;
  colour: string;
  icon: string;
}

/** One item's whole content, as the core encodes it. Contains secrets: drop it after use. */
export interface ItemContentJson {
  title: string;
  notes: string;
  favourite: boolean;
  custom_fields: { label: string; value: string; hidden: boolean }[];
  kind: ItemKind;
  data: Record<string, unknown>;
  [extra: string]: unknown;
}

export class WrongPassword extends Error {
  constructor() {
    super("wrong password");
    this.name = "WrongPassword";
  }
}

export class Keyring implements Keys {
  private readonly summaries = new Map<string, { version_id: string; summary: Summary }>();
  private freed = false;

  private constructor(
    private readonly session: CoreSession,
    private readonly store: Store,
    private readonly core: Core,
  ) {}

  /** Unlocks with the master password and opens the vaults already stored. */
  static async unlock(
    core: Core,
    store: Store,
    account: Account,
    password: string,
  ): Promise<Keyring> {
    let session: CoreSession;
    try {
      session = core.Session.unlockWithPassword(account.user_id, account.keys_json, password);
    } catch {
      throw new WrongPassword();
    }
    const keyring = new Keyring(session, store, core);
    await keyring.openStoredVaults();
    return keyring;
  }

  /** Unlocks again with the user key kept in `chrome.storage.session`. */
  static async resume(
    core: Core,
    store: Store,
    account: Account,
    userKey: Uint8Array,
  ): Promise<Keyring> {
    const session = core.Session.fromUserKey(account.user_id, account.keys_json, userKey);
    const keyring = new Keyring(session, store, core);
    await keyring.openStoredVaults();
    return keyring;
  }

  private async openStoredVaults(): Promise<void> {
    for (const vault of await this.store.read((tx) => tx.vaults())) {
      try {
        this.session.openVault(vault.id, vault.key_sealed);
      } catch {
        // Not openable: left out, as in sync.
      }
    }
  }

  /** Unlocks with the session PIN and its wrapped key (E-02). */
  /** Unlocks with the user key the paired desktop app sealed to this extension (E-02). */
  static async fromDesktop(
    core: Core,
    store: Store,
    account: Account,
    pairingPrivate: Uint8Array,
    pairingId: string,
    sealed: Uint8Array,
  ): Promise<Keyring> {
    const session = core.Session.fromDesktop(
      account.user_id,
      account.keys_json,
      pairingPrivate,
      pairingId,
      sealed,
    );
    const keyring = new Keyring(session, store, core);
    await keyring.openStoredVaults();
    return keyring;
  }

  static async unlockWithPin(
    core: Core,
    store: Store,
    account: Account,
    pin: string,
    blob: Uint8Array,
  ): Promise<Keyring> {
    const session = core.Session.unlockWithPin(account.user_id, account.keys_json, pin, blob);
    const keyring = new Keyring(session, store, core);
    await keyring.openStoredVaults();
    return keyring;
  }

  /** The user key under a PIN, for `chrome.storage.session` only. */
  wrapWithPin(pin: string): Uint8Array {
    return this.session.wrapWithPin(pin);
  }

  /** For `chrome.storage.session` only. */
  exportUserKey(): Uint8Array {
    return this.session.exportUserKey();
  }

  /** Locks: wipes every key and decrypted summary from memory. */
  lock(): void {
    if (this.freed) return;
    this.freed = true;
    this.summaries.clear();
    this.session.free();
  }

  get locked(): boolean {
    return this.freed;
  }

  // ---- Keys, for sync -------------------------------------------------------

  openVault(vaultId: string, keySealed: Uint8Array): void {
    this.session.openVault(vaultId, keySealed);
  }

  itemKind(
    vaultId: string,
    itemId: string,
    keyWrapped: Uint8Array,
    contentEnc: Uint8Array,
  ): ItemKind {
    return this.session.itemKind(vaultId, itemId, keyWrapped, contentEnc) as ItemKind;
  }

  // ---- reading --------------------------------------------------------------

  async vaults(): Promise<VaultInfo[]> {
    const rows = await this.store.read((tx) => tx.vaults());
    const out: VaultInfo[] = [];
    for (const row of rows) {
      try {
        const meta = JSON.parse(this.session.vaultMeta(row.id, row.meta_enc)) as {
          name: string;
          colour: string;
          icon: string;
        };
        out.push({ id: row.id, name: meta.name, colour: meta.colour, icon: meta.icon });
      } catch {
        // Left out.
      }
    }
    return out;
  }

  private summarise(row: ItemRow): Summary | null {
    const cached = this.summaries.get(row.id);
    if (cached && cached.version_id === row.version_id) {
      cached.summary.trashed = row.trashed_at !== null;
      cached.summary.vault_id = row.vault_id;
      return cached.summary;
    }
    try {
      const summary = JSON.parse(
        this.session.itemSummary(row.vault_id, row.id, row.key_wrapped, row.content_enc),
      ) as Summary;
      summary.trashed = row.trashed_at !== null;
      summary.updated_at = row.updated_at;
      this.summaries.set(row.id, { version_id: row.version_id, summary });
      return summary;
    } catch {
      return null; // From a newer version of Kandado, or damaged: left out.
    }
  }

  /** Every readable item, trashed ones included (callers filter). */
  async items(): Promise<Summary[]> {
    const rows = await this.store.read((tx) => tx.items());
    const live = new Set(rows.map((row) => row.id));
    for (const id of this.summaries.keys()) if (!live.has(id)) this.summaries.delete(id);
    return rows.flatMap((row) => {
      const summary = this.summarise(row);
      return summary ? [summary] : [];
    });
  }

  /** One item's whole content. Ask only at the moment of fill, copy or reveal. */
  async content(itemId: string): Promise<ItemContentJson> {
    const row = await this.store.read((tx) => tx.item(itemId));
    if (!row) throw new Error("no such item");
    return JSON.parse(
      this.session.itemContent(row.vault_id, row.id, row.key_wrapped, row.content_enc),
    ) as ItemContentJson;
  }

  async totp(itemId: string, nowMs: number): Promise<string> {
    const row = await this.store.read((tx) => tx.item(itemId));
    if (!row) throw new Error("no such item");
    return this.session.itemTotp(
      row.vault_id,
      row.id,
      row.key_wrapped,
      row.content_enc,
      Math.floor(nowMs / 1000),
    );
  }

  /** Whether a saved website fits a page: "no", "yes", or "insecure" (HTTP page, HTTPS login). */
  matchUrl(saved: string, mode: string, page: string): "no" | "yes" | "insecure" {
    return (["no", "yes", "insecure"] as const)[this.core.matchUrl(saved, mode, page)] ?? "no";
  }

  // ---- writing --------------------------------------------------------------

  /** Encrypts a new item and queues it for the next sync. */
  async createItem(
    vaultId: string,
    itemId: string,
    content: ItemContentJson,
    nowSeconds: number,
    versionId: string,
  ): Promise<void> {
    const sealed = this.session.sealNewItem(vaultId, itemId, JSON.stringify(content));
    const row: ItemRow = {
      id: itemId,
      vault_id: vaultId,
      key_wrapped: new Uint8Array(sealed.key_wrapped),
      content_enc: new Uint8Array(sealed.content_enc),
      digest: await digestOf(sealed.content_enc),
      version_id: versionId,
      created_at: nowSeconds,
      updated_at: nowSeconds,
      trashed_at: null,
      revision: 0,
      dirty: true,
      base: null,
    };
    sealed.free();
    await this.store.atomically(async (tx) => {
      await tx.putItem(row);
    });
  }

  /** Saves an edit (for example a new password). The server keeps the replaced version. */
  async updateItem(
    itemId: string,
    content: ItemContentJson,
    nowSeconds: number,
    versionId: string,
  ): Promise<void> {
    const row = await this.store.read((tx) => tx.item(itemId));
    if (!row) throw new Error("no such item");
    const sealed = this.session.sealItemUpdate(
      row.vault_id,
      itemId,
      row.key_wrapped,
      JSON.stringify(content),
    );
    const contentEnc = new Uint8Array(sealed.content_enc);
    sealed.free();
    const digest = await digestOf(contentEnc);
    await this.store.atomically(async (tx) => {
      const current = await tx.item(itemId);
      if (!current) return;
      await tx.putItem({
        ...current,
        content_enc: contentEnc,
        digest,
        version_id: versionId,
        updated_at: nowSeconds,
        dirty: true,
      });
    });
    this.summaries.delete(itemId);
  }
}

function digestOf(bytes: Uint8Array): Promise<string> {
  return sha256Hex(new Uint8Array(bytes));
}
