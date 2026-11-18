# Kandado

Kandado is a zero-knowledge password manager. Every client encrypts your data before it leaves the device, and the server only ever stores ciphertext it cannot read. Rust is the main language on every tier: the backend, the Windows desktop app, and the core of the browser extension (compiled to WebAssembly).

> **Status:** early development. See the [implementation plan](docs/implementation-plan/README.md) for the phases (P0 to P10) and what each one delivers.

## How it works

- **Identity is Google.** Signing in with Google proves who you are to the backend. It decrypts nothing.
- **Encryption is your master password.** It never leaves the device. Argon2id turns it into a master key that unwraps a random user key, which in turn protects vault keys, item keys and item content (XChaCha20-Poly1305 throughout, HPKE for sharing).
- **A 12-word recovery phrase** can also unwrap the user key, so a forgotten master password is recoverable. Losing both means the data is gone, by design.

Someone who takes over the Google account, or the entire database, gets only ciphertext. The full model is in [docs/architecture/security.md](docs/architecture/security.md).

## Features

Planned across the phases:

- Logins, cards, notes, identities, passwords, passkeys and email aliases
- Multiple vaults, search, trash, item history
- Multi-device sync with realtime updates and offline support
- Browser extension for Chrome, Edge and Firefox with autofill, save and update, and optional unlock through the desktop app
- Vault sharing, single-item sharing and secure links
- Monitor: breached, weak and reused passwords, missing 2FA, email monitoring
- Import from other password managers, and export
- Windows Hello unlock, auto-lock, tray, global shortcuts

Mobile is deferred; the shared crates and the API are designed so it needs no server changes.

## Architecture

```
┌────────────────┐  HTTPS + WSS  ┌──────────────────────┐      ┌────────────┐
│ Desktop app    │◄─────────────►│ kandado-server       │◄────►│ PostgreSQL │
│ (GPUI, SQLite) │               │ (axum)               │      └────────────┘
└───────▲────────┘               └───▲──────────────────┘
        │ named pipe                 │ HTTPS + WSS
┌───────┴────────┐           ┌───────┴──────────────┐
│ native host    │◄─────────►│ Browser extension    │
└────────────────┘  native   │ (MV3, WASM core)     │
                    messaging└──────────────────────┘
```

| Tier | Stack |
| --- | --- |
| Backend | Rust, axum, tokio, sqlx, PostgreSQL |
| Desktop (Windows) | Rust, GPUI, Win32 via the `windows` crate, SQLite |
| Browser extension | Manifest V3, TypeScript + React 18, Rust core compiled to WebAssembly |
| Shared | `kandado-crypto`, `kandado-model`, `kandado-protocol` (no I/O, no platform APIs) |

More detail in [docs/architecture/](docs/architecture/README.md).

## Repository layout

```
kandado/
├─ Cargo.toml              # one Cargo workspace for every Rust crate
├─ package.json            # pnpm workspace root: scripts, Biome
├─ docs/                   # design system, architecture, implementation plan
├─ scripts/                # build, token/icon/UI sync, checks
├─ .github/workflows/      # CI
└─ src/
   ├─ shared/              # kandado-crypto, kandado-model, kandado-protocol
   ├─ backend/             # kandado-server, migrations, docker-compose.yml
   ├─ desktop/             # kandado-vault, kandado-bridge, kandado-app, host (GPUI)
   └─ extensions/          # wasm core, extension source, per-browser manifests
```

Tiers never depend on each other directly; they meet only through `src/shared/`.

## Getting started

### Prerequisites

- Rust 1.100.0 (pinned in `rust-toolchain.toml`; `rustup` installs it automatically, with the `wasm32-unknown-unknown` target)
- Node.js 24 or later and pnpm
- Docker, for the local PostgreSQL
- The wasm-bindgen CLI, for the extension:

```powershell
cargo install wasm-bindgen-cli --version 0.2.130 --locked
```

### Install JavaScript dependencies

```powershell
pnpm install
```

### Run the backend

Copy the example environment file, start PostgreSQL, then run the server (it applies migrations and serves on `127.0.0.1:8080`):

```powershell
Copy-Item .env.example .env
```

```powershell
docker compose -f src/backend/docker-compose.yml up -d
```

```powershell
cargo run -p kandado-server
```

`KANDADO_DEV_AUTH=1` enables a development-only sign-in route that stands in for Google (debug builds only). To use real Google sign-in, set `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` in `.env`.

### Run the desktop app

```powershell
cargo run -p kandado
```

The component gallery, which mirrors the design system previews:

```powershell
cargo run -p kandado --example gallery
```

### Build the browser extension

```powershell
node scripts/build-extension.mjs
```

Output goes to `src/extensions/dist/{chrome,edge,firefox}`; load that folder as an unpacked extension. Use `--dev` for a debug build, `--browser=chrome` for a single browser and `--server=URL` to point at a different API.

## Checks

`pnpm check` runs everything CI runs, in the same order: Biome, the generated-file checks, extension type check and unit tests, then `cargo fmt`, `cargo clippy` and `cargo test`.

```powershell
pnpm check
```

Skip a half with `--no-rust` or `--no-js`. Other useful scripts:

| Command | What it does |
| --- | --- |
| `pnpm lint` | Biome lint and format check |
| `pnpm format` | Format with Biome |
| `pnpm sync:tokens` / `sync:icons` / `sync:ui` | Copy design tokens, icons and UI assets from the design system into `src/` |
| `pnpm --filter @kandado/extension e2e` | Build the extension and run the Playwright end-to-end tests |

## Documentation

| Folder | Answers |
| --- | --- |
| [docs/design-system/](docs/design-system/project/README.md) | What Kandado looks like and says: tokens, components, screens |
| [docs/architecture/](docs/architecture/README.md) | How Kandado is built: crates, key hierarchy, API, schema |
| [docs/implementation-plan/](docs/implementation-plan/README.md) | In what order it is built: phases, exit criteria, open decisions |

## License

Kandado is licensed under the [GNU Affero General Public License v3.0 or later](LICENSE).
