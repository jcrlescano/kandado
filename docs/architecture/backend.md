# Backend

One Rust service, `kandado-server`, in front of PostgreSQL. It authenticates users, stores ciphertext, orders changes for sync, and runs the features that need a server: invitations, secure links, breach checks, devices.

## Stack

| Concern | Choice |
| --- | --- |
| HTTP | axum on tokio, tower-http (trace, compression, CORS, request limits) |
| Database | PostgreSQL through sqlx (compile-time checked queries, `sqlx migrate`) |
| Auth | `openidconnect` for Google; opaque rotating refresh tokens; short-lived signed access tokens |
| Realtime | WebSocket at `/v1/events` (PostgreSQL `LISTEN/NOTIFY` fan-out) |
| Email | SMTP through `lettre` (invitations, breach alerts, new-device notices) |
| Jobs | In-process tokio tasks with advisory locks |
| Config | Environment variables; `.env` for local development |
| Observability | `tracing` with JSON logs, `/healthz`, `/readyz` |

## Crate layout

```
src/backend/
├─ kandado-server/
│  └─ src/
│     ├─ main.rs        # config, pool, router, shutdown
│     ├─ config.rs
│     ├─ error.rs       # one error type → problem+json
│     ├─ auth/          # google flow, tokens, session extractor, step-up check
│     ├─ http/          # one module per API area below
│     ├─ db/            # sqlx queries, one module per table group
│     ├─ events.rs      # WebSocket hub
│     ├─ jobs/          # trash purge, link expiry, invite expiry, breach scan
│     └─ mail/          # templates and sender
├─ migrations/
└─ docker-compose.yml
```

One crate with modules. Split it only if a second binary appears (the alias mail service in phase 9 is the likely first).

## API

All routes are under `/v1`, JSON, authenticated with a bearer access token unless marked public. Request and response types live in `kandado-protocol`. Encrypted fields are base64 strings the server never parses.

| Area | Routes | Used by screens |
| --- | --- | --- |
| Auth | `GET /auth/google/start` (public), `GET /auth/google/callback` (public), `POST /auth/token` (public), `POST /auth/refresh` (public), `POST /auth/logout` | D-01, D-52, E-01, E-13 |
| Account | `GET /me`, `POST /account/keys`, `GET /account/keys`, `PUT /account/keys/master`, `DELETE /account` | D-02, D-03, D-04, D-50 |
| Devices | `GET /devices`, `DELETE /devices/{id}`, `POST /devices/revoke-others` | D-49 |
| Sync | `GET /sync?cursor=`, `POST /sync` (batch of vault and item changes) | Every item screen, D-54 |
| Item history | `GET /items/{id}/revisions`, `GET /items/{id}/revisions/{rev}` | D-16 |
| Vault members | `GET /vaults/{id}/members`, `PATCH /vaults/{id}/members/{user}`, `DELETE /vaults/{id}/members/{user}` | D-28 |
| Invitations | `POST /vaults/{id}/invites`, `GET /invites`, `POST /invites/{id}/accept`, `POST /invites/{id}/decline`, `POST /invites/{id}/resend`, `DELETE /invites/{id}` | D-29, D-30 |
| Item shares | `POST /items/{id}/shares`, `GET /items/{id}/shares`, `DELETE /items/{id}/shares/{user}` | D-31 |
| Public keys | `GET /users/lookup?email=` | D-29, D-31 |
| Secure links | `POST /links`, `GET /links`, `DELETE /links/{id}`, `GET /public/links/{id}` (public), `GET /l/{id}` (public viewer page) | D-32, D-33 |
| Monitor | `GET /monitor/breaches`, `POST /monitor/breaches/{id}/resolve`, `GET /monitor/pwned/{prefix}`, `GET /monitor/emails`, `POST /monitor/emails`, `PATCH /monitor/emails/{id}`, `GET /monitor/2fa-sites` | D-35 to D-40, E-15 |
| Aliases | `GET/POST /aliases`, `PATCH/DELETE /aliases/{id}`, `GET/POST /mailboxes`, `POST /mailboxes/{id}/verify`, `GET /alias-domains` | D-08, D-21, D-46, E-08, E-12, E-19 |
| Icons | `GET /icons/{domain}` (proxied and cached, so sites never see the user) | Item rows everywhere |
| Events | `GET /events` (WebSocket) | Sync, D-30 |
| Updates | Static `latest.json` / `beta.json` plus installer, served from object storage | D-54 |

Trash, restore, move, favourite, create, edit and delete are all item state changes that travel through `/sync`. There are no separate endpoints for them.

## Sync

- Every vault and item row has a server-assigned, monotonically increasing `revision` (one global sequence).
- `GET /sync?cursor=N` returns everything the user can see with `revision > N`, including tombstones and membership changes, and the next cursor.
- `POST /sync` sends a batch. Each change carries the `base_revision` it was made against. If the server row is newer, that change is rejected with the current row; the client merges and retries.
- Conflict policy: the part changed on both sides goes to the later writer, parts changed on one side are kept, and the losing version is kept as a revision, so nothing is lost (visible in D-16). A deletion is a tombstone and beats edits. Details in [decisions.md](../implementation-plan/decisions.md) (P3-3).
- After a successful write the server notifies other devices over `/events`, and they pull.
- Clients work fully offline against their local store and queue changes.

## Database schema

Outline only; columns are finalised in each migration. All IDs are UUIDv7. All `*_enc` and `*_wrapped` columns are opaque bytes.

| Table | Purpose | Key columns |
| --- | --- | --- |
| `users` | Account | `id`, `email`, `display_name`, `plan`, `created_at`, `deleted_at` |
| `oauth_identities` | Google link | `provider`, `subject`, `user_id`, unique (`provider`, `subject`) |
| `user_keys` | Wrapped keys | `user_id`, `kdf_salt`, `kdf_*` (memory, iterations, parallelism), `user_key_by_master`, `user_key_by_recovery`, `public_x25519`, `private_keys_enc`, `version` (Ed25519 joins later) |
| `devices` | Sessions (D-49) | `id`, `user_id`, `name`, `platform`, `refresh_token_hash`, `previous_refresh_token_hash`, `authenticated_at`, `last_seen_at`, `revoked_at` |
| `auth_handoffs` | Single-use sign-in codes | `code_hash`, `challenge`, `user_id`, `purpose`, `auth_time`, `expires_at` |
| `auth_flows` | Google round trip in progress (10 min) | `state_hash`, `nonce`, `pkce_verifier`, `client_challenge`, `redirect`, `purpose`, `user_id` |
| `vaults` | Vault | `id`, `owner_id`, `meta_enc` (name, colour, icon), `revision`, `deleted_at` |
| `vault_members` | Membership | `vault_id`, `user_id`, `role` (viewer, editor, manager, owner), `vault_key_sealed` |
| `vault_invites` | Pending invites | `id`, `vault_id`, `email`, `role`, `invited_by`, `state`, `expires_at` |
| `items` | Current item | `id`, `vault_id`, `kind`, `item_key_wrapped`, `content_enc`, `revision`, `trashed_at`, `deleted_at` |
| `item_revisions` | History (D-16) | `item_id`, `revision`, `content_enc`, `item_key_wrapped`, `author_id`, `created_at` |
| `item_shares` | Single-item shares | `item_id`, `user_id`, `role`, `item_key_sealed` |
| `secure_links` | Links (D-33) | `id`, `item_id`, `owner_id`, `payload_enc`, `expires_at`, `max_views`, `views`, `revoked_at` |
| `monitored_emails` | Email monitoring | `id`, `user_id`, `address`, `enabled` |
| `breaches` | Known breaches | `id`, `name`, `domain`, `occurred_on`, `data_classes` |
| `user_breaches` | Matches (D-36) | `user_id`, `breach_id`, `address`, `resolved_at` |
| `aliases` | Aliases | `id`, `user_id`, `address`, `mailbox_id`, `enabled`, counters |
| `mailboxes` | Forward targets | `id`, `user_id`, `address`, `verified_at` |
| `audit_events` | Security log | `user_id`, `device_id`, `kind`, `at`, `detail` |

`kind` on `items` is plaintext (login, alias, card, note, identity, password, passkey) so the server can count and filter without reading content (decided: decision 11 in [decisions.md](../implementation-plan/decisions.md)).

Authorisation is enforced in SQL: every query joins through `vault_members` or `item_shares` for the calling user. Role rules: viewer reads; editor writes items; manager also invites and changes roles; owner also deletes the vault.

## Jobs

| Job | Schedule | Does |
| --- | --- | --- |
| Trash purge | Hourly | Hard-deletes items trashed more than 30 days ago, writes tombstones |
| Link expiry | Every 5 min | Deletes payloads of expired, exhausted or revoked links |
| Invite expiry | Daily | Expires invites after 7 days |
| Session cleanup | Daily | Removes revoked devices and used handoff codes |
| Breach scan | Daily | Checks monitored emails against the breach source, creates `user_breaches`, sends email |

## Limits and protection

Per-IP and per-user rate limits on auth, lookup and link routes; request body cap; strict CORS allow-list (extension origins only); constant-time token comparison; refresh-token reuse revokes the whole device.

## Local development

`docker compose up` starts PostgreSQL. `cargo run -p kandado-server` applies migrations and serves on `127.0.0.1:8080`. A development-only sign-in route (`GET /v1/dev/sign-in`, compiled out of release builds and off unless `KANDADO_DEV_AUTH=1`) stands in for Google in integration tests. `KANDADO_TOKEN_SECRET` (32 or more characters) signs access tokens; `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` turn on the real Google flow. Tests need PostgreSQL 18 (`DATABASE_URL`).

### Authentication details

- Access tokens are HMAC-SHA256 signed and live 15 minutes; every request also checks the device is not revoked, so revoking takes effect at once.
- Refresh tokens rotate on use. Presenting one that was already rotated away revokes the whole device (`refresh_token_reused` in the audit log).
- A sensitive route (change master password, sign out other devices, delete account) answers `403 step_up_required` unless the session's last Google sign-in is under 5 minutes old. The client runs the Google flow with `purpose=step_up` and the current access token, and keeps its session.
- Redirects after sign-in are checked exactly: `http://127.0.0.1:{port}/callback` for the desktop app, an allow-listed `https` host for an extension.
