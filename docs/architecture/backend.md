# Backend

One Rust service, `kandado-server`, in front of PostgreSQL. It authenticates users, stores ciphertext, orders changes for sync, and runs the features that need a server: secure links, breach checks, devices.

## Stack

| Concern | Choice |
| --- | --- |
| HTTP | axum on tokio, tower-http (trace, compression, CORS, request limits) |
| Database | PostgreSQL through sqlx (compile-time checked queries, `sqlx migrate`) |
| Auth | `openidconnect` for Google; opaque rotating refresh tokens; short-lived signed access tokens |
| Realtime | WebSocket at `/v1/events` (PostgreSQL `LISTEN/NOTIFY` fan-out) |
| Email | None yet (decisions 14, 15); new-device notices would need a provider |
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
│     ├─ jobs/          # trash purge, link expiry
│     └─ mail/          # templates and sender
├─ migrations/
└─ docker-compose.yml
```

One crate with modules. Split it only if a second binary appears.

## API

All routes are under `/v1`, JSON, authenticated with a bearer access token unless marked public. Request and response types live in `kandado-protocol`. Encrypted fields are base64 strings the server never parses.

| Area | Routes | Used by screens |
| --- | --- | --- |
| Auth | `GET /auth/google/start` (public), `GET /auth/google/callback` (public), `POST /auth/token` (public), `POST /auth/refresh` (public), `POST /auth/logout` | D-01, D-52, E-01, E-13 |
| Account | `GET /me`, `POST /account/keys`, `GET /account/keys`, `PUT /account/keys/master`, `DELETE /account` | D-02, D-03, D-04, D-50 |
| Devices | `GET /devices`, `DELETE /devices/{id}`, `POST /devices/revoke-others` | D-49 |
| Sync | `GET /sync?cursor=`, `POST /sync` (batch of vault and item changes) | Every item screen, D-54 |
| Item history | `GET /items/{id}/revisions`, `GET /items/{id}/revisions/{rev}` | D-16 |
| Secure links | `POST /links`, `GET /links`, `DELETE /links/{id}`, `POST /public/links/{id}/open` (public; counts one view), `GET /l/{id}` and `GET /l/assets/{name}` (public viewer page, outside `/v1`) | D-32, D-33 |
| Monitor | `GET /monitor/pwned/{prefix}`, `GET /monitor/breaches`, `GET /monitor/2fa-sites` (relayed and cached from HIBP and the 2FA Directory) | D-35 to D-39, E-15 |
| Icons | `GET /icons/{domain}` (proxied and cached, so sites never see the user) | Item rows everywhere |
| Events | `GET /events` (WebSocket) | Sync |
| Updates | Static `latest.json` / `beta.json` plus installer, served from object storage | D-54 |

Trash, restore, move, favourite, create, edit and delete are all item state changes that travel through `/sync`. There are no separate endpoints for them.

## Sync

- Every vault and item row has a server-assigned, monotonically increasing `revision` (one global sequence).
- `GET /sync?cursor=N` returns everything the user can see with `revision > N`, including tombstones, and the next cursor.
- `POST /sync` sends a batch. Each change carries the `base_revision` it was made against. If the server row is newer, that change is rejected with the current row; the client merges and retries.
- Conflict policy: the part changed on both sides goes to the later writer, parts changed on one side are kept, and the losing version is kept as a revision, so nothing is lost (visible in D-16). A deletion is a tombstone and beats edits. Details in [decisions.md](../implementation-plan/decisions.md) (P3-3).
- After a successful write the server notifies other devices over `/events`, and they pull.
- Clients work fully offline against their local store and queue changes.

## Database schema

Tables, the revision sequence, authorisation in SQL, tombstones and migrations are in [database.md](database.md). In short: all IDs are UUIDv7, every `*_enc`, `*_wrapped` and `*_sealed` column is opaque bytes, and every query that returns user data joins through `vault_members` for the caller.

## Jobs

| Job | Schedule | Does |
| --- | --- | --- |
| Trash purge | Hourly | Hard-deletes items trashed more than 30 days ago, writes tombstones |
| Link expiry | Every 5 min | Drops the payloads of expired, used-up or revoked links; deletes links 30 days after they ended |
| Session cleanup | Daily | Removes revoked devices and used handoff codes |

## Limits and protection

Per-IP and per-user rate limits on auth, lookup and link routes; request body cap; strict CORS allow-list (extension origins only); constant-time token comparison; refresh-token reuse revokes the whole device.

## Local development

`docker compose up` starts PostgreSQL. `cargo run -p kandado-server` applies migrations and serves on `127.0.0.1:8080`. A development-only sign-in route (`GET /v1/dev/sign-in`, compiled out of release builds and off unless `KANDADO_DEV_AUTH=1`) stands in for Google in integration tests. `KANDADO_TOKEN_SECRET` (32 or more characters) signs access tokens; `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` turn on the real Google flow. Tests need PostgreSQL 18 (`DATABASE_URL`). The secure link viewer is built with `node scripts/build-viewer.mjs` into `src/backend/viewer/dist`, which the server reads at startup from `KANDADO_VIEWER_DIR` (that folder by default; empty turns it off). Without it the API runs and `/l/{id}` answers 503.

### Secure links

- The device chooses the link id (UUIDv7) and encrypts a snapshot of the item to it (`kandado_model::link`, `Purpose::SecureLink`). The key goes only into the URL fragment, which browsers never send; `CreateLink` has no field for it.
- Expiry is one of 1 hour, 1 day, 7 days, 30 days; the view limit one of 1, 5, 10 or none. The payload is at most 32 KiB. An account may have 500 working links.
- Opening is a `POST`, so a crawler or chat preview fetching `/l/{id}` uses no view. The viewer page also waits for "Show item" before opening. Every link that does not work any more answers 404, whatever the reason.
- The payload is dropped the moment a link stops working: on its last view (in the same transaction), on revoke, by the expiry job, and when its item is deleted for good. Trashing an item does not stop its links; the item can come back.
- Monitor (decision 15, P7-1): `/v1/monitor/pwned/{prefix}` relays Pwned Passwords with `Add-Padding`; `/v1/monitor/breaches` and `/v1/monitor/2fa-sites` relay Have I Been Pwned's breach list and the 2FA Directory's TOTP list, filtered. Answers are kept in memory (24 h and 12 h); a list is served up to a week old when its source is down. Devices match the lists locally, so the server never learns a person's sites.
- The viewer page is served with `Content-Security-Policy: default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; …`, `Referrer-Policy: no-referrer`, `Cache-Control: no-store` and `X-Frame-Options: DENY`. It removes the key from the address bar as soon as it has read it, and writes every value with `textContent`.

### Authentication details

- Access tokens are HMAC-SHA256 signed and live 15 minutes; every request also checks the device is not revoked, so revoking takes effect at once.
- Refresh tokens rotate on use. Presenting one that was already rotated away revokes the whole device (`refresh_token_reused` in the audit log).
- A sensitive route (change master password, sign out other devices, delete account) answers `403 step_up_required` unless the session's last Google sign-in is under 5 minutes old. The client runs the Google flow with `purpose=step_up` and the current access token, and keeps its session.
- Redirects after sign-in are checked exactly: `http://127.0.0.1:{port}/callback` for the desktop app, an allow-listed `https` host for an extension.
