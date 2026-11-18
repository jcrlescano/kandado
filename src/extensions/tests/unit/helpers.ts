import "fake-indexeddb/auto";

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { type Core, provideCore } from "../../src/background/core";
import { type ItemContentJson, Keyring } from "../../src/background/keyring";
import { type Account, Store } from "../../src/background/store";
import type { Remote } from "../../src/background/sync";
import { uuidv7 } from "../../src/shared/bytes";
import type {
  ChangeResult,
  PullResponse,
  PushRequest,
  PushResponse,
  WireItem,
  WireVault,
} from "../../src/shared/types";

type CoreSession = import("../../wasm/pkg-test/kandado_wasm.js").Session;

type TestCore = Core & {
  Session: Core["Session"] & {
    createAccountForTests(
      userId: string,
      password: string,
    ): { keysJson: string; intoSession(): CoreSession };
  };
};

let core: TestCore | undefined;

/** The WebAssembly core built with the test helpers (see global-setup.ts). */
export async function testCore(): Promise<TestCore> {
  if (core) return core;
  const dir = resolve(import.meta.dirname, "../../wasm/pkg-test");
  const mod = (await import(/* @vite-ignore */ `${dir}/kandado_wasm.js`)) as TestCore & {
    initSync(options: { module: BufferSource }): unknown;
  };
  mod.initSync({ module: readFileSync(`${dir}/kandado_wasm_bg.wasm`) });
  core = mod;
  provideCore(mod);
  return mod;
}

export const PASSWORD = "correct horse battery staple";

export interface World {
  account: Account;
  vaultId: string;
  /** What the server holds of the vault. */
  vault: WireVault;
}

/** An account with one vault, made the way the desktop app makes them. */
export async function newWorld(): Promise<World> {
  const c = await testCore();
  const userId = uuidv7();
  const made = c.Session.createAccountForTests(userId, PASSWORD);
  // `intoSession` consumes the account, so read its keys first.
  const keysJson = made.keysJson;
  const session = made.intoSession() as CoreSession & {
    createVaultForTests(id: string, name: string): { key_sealed: Uint8Array; meta_enc: Uint8Array };
  };
  const vaultId = uuidv7();
  const sealed = session.createVaultForTests(vaultId, "Personal");
  const { toBase64 } = await import("../../src/shared/bytes");
  const vault: WireVault = {
    id: vaultId,
    key_sealed: toBase64(sealed.key_sealed),
    meta_enc: toBase64(sealed.meta_enc),
    revision: 0,
  };
  session.free();
  return {
    account: {
      user_id: userId,
      email: "juan@example.com",
      display_name: "Juan",
      device_id: uuidv7(),
      keys_json: keysJson,
    },
    vaultId,
    vault,
  };
}

let counter = 0;

/** A device: its own IndexedDB and an unlocked keyring over it. */
export async function newDevice(world: World): Promise<{ store: Store; keyring: Keyring }> {
  counter += 1;
  const store = await Store.open(`kandado-test-${counter}-${uuidv7()}`);
  const keyring = await Keyring.unlock(await testCore(), store, world.account, PASSWORD);
  return { store, keyring };
}

export function login(
  title: string,
  password = "pw",
  url = "https://example.com",
): ItemContentJson {
  return {
    title,
    notes: "",
    favourite: false,
    custom_fields: [],
    kind: "login",
    data: {
      username: "juan",
      password,
      websites: [{ url, match_mode: "domain" }],
    },
  };
}

/**
 * An in-memory Kandado server for the sync rules: one global revision counter,
 * a change is accepted only against the row's current revision, a tombstone wins.
 * It mirrors `kandado-server/src/db/sync.rs`.
 */
export class FakeServer implements Remote {
  private revision = 0;
  readonly vaults = new Map<string, WireVault>();
  readonly items = new Map<string, WireItem>();
  pulls = 0;

  constructor(vault?: WireVault) {
    if (vault) this.vaults.set(vault.id, { ...vault, revision: this.next() });
  }

  private next(): number {
    this.revision += 1;
    return this.revision;
  }

  /** Vaults and items together, oldest revision first, cut at `limit` rows (as the real pull does). */
  async pull(cursor: number, limit: number): Promise<PullResponse> {
    this.pulls += 1;
    const rows = [
      ...[...this.vaults.values()].map((vault) => ({ revision: vault.revision, vault })),
      ...[...this.items.values()].map((item) => ({ revision: item.revision, item })),
    ]
      .filter((row) => row.revision > cursor)
      .sort((x, y) => x.revision - y.revision);
    const page = rows.slice(0, limit);
    const vaults: WireVault[] = [];
    const items: WireItem[] = [];
    for (const row of page) {
      if ("vault" in row) vaults.push(row.vault);
      else items.push(row.item);
    }
    return {
      vaults,
      items,
      cursor: page.at(-1)?.revision ?? cursor,
      more: rows.length > page.length,
    };
  }

  async push(request: PushRequest): Promise<PushResponse> {
    const vaults: ChangeResult<WireVault>[] = request.vaults.map((change) => {
      const current = this.vaults.get(change.id);
      if (!current) {
        if (change.revision !== 0) return { status: "rejected" };
        const revision = this.next();
        this.vaults.set(change.id, { ...change, revision });
        return { status: "applied", revision };
      }
      if (current.revision !== change.revision) return { status: "conflict", current };
      const revision = this.next();
      this.vaults.set(change.id, { ...change, revision });
      return { status: "applied", revision };
    });
    const items: ChangeResult<WireItem>[] = request.items.map((change) => {
      const current = this.items.get(change.id);
      if (!this.vaults.has(change.vault_id)) return { status: "rejected" };
      if (!current) {
        if (change.revision !== 0 || change.deleted) return { status: "rejected" };
        const revision = this.next();
        this.items.set(change.id, { ...change, revision });
        return { status: "applied", revision };
      }
      if (!change.deleted && current.kind !== change.kind) return { status: "rejected" };
      if (current.deleted || current.revision !== change.revision) {
        return { status: "conflict", current };
      }
      const revision = this.next();
      this.items.set(
        change.id,
        change.deleted
          ? { ...current, deleted: true, key_wrapped: "", content_enc: "", revision }
          : { ...change, revision },
      );
      return { status: "applied", revision };
    });
    return { vaults, items, cursor: this.revision };
  }
}
