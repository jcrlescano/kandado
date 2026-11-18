// The extension's own copy of the vault, in IndexedDB. Content stays encrypted at
// rest: rows hold the same ciphertext the server does, plus the sync metadata
// needed to query and merge (see sync.ts). Nothing here can be read without the
// keys the WebAssembly session holds in memory.

const VERSION = 1;

/** What the server had the last time this device synced an item (for the three-way merge). */
export interface Base {
  digest: string;
  vault_id: string;
  trashed_at: number | null;
}

export interface VaultRow {
  id: string;
  key_sealed: Uint8Array<ArrayBuffer>;
  meta_enc: Uint8Array<ArrayBuffer>;
  revision: number;
  dirty: boolean;
}

export interface ItemRow {
  id: string;
  vault_id: string;
  key_wrapped: Uint8Array<ArrayBuffer>;
  content_enc: Uint8Array<ArrayBuffer>;
  /** SHA-256 of `content_enc`, to tell whether the content changed without keeping a second copy. */
  digest: string;
  version_id: string;
  created_at: number;
  updated_at: number;
  trashed_at: number | null;
  /** The server's revision this row was last known at; 0 if the server has never seen it. */
  revision: number;
  dirty: boolean;
  base: Base | null;
}

/** An item deleted for good here (or by another device) and not yet confirmed. */
export interface Tombstone {
  item_id: string;
  vault_id: string;
  revision: number;
}

export interface Account {
  user_id: string;
  email: string;
  display_name: string;
  device_id: string;
  /** `GET /v1/account/keys` as JSON text; ciphertext and public data only. */
  keys_json: string;
}

type StoreName = "kv" | "vaults" | "items" | "tombstones";

function request<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

/** The reads and writes of one transaction. Await only these inside `atomically`. */
export class Tx {
  constructor(private readonly tx: IDBTransaction) {}

  private store(name: StoreName) {
    return this.tx.objectStore(name);
  }
  vault(id: string): Promise<VaultRow | undefined> {
    return request(this.store("vaults").get(id));
  }
  putVault(row: VaultRow) {
    return request(this.store("vaults").put(row));
  }
  vaults(): Promise<VaultRow[]> {
    return request(this.store("vaults").getAll());
  }
  item(id: string): Promise<ItemRow | undefined> {
    return request(this.store("items").get(id));
  }
  putItem(row: ItemRow) {
    return request(this.store("items").put(row));
  }
  deleteItem(id: string) {
    return request(this.store("items").delete(id));
  }
  items(): Promise<ItemRow[]> {
    return request(this.store("items").getAll());
  }
  tombstone(id: string): Promise<Tombstone | undefined> {
    return request(this.store("tombstones").get(id));
  }
  tombstones(): Promise<Tombstone[]> {
    return request(this.store("tombstones").getAll());
  }
  putTombstone(row: Tombstone) {
    return request(this.store("tombstones").put(row));
  }
  removeTombstone(id: string) {
    return request(this.store("tombstones").delete(id));
  }
  async cursor(): Promise<number> {
    return ((await request(this.store("kv").get("cursor"))) as number | undefined) ?? 0;
  }
  setCursor(cursor: number) {
    return request(this.store("kv").put(cursor, "cursor"));
  }
}

export class Store {
  private constructor(private readonly db: IDBDatabase) {}

  static async open(name = "kandado"): Promise<Store> {
    const opening = indexedDB.open(name, VERSION);
    opening.onupgradeneeded = () => {
      const db = opening.result;
      db.createObjectStore("kv");
      db.createObjectStore("vaults", { keyPath: "id" });
      db.createObjectStore("items", { keyPath: "id" });
      db.createObjectStore("tombstones", { keyPath: "item_id" });
    };
    return new Store(await request(opening));
  }

  close() {
    this.db.close();
  }

  /**
   * Runs `body` in one transaction over all stores. Everything it awaits must
   * be a request made through `Tx`; anything else lets the transaction end.
   * If `body` throws, nothing it wrote is kept.
   */
  async atomically<T>(body: (tx: Tx) => Promise<T>, write = true): Promise<T> {
    const tx = this.db.transaction(
      ["kv", "vaults", "items", "tombstones"],
      write ? "readwrite" : "readonly",
    );
    const done = new Promise<void>((resolve, reject) => {
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error ?? new Error("transaction aborted"));
    });
    let result: T;
    try {
      result = await body(new Tx(tx));
    } catch (error) {
      tx.abort();
      await done.catch(() => {});
      throw error;
    }
    await done;
    return result;
  }

  read<T>(body: (tx: Tx) => Promise<T>): Promise<T> {
    return this.atomically(body, false);
  }

  async getKv<T>(key: string): Promise<T | undefined> {
    const tx = this.db.transaction("kv", "readonly");
    return (await request(tx.objectStore("kv").get(key))) as T | undefined;
  }

  async setKv(key: string, value: unknown): Promise<void> {
    const tx = this.db.transaction("kv", "readwrite");
    await request(tx.objectStore("kv").put(value, key));
  }

  async deleteKv(key: string): Promise<void> {
    const tx = this.db.transaction("kv", "readwrite");
    await request(tx.objectStore("kv").delete(key));
  }

  account(): Promise<Account | undefined> {
    return this.getKv<Account>("account");
  }

  /** Sign out: removes the account and every row. */
  async wipe(): Promise<void> {
    const tx = this.db.transaction(["kv", "vaults", "items", "tombstones"], "readwrite");
    for (const name of ["kv", "vaults", "items", "tombstones"] as const) {
      tx.objectStore(name).clear();
    }
    await new Promise<void>((resolve, reject) => {
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
  }
}
