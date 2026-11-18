// Sync: keeping this browser's copy the same as the server's, and so the same as
// the person's other devices. The same protocol and the same merge as the
// desktop sync engine (`kandado-vault/src/sync.rs`, decisions P3-1 to P3-4).
//
// Every write marks its row as not yet sent (`dirty`). A round then
//
//  1. pulls everything after this device's cursor and applies it, merging with
//     anything changed here;
//  2. pushes what is waiting. The server accepts a change only if it was made
//     against the row's current revision; otherwise it answers with the row as
//     it is now, which is merged in and sent again.
//
// The merge is three-way against `base`, what the server had last time: content
// and trash state are each compared with it. A part changed only here is kept,
// a part changed only elsewhere is taken, and a part changed in both goes to this
// device's change (the later writer). The server keeps every saved version, so
// the version that lost stays in the item's history. An item deleted for good is
// deleted everywhere.

import { fromBase64, sha256Hex, toBase64 } from "../shared/bytes";
import type {
  ChangeResult,
  ItemKind,
  PullResponse,
  PushRequest,
  PushResponse,
  WireItem,
  WireVault,
} from "../shared/types";
import type { ItemRow, Store, Tx, VaultRow } from "./store";

export const MAX_BATCH = 200;
export const PULL_LIMIT = 500;
const MAX_ROUNDS = 8;

export interface Remote {
  pull(cursor: number, limit: number): Promise<PullResponse>;
  push(request: PushRequest): Promise<PushResponse>;
}

/** What sync needs from the unlocked session. Keys never leave it. */
export interface Keys {
  openVault(vaultId: string, keySealed: Uint8Array): void;
  itemKind(
    vaultId: string,
    itemId: string,
    keyWrapped: Uint8Array,
    contentEnc: Uint8Array,
  ): ItemKind;
}

export interface Report {
  pulled: number;
  pushed: number;
  conflicts: number;
  rejected: number;
  /** Something visible changed: lists need drawing again. */
  changed: boolean;
}

type Digests = Map<string, string>;

async function digestsOf(items: WireItem[]): Promise<Digests> {
  const out: Digests = new Map();
  for (const item of items) {
    if (!item.deleted) out.set(item.id, await sha256Hex(fromBase64(item.content_enc)));
  }
  return out;
}

function adopt(remote: WireItem, digest: string, local: ItemRow | undefined): ItemRow {
  return {
    id: remote.id,
    vault_id: remote.vault_id,
    key_wrapped: fromBase64(remote.key_wrapped),
    content_enc: fromBase64(remote.content_enc),
    digest,
    version_id: remote.version_id,
    created_at: local ? Math.min(local.created_at, remote.created_at) : remote.created_at,
    updated_at: remote.updated_at,
    trashed_at: remote.trashed_at,
    revision: remote.revision,
    dirty: false,
    base: { digest, vault_id: remote.vault_id, trashed_at: remote.trashed_at },
  };
}

/** Merges an item changed here with a newer version from the server. */
export function merge(local: ItemRow, remote: WireItem, remoteDigest: string): ItemRow {
  const base = local.base;
  const contentChanged = base === null || local.digest !== base.digest;
  const placeChanged = base === null || local.vault_id !== base.vault_id;
  const trashChanged = base === null || local.trashed_at !== base.trashed_at;

  const merged = adopt(remote, remoteDigest, local);
  if (contentChanged) {
    merged.content_enc = local.content_enc;
    merged.digest = local.digest;
    merged.version_id = local.version_id;
    merged.updated_at = local.updated_at;
  }
  if (placeChanged) {
    merged.vault_id = local.vault_id;
    merged.key_wrapped = local.key_wrapped;
  }
  if (trashChanged) merged.trashed_at = local.trashed_at;
  merged.dirty =
    merged.digest !== remoteDigest ||
    merged.vault_id !== remote.vault_id ||
    merged.trashed_at !== remote.trashed_at;
  return merged;
}

function sameBytes(a: Uint8Array, b: Uint8Array): boolean {
  return a.length === b.length && a.every((byte, i) => byte === b[i]);
}

/** Applies one vault from the server. Returns whether anything changed. */
async function applyRemoteVault(tx: Tx, keys: Keys, remote: WireVault): Promise<boolean> {
  const local = await tx.vault(remote.id);
  const keySealed = fromBase64(remote.key_sealed);
  const metaEnc = fromBase64(remote.meta_enc);
  if (!local) {
    try {
      keys.openVault(remote.id, keySealed);
    } catch {
      // A vault this account cannot open is not kept: it would break every list.
      return false;
    }
    await tx.putVault({
      id: remote.id,
      key_sealed: keySealed,
      meta_enc: metaEnc,
      revision: remote.revision,
      dirty: false,
    });
    return true;
  }
  if (remote.revision <= local.revision) return false;
  const row: VaultRow = { ...local, revision: remote.revision };
  // Renamed here and elsewhere: this one is later, so the name stays.
  if (!(local.dirty && !sameBytes(local.meta_enc, metaEnc))) {
    row.meta_enc = metaEnc;
    row.dirty = false;
  }
  await tx.putVault(row);
  return true;
}

/** Applies one item from the server. Returns whether anything changed. */
async function applyRemoteItem(tx: Tx, remote: WireItem, digests: Digests): Promise<boolean> {
  const local = await tx.item(remote.id);
  const tombstone = await tx.tombstone(remote.id);

  if (remote.deleted) {
    // Deleted for good somewhere: deleted here too, whatever was changed here since.
    await tx.removeTombstone(remote.id);
    if (!local) return false;
    await tx.deleteItem(remote.id);
    return true;
  }
  if (tombstone) {
    // Deleted for good here, and the server has a newer version of it: the
    // deletion is still waiting to be sent, against that version.
    if (remote.revision > tombstone.revision) {
      await tx.putTombstone({ ...tombstone, revision: remote.revision });
    }
    return false;
  }
  const digest = digests.get(remote.id) ?? "";
  if (!local) {
    await tx.putItem(adopt(remote, digest, undefined));
    return true;
  }
  // Already have it: the echo of our own write, or an old page.
  if (remote.revision <= local.revision) return false;
  if (!local.dirty) {
    await tx.putItem(adopt(remote, digest, local));
    return true;
  }
  await tx.putItem(merge(local, remote, digest));
  return true;
}

/** Applies one page from the server and moves the cursor to its end. */
export async function applyPull(
  store: Store,
  keys: Keys,
  page: PullResponse,
  report: Report,
): Promise<void> {
  const digests = await digestsOf(page.items);
  await store.atomically(async (tx) => {
    for (const vault of page.vaults) {
      if (await applyRemoteVault(tx, keys, vault)) {
        report.pulled += 1;
        report.changed = true;
      }
    }
    for (const item of page.items) {
      if (await applyRemoteItem(tx, item, digests)) {
        report.pulled += 1;
        report.changed = true;
      }
    }
    await tx.setCursor(Math.max(page.cursor, await tx.cursor()));
  });
}

// ---- sending what is waiting ------------------------------------------------

interface Sent {
  vaults: { id: string; meta: Uint8Array }[];
  items: { id: string; digest: string; vault_id: string; trashed_at: number | null }[];
}

interface Batch {
  request: PushRequest;
  sent: Sent;
}

/** The kind a deletion carries. The server ignores it: it knows the item's kind. */
const TOMBSTONE_KIND: ItemKind = "note";

async function prepare(store: Store, keys: Keys, skip: Set<string>): Promise<Batch | null> {
  const [vaults, items, tombstones] = await store.read(async (tx) => {
    return [await tx.vaults(), await tx.items(), await tx.tombstones()] as const;
  });
  const request: PushRequest = { vaults: [], items: [] };
  const sent: Sent = { vaults: [], items: [] };

  for (const vault of vaults) {
    if (!vault.dirty || skip.has(vault.id) || request.vaults.length >= MAX_BATCH) continue;
    sent.vaults.push({ id: vault.id, meta: vault.meta_enc });
    request.vaults.push({
      id: vault.id,
      key_sealed: toBase64(vault.key_sealed),
      meta_enc: toBase64(vault.meta_enc),
      revision: vault.revision,
    });
  }
  for (const item of items) {
    if (!item.dirty || skip.has(item.id) || request.items.length >= MAX_BATCH) continue;
    // The kind is read from the content, which only this unlocked browser can do;
    // it is sent in the clear because the server files items by it (decision 11).
    let kind: ItemKind;
    try {
      kind = keys.itemKind(item.vault_id, item.id, item.key_wrapped, item.content_enc);
    } catch {
      continue; // Cannot be read, so it is not sent.
    }
    sent.items.push({
      id: item.id,
      digest: item.digest,
      vault_id: item.vault_id,
      trashed_at: item.trashed_at,
    });
    request.items.push({
      id: item.id,
      vault_id: item.vault_id,
      kind,
      key_wrapped: toBase64(item.key_wrapped),
      content_enc: toBase64(item.content_enc),
      version_id: item.version_id,
      created_at: item.created_at,
      updated_at: item.updated_at,
      trashed_at: item.trashed_at,
      deleted: false,
      revision: item.revision,
    });
  }
  for (const tombstone of tombstones) {
    if (skip.has(tombstone.item_id) || request.items.length >= MAX_BATCH) continue;
    request.items.push({
      id: tombstone.item_id,
      vault_id: tombstone.vault_id,
      kind: TOMBSTONE_KIND,
      key_wrapped: "",
      content_enc: "",
      version_id: "00000000-0000-0000-0000-000000000000",
      created_at: 0,
      updated_at: 0,
      trashed_at: null,
      deleted: true,
      revision: tombstone.revision,
    });
  }
  if (request.vaults.length === 0 && request.items.length === 0) return null;
  return { request, sent };
}

async function applyDeletionResult(
  tx: Tx,
  change: WireItem,
  result: ChangeResult<WireItem>,
  digests: Digests,
  report: Report,
): Promise<void> {
  if (result.status === "applied") {
    await tx.removeTombstone(change.id);
    report.pushed += 1;
  } else if (result.status === "conflict") {
    // Either it is gone already, or it changed: the deletion is sent again on
    // top of the new revision.
    await applyRemoteItem(tx, result.current, digests);
    report.conflicts += 1;
  } else {
    // Not ours, or never known to the server: nothing to delete.
    await tx.removeTombstone(change.id);
  }
}

/** Records the server's answers. Returns the ids it refused. */
async function applyPush(
  store: Store,
  keys: Keys,
  batch: Batch,
  response: PushResponse,
  report: Report,
): Promise<string[]> {
  if (
    response.vaults.length !== batch.request.vaults.length ||
    response.items.length !== batch.request.items.length
  ) {
    throw new Error("the server answered a different number of changes than were sent");
  }
  const conflicts = response.items.flatMap((result) =>
    result.status === "conflict" ? [result.current] : [],
  );
  const digests = await digestsOf(conflicts);
  const refused: string[] = [];

  await store.atomically(async (tx) => {
    for (const [index, result] of response.vaults.entries()) {
      const sent = batch.sent.vaults[index];
      if (!sent) continue;
      if (result.status === "applied") {
        const row = await tx.vault(sent.id);
        if (row) {
          await tx.putVault({
            ...row,
            revision: result.revision,
            dirty: !sameBytes(row.meta_enc, sent.meta),
          });
          report.pushed += 1;
        }
      } else if (result.status === "conflict") {
        await applyRemoteVault(tx, keys, result.current);
        report.conflicts += 1;
        report.changed = true;
      } else {
        refused.push(sent.id);
        report.rejected += 1;
      }
    }

    let live = 0;
    for (const [index, result] of response.items.entries()) {
      const change = batch.request.items[index];
      if (!change) continue;
      if (change.deleted) {
        await applyDeletionResult(tx, change, result, digests, report);
        continue;
      }
      const sent = batch.sent.items[live];
      live += 1;
      if (!sent) throw new Error("the batch and its record disagree");
      if (result.status === "applied") {
        const row = await tx.item(sent.id);
        if (row) {
          await tx.putItem({
            ...row,
            revision: result.revision,
            base: { digest: sent.digest, vault_id: sent.vault_id, trashed_at: sent.trashed_at },
            // Edited again while the request was on its way: still waiting.
            dirty:
              row.digest !== sent.digest ||
              row.vault_id !== sent.vault_id ||
              row.trashed_at !== sent.trashed_at,
          });
          report.pushed += 1;
        }
      } else if (result.status === "conflict") {
        await applyRemoteItem(tx, result.current, digests);
        report.conflicts += 1;
        report.changed = true;
      } else {
        refused.push(sent.id);
        report.rejected += 1;
      }
    }
  });
  return refused;
}

/** One whole sync: pull, push, and again while there is something to settle. */
export async function sync(store: Store, keys: Keys, remote: Remote): Promise<Report> {
  const report: Report = { pulled: 0, pushed: 0, conflicts: 0, rejected: 0, changed: false };
  const refused = new Set<string>();
  for (let round = 0; round < MAX_ROUNDS; round += 1) {
    for (;;) {
      const cursor = await store.read((tx) => tx.cursor());
      const page = await remote.pull(cursor, PULL_LIMIT);
      await applyPull(store, keys, page, report);
      if (!page.more) break;
    }
    const batch = await prepare(store, keys, refused);
    if (!batch) return report;
    const response = await remote.push(batch.request);
    for (const id of await applyPush(store, keys, batch, response, report)) refused.add(id);
  }
  // Still changing at the same moment as another device: the next sync carries on.
  return report;
}
