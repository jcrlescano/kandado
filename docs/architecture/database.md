# Database

Kandado keeps data in three places: PostgreSQL on the server, an SQLite file on the desktop and IndexedDB in the browser extension. All three hold the same thing, ciphertext plus the plain metadata needed to order, authorise and merge changes. None of them can decrypt what they store. The keys exist only in memory on an unlocked client (see [security.md](security.md)).

```
            ┌───────────────────────── PostgreSQL (server) ─────────────────────────┐
            │ accounts · wrapped keys · devices · vaults · items · item_revisions   │
            │ one global `sync_revision` sequence orders every synced row           │
            └──────────────▲─────────────────────────────────────▲──────────────────┘
                           │ GET/POST /v1/sync (cursor)          │
   ┌───────────────────────┴──────────┐           ┌──────────────┴───────────────────┐
   │ SQLite %APPDATA%\Kandado\vault.db│           │ IndexedDB  "kandado"             │
   │ desktop (kandado-vault::store)    │           │ extension (background/store.ts)  │
   └───────────────────────────────────┘           └──────────────────────────────────┘
```

## What the server can and cannot see

| Readable by the server | Opaque bytes |
| --- | --- |
| IDs, foreign keys, `role`, item `kind`, revisions, timestamps, tombstone flags, email address, display name, device name and platform | Every `*_enc`, `*_wrapped`, `*_sealed` and `user_key_*` column: vault names, item content, item and vault keys, private keys |

`kind` on `items` is plaintext on purpose, so the server can count and filter without reading content (decision 11 in [decisions.md](../implementation-plan/decisions.md)). Anything else that would leak content stays inside `content_enc`.

## PostgreSQL

### Setup

- PostgreSQL 18 or later. [0001_baseline.sql](../../src/backend/migrations/0001_baseline.sql) refuses to run on anything older, because every ID defaults to the built-in `uuidv7()`.
- Access is through sqlx 0.9 with a `PgPool` (`KANDADO_DB_MAX_CONNECTIONS`, default 10, 5 s acquire timeout). `DATABASE_URL` is required and is never logged.
- Migrations live in [src/backend/migrations/](../../src/backend/migrations/) and are embedded with `sqlx::migrate!`. The server applies them on start, before it accepts connections.
- Queries live in `kandado-server/src/db/`, one module per group of tables (`users`, `keys`, `devices`, `flows`, `sync`). Every query is parameterised with `.bind()`; nothing is built by concatenating input into SQL.
- Locally, `docker compose -f src/backend/docker-compose.yml up -d` starts PostgreSQL 18 on `127.0.0.1:54329` (user, password and database all `kandado`). Not for production.

### Migrations

| File | Phase | Adds |
| --- | --- | --- |
| `0001_baseline.sql` | 0 | Version check only |
| `0002_accounts.sql` | 1 | `users`, `oauth_identities`, `user_keys`, `devices`, `auth_flows`, `auth_handoffs`, `audit_events` |
| `0003_sync.sql` | 3 | `sync_revision` sequence, `vaults`, `vault_members`, `items`, `item_revisions` |

Migrations are append-only. A shipped migration is never edited; a change is a new file with the next number.

### Tables in place

**Accounts and sign-in**

| Table | Holds | Notes |
| --- | --- | --- |
| `users` | One account | `email` is unique among live accounts (`lower(email)` where `deleted_at IS NULL`). `plan` defaults to `free` |
| `oauth_identities` | Which Google subject belongs to which user | Primary key (`provider`, `subject`), so another provider needs no change to `users` |
| `user_keys` | The wrapped user key and KDF parameters | One row per user: `kdf_salt`, `kdf_memory_kib`, `kdf_iterations`, `kdf_parallelism`, `user_key_by_master`, `user_key_by_recovery`, `public_x25519`, `private_keys_enc`, `version`. Ed25519 joins in phase 6 |
| `devices` | A signed-in session (D-49) | Refresh tokens stored as SHA-256 hashes. `previous_refresh_token_hash` lets the server spot a reused token and revoke the device. `authenticated_at` drives step-up checks |
| `auth_flows` | A Google round trip between `/start` and `/callback` | Keyed by a hash of `state`. Taken with `DELETE … RETURNING`, so each is used once. 10 minutes |
| `auth_handoffs` | Single-use codes the client trades for tokens | Keyed by a hash of the code, PKCE `challenge` checked. 60 seconds |
| `audit_events` | Security log | `bigint` identity key, `kind` text, optional `detail` jsonb. Indexed by (`user_id`, `at DESC`) |

**Vaults and items**

| Table | Holds | Notes |
| --- | --- | --- |
| `vaults` | A vault | `meta_enc` (name, colour, icon), `revision`, `deleted_at`. ID is chosen by the client |
| `vault_members` | Who may use a vault, and how | Primary key (`vault_id`, `user_id`). `role` is `viewer`, `editor`, `manager` or `owner`. `vault_key_sealed` is the vault key sealed to that member |
| `items` | The current version of each item | `kind`, `item_key_wrapped`, `content_enc`, `version_id`, `revision`. `created_at`, `updated_at`, `trashed_at` are the device's Unix seconds; `trashed_on` and `deleted_at` are the server's clock |
| `item_revisions` | Every saved version, current one included (D-16) | Primary key (`item_id`, `version_id`). `author_id`, `saved_at` |

All user-owned rows cascade from `users`, so deleting an account (`DELETE FROM users`) removes everything it owns in one statement.

### Planned tables

From the backend outline; columns are settled when each migration is written.

| Table | Phase | Purpose |
| --- | --- | --- |
| `vault_invites` | 6 | Pending invites: `vault_id`, `email`, `role`, `invited_by`, `state`, `expires_at` |
| `item_shares` | 6 | Single-item shares: `item_id`, `user_id`, `role`, `item_key_sealed` |
| `secure_links` | 6 | Links (D-33): `item_id`, `owner_id`, `payload_enc`, `expires_at`, `max_views`, `views`, `revoked_at` |
| `monitored_emails`, `breaches`, `user_breaches` | 7 | Email monitoring and breach matches (D-36) |
| `aliases`, `mailboxes` | 9 | Email aliases and their forward targets |

### Authorisation in SQL

The server does not load a row and then decide whether the caller may see it. The rule is in the query:

- Every read joins through `vault_members` on the calling user (`JOIN vault_members m ON m.vault_id = … AND m.user_id = $1`). A row that is not the caller's looks exactly like one that does not exist.
- Every write first reads the caller's role in the vault it touches. Viewer reads; editor writes items; manager also renames the vault, invites and changes roles; owner also deletes the vault.
- Moving an item checks the role in both the old and the new vault.
- An item may not change `kind` after it is created.
- Sharing (phase 6) only changes who is in `vault_members` (and, later, `item_shares`). The queries stay the same.

### Revisions and the sync cursor

Sync works because every row that syncs carries a `revision` from one global sequence, `sync_revision`. A row's `revision` is the value it was last written with, so "everything after cursor N" is just `revision > N`. The API side is in [backend.md](backend.md#sync).

```
push (one device, one batch)
  BEGIN
  pg_advisory_xact_lock(1, hashtext(user_id))   -- one writer per account at a time
  for each vault, then each item:
      SELECT … FOR UPDATE                       -- the current row, locked
      check role, size (MAX_BLOB), kind
      base_revision ≠ current revision → Conflict { current }
      otherwise nextval('sync_revision')       -- drawn inside the lock
                → UPDATE / INSERT, keep version
  pg_notify('kandado_sync', user_id)            -- only if something applied
  COMMIT                                        -- notification sent here
```

- **Ordering.** Revisions are drawn while holding the per-user advisory lock, so two devices of the same account can never commit revisions out of order. Without this, a pull could move its cursor past a revision that commits a moment later and miss it.
- **Batches are atomic.** A push is one transaction. Each change is answered on its own: `Applied { revision }`, `Conflict { current }` or `Rejected`.
- **Creating.** A change with `revision = 0` for an ID the server has never seen is an insert. Creating a vault also inserts the creator as `owner` in `vault_members`.
- **Pulling.** `pull` reads vaults and items with `revision > cursor`, merges them by revision, cuts at the limit and returns the last revision as the next cursor, plus `more` when there is another page.
- **Versions.** When `version_id` changes, the new content is inserted into `item_revisions` (`ON CONFLICT DO NOTHING`, so a retry is harmless). Moving or trashing without editing keeps the `version_id` and adds no version.

### Trash and tombstones

- Trashing sets `trashed_at` (device clock) and `trashed_on` (server clock, set the first time only). Restoring clears both.
- Deleting for good turns the row into a tombstone: `content_enc` and `item_key_wrapped` become empty, `deleted_at` is set, a new revision is drawn and every `item_revisions` row for the item is deleted. The row itself stays so other devices learn of the deletion through `revision > cursor`.
- A tombstone beats any edit: a push against a deleted item always comes back as a conflict.
- The trash purge job runs hourly and tombstones items whose `trashed_on` is more than 30 days old. It works one user at a time under that user's advisory lock, so its revisions stay ordered and it is safe on several server instances at once. It reads `trashed_on`, never a device clock.

### Notifications

A push that applies at least one change calls `pg_notify('kandado_sync', user_id)` inside its transaction, so the notification is delivered on commit and never for a write that rolled back. Each server process keeps one `PgListener` on that channel and wakes the WebSocket connections of that user. The message carries no data; the device pulls, and the pull is authorised like any other. PostgreSQL delivers to every listening instance, so this works with more than one server.

### Short-lived rows

`auth_flows` and `auth_handoffs` are consumed with `DELETE … WHERE expires_at > now() RETURNING …`, which reads and removes in one statement so a code can never be used twice. Expired rows are harmless (every read checks `expires_at`) and are removed by `db::flows::purge_expired`, which the daily session cleanup will call; that job is not scheduled yet (see [backend.md](backend.md#jobs)).

### Tests

Server tests use `#[sqlx::test(migrations = "../migrations")]`: each test gets its own empty database with every migration applied. They need PostgreSQL 18 reachable at `DATABASE_URL`.

## SQLite (desktop)

The desktop app's local copy lives in `%APPDATA%\Kandado\vault.db`, opened by `kandado-vault::store::Store` through `rusqlite` with SQLite bundled. It lets the app unlock and work fully offline.

### Settings

- `journal_mode = WAL`, `synchronous = FULL`, `foreign_keys = ON`.
- One connection, owned by `Store`. `Store::atomically` wraps work in a transaction; nested calls join the outer one, so a large operation such as an import can be built from small ones and still commit or roll back as a whole.
- IDs are stored as 16-byte blobs. Tables are `WITHOUT ROWID` where the key is a UUID.

### Tables

| Table | Holds |
| --- | --- |
| `account` | One row: user and device ID, email, display name and the same wrapped keys and KDF parameters as `user_keys`, so unlocking works offline |
| `device_wrap` | One row, optional: the user key wrapped for this device (`method` is `hello` or `dpapi`) for Windows Hello unlock |
| `vaults` | `key_enc`, `meta_enc`, timestamps, `server_revision`, `dirty` |
| `items` | `key_enc`, `content_enc`, `version_id`, timestamps, `trashed_at`, `server_revision`, `dirty`, and the merge base (`base_digest`, `base_vault_id`, `base_trashed_at`). Indexed by `vault_id` |
| `item_revisions` | Earlier versions: `local_id`, `item_id`, `version_id`, `content_enc`, `saved_at`. Unique (`item_id`, `version_id`) |
| `tombstones` | Items deleted here that the server still has: `item_id`, `vault_id`, `server_revision` |
| `sync_state` | One row: the cursor, how far this computer has read the server's changes |

There is no separate outbox. A row waiting to be sent is marked `dirty = 1`; a deletion waiting to be sent is a row in `tombstones`. `server_revision` is the `base_revision` sent with the change, and 0 means the server has never seen the row.

The merge base records what the server had at the last sync (a SHA-256 of `content_enc`, the vault and the trash state). With it, sync can tell which side changed what and do a three-way merge instead of overwriting.

### Schema versions

The schema version is `PRAGMA user_version`, upgraded step by step in `Store::init`:

| Version | Phase | Change |
| --- | --- | --- |
| 1 | 1 | `account`, `device_wrap` |
| 2 | 2 | `vaults`, `items`, `item_revisions` (local only) |
| 3 | 3 | Rebuilds the three for sync, keeping existing rows: each gets a random `version_id` and is marked dirty. Adds `tombstones` and `sync_state` |

A file newer than the app knows is refused with `StoreError::Corrupt` rather than read wrongly. Rebuilding a table is done in one transaction with `defer_foreign_keys` on.

### Sign-out

`Store::wipe` closes the connection and deletes `vault.db` with its `-wal`, `-shm` and `-journal` files (D-52). Nothing about the account stays on the computer.

## IndexedDB (extension)

The extension keeps its own copy in an IndexedDB database named `kandado` ([background/store.ts](../../src/extensions/src/background/store.ts)), so it works without the desktop app. It mirrors the desktop store in object stores:

| Store | Key | Holds |
| --- | --- | --- |
| `vaults` | `id` | `key_sealed`, `meta_enc`, `revision`, `dirty` |
| `items` | `id` | `key_wrapped`, `content_enc`, `digest`, `version_id`, timestamps, `revision`, `dirty`, merge base |
| `tombstones` | `item_id` | Deletions waiting to be sent |
| `kv` | string | Small values: the sync `cursor` and the signed-in account (IDs, email, and the wrapped keys as JSON) |

`Store.atomically` runs a body in one transaction over all four stores; if it throws, nothing is kept. Every value awaited inside must be an IndexedDB request, or the browser ends the transaction early. The schema version is the IndexedDB database version (currently 1).

## Rules for changing the schema

1. Server: add a new numbered file under `src/backend/migrations/`. Never edit a shipped one.
2. Desktop: bump `SCHEMA_VERSION` in `store.rs` and add an `if version < N` step that keeps existing rows.
3. Extension: bump `VERSION` in `store.ts` and handle the old version in `onupgradeneeded`.
4. A new encrypted column ends in `_enc`, `_wrapped` or `_sealed` and is `bytea` / `BLOB`. A new plaintext column needs a reason the server must read it, recorded in [decisions.md](../implementation-plan/decisions.md).
5. A new query that returns user data joins through `vault_members` for the caller.
6. A new synced table draws its `revision` from `sync_revision` inside the per-user lock.
7. Update this page in the same change.
