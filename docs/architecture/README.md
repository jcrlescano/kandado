# Architecture overview

Kandado is a zero-knowledge password manager: clients encrypt everything before it leaves the device, and the backend stores ciphertext it cannot read. Rust is the main language on every tier.

| Tier | Stack | Doc |
| --- | --- | --- |
| Backend | Rust, axum, tokio, sqlx, PostgreSQL | [backend.md](backend.md) |
| Desktop (Windows) | Rust, GPUI Kit, Win32 via the `windows` crate, SQLite | [desktop.md](desktop.md) |
| Browser extension | Manifest V3, TypeScript + React 18 UI, Rust core compiled to WebAssembly | [extensions.md](extensions.md) |
| Mobile | Deferred. Will reuse the shared Rust crates | — |
| Keys and auth | Google sign-in for identity, master password for encryption | [security.md](security.md) |

## System diagram

```
                      ┌──────────────────────────────┐
                      │ Google (OpenID Connect)      │
                      └──────────────▲───────────────┘
                                     │ auth code flow (server side)
┌────────────────┐  HTTPS + WSS  ┌───┴──────────────────┐      ┌────────────┐
│ Desktop app    │◄─────────────►│ kandado-server       │◄────►│ PostgreSQL │
│ (GPUI, SQLite) │               │ (axum)               │      └────────────┘
└───────▲────────┘               │  REST /v1, events WS │
        │ named pipe             │  jobs, mail, links   │
┌───────┴────────┐               └───▲──────────────────┘
│ native host    │                   │ HTTPS + WSS
└───────▲────────┘                   │
        │ native messaging   ┌───────┴──────────────┐
        └───────────────────►│ Browser extension    │
                             │ (MV3, WASM core)     │
                             └──────────────────────┘
```

The extension works on its own (it syncs with the backend directly). The link to the desktop app is an optional convenience for unlocking (E-02 "Unlock with desktop app").

## Repository layout

```
kandado/
├─ Cargo.toml                 # one Cargo workspace for every Rust crate
├─ package.json               # pnpm workspace root: scripts, Biome
├─ pnpm-workspace.yaml
├─ docs/                      # see docs/README.md
├─ scripts/                   # build, token sync, release, generated-code checks
├─ .github/workflows/         # check.yml, release.yml
└─ src/
   ├─ shared/                 # Rust crates used by more than one tier
   │  ├─ kandado-crypto/      # key hierarchy, AEAD, KDF, recovery phrase
   │  ├─ kandado-model/       # item kinds, encode/decode, generator, TOTP, strength, URL matching
   │  └─ kandado-protocol/    # API request and response types (serde)
   ├─ backend/
   │  ├─ kandado-server/      # axum binary
   │  ├─ migrations/          # sqlx migrations
   │  └─ docker-compose.yml   # local PostgreSQL
   ├─ desktop/
   │  ├─ crates/
   │  │  ├─ kandado-vault/    # local store, sync engine, vault operations (no UI)
   │  │  ├─ kandado-bridge/   # local IPC for the extension + native messaging host
   │  │  └─ kandado-app/      # application services for the host
   │  ├─ host/                # GPUI views, windows, tray, hotkeys, updater (package `kandado`)
   │  ├─ tools/update-signer/
   │  ├─ packaging/windows/   # NSIS installer
   │  ├─ resources/           # fonts, icons, synced tokens
   │  └─ locales/
   ├─ extensions/
   │  ├─ wasm/                # kandado-wasm: wasm-bindgen facade over the shared crates
   │  ├─ src/                 # background, content, popup, pages
   │  └─ manifests/           # chrome, edge, firefox
   └─ mobile/                 # empty until mobile starts
```

`src/shared/` is an addition to the four folders originally named (backend, desktop, extensions, mobile). It exists because the crypto, item model and API types must be identical on the server, the desktop app and the extension, and belong to none of them. See [decisions.md](../implementation-plan/decisions.md).

## Dependency rules

```
kandado-crypto ◄── kandado-model ◄── kandado-protocol
       ▲                 ▲                  ▲
       │                 │                  ├── kandado-server      (backend)
       │                 │                  ├── kandado-vault ◄── kandado-app ◄── host   (desktop)
       │                 │                  │         ▲
       │                 │                  │   kandado-bridge
       └─────────────────┴──────────────────┴── kandado-wasm        (extension)
```

- Shared crates have no I/O, no async runtime and no platform APIs. They compile to `x86_64-pc-windows-msvc`, Linux (server) and `wasm32-unknown-unknown`.
- `kandado-server` never depends on `kandado-crypto` for decryption. It uses `kandado-protocol` types and treats every encrypted field as opaque bytes.
- Nothing in `src/desktop/crates/` depends on GPUI. Only `host` does.
- No crate depends on another tier's crate. Tiers meet only through `src/shared/`.

## Desktop building blocks

| Path | Role |
| --- | --- |
| `src/desktop/crates/kandado-vault` | UI-independent domain layer |
| `src/desktop/crates/kandado-bridge` (unprivileged; pipe, protocol, native host) | Local IPC |
| `src/desktop/crates/kandado-app` | Application services shared by the host |
| `src/desktop/host` (package `kandado`) | GPUI views, windows, tray, hotkeys, updater |
| `src/desktop/tools/update-signer` | Minisign update signing |
| `src/desktop/packaging/windows` | Installer |
| `scripts/`, pnpm, Biome | Build orchestration and lint |
| `settings.json`, `preferences.json`, an encrypted SQLite vault | Local state |
| `.github/workflows/check.yml`, `release.yml` | CI |

## Toolchain

- Rust stable, edition 2024, MSVC target on Windows. Pin `rust-version` in the workspace when it is scaffolded.
- Node and pnpm for scripts, the extension build and Biome.
- Release profile: `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`.
- Exact crate versions are pinned when each crate is scaffolded, not in these docs.
