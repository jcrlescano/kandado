# Desktop app (Windows)

A native Rust app drawn with GPUI Kit, in layers: a UI-independent domain crate, a local IPC crate, an application-services crate, and a GPUI host on top. Windows 10 22H2 or later, x64. macOS and Linux are designed for but not built yet.

## Layers

```
host  (package `kandado`)     GPUI views, windows, tray, hotkeys, updater, installer glue
  │
kandado-app                   session and lock, settings, clipboard, sync scheduling, import/export, monitor
  │
kandado-vault ── kandado-bridge
  │                 local IPC for the extension
src/shared: kandado-model, kandado-crypto, kandado-protocol
```

A layer may call only the layers below it. Only `host` knows GPUI. Only `kandado-app`, `kandado-bridge` and `host` call Win32.

### kandado-vault (domain, no UI)

| Module | Does |
| --- | --- |
| `store` | SQLite (`rusqlite`, bundled). Tables mirror the server's `vaults`, `items`, `item_revisions`, plus `outbox` and `sync_state`. Content stays encrypted at rest |
| `session` | Holds the unlocked user key and decrypted vault keys in memory; `lock()` drops them |
| `vaults`, `items` | Create, edit, move, trash, restore, delete, favourite, duplicate; writes go to the store and the outbox |
| `index` | In-memory search index over decrypted titles, usernames, URLs and notes; rebuilt on unlock |
| `sync` | Pull by cursor, push what is waiting, merge conflicts (three-way, per item) |
| `events` | The `/events` WebSocket: says when the server has something new |
| `api` | `reqwest` client for `kandado-protocol` |
| `history` | Revisions list and restore |
| `health` | Weak, reused and missing-2FA analysis; breached-password check by hash prefix |
| `import`, `export` | Parsers for Proton Pass, Bitwarden, 1Password, LastPass, browser CSV, generic CSV; encrypted, JSON and CSV export |

Phase 1 built `api`, `store` and `session`; phase 2 added `vaults`, `items`, `index` and `history`; phase 3 added `sync` and `events`; phase 4 added `import` (with the export formats); `health` arrives with phase 7.

### kandado-bridge (local IPC)

- `pipe`, `server`, `protocol`, `security`: a named-pipe server inside the desktop app, ACL-restricted to the current user.
- `bin/kandado-native-host`: the small executable browsers launch for native messaging. It relays length-prefixed JSON between the extension and the pipe.
- `install`: writes and removes the native messaging manifests and registry keys for Chrome, Edge and Firefox (driven from D-45).
- Messages: `status`, `pair`, `unlock`, `unpair`, and the events `locked` and `unlocked` from the app. The bridge never serves item data; the extension has its own vault copy. An unlock carries the user key sealed to the paired extension (`kandado_crypto::pairing`; decisions P5-25 to P5-29).

### kandado-app (application services)

| Module | Does |
| --- | --- |
| `auth` | Google sign-in: loopback listener, browser launch, token storage in Windows Credential Manager |
| `onboarding` | Key setup, recovery phrase generation and file save |
| `lock` | Auto-lock timer, lock on sleep and session switch, Windows Hello |
| `clipboard` | Copy with timed clear, excluded from clipboard history |
| `settings` | `settings.json` (app settings) and `preferences.json` (window position, pane widths, last vault) |
| `sync` | When to sync: on unlock, on change, on event, on a timer, with backoff; offline state for D-54 |
| `totp_import` | Parse `otpauth://`, decode a QR code from a screen capture (D-20) |
| `tasks` | Background work queue so the UI thread never blocks |
| `browser` | The extension link (D-45): pairing, unlock, lock events, host registration |

### host (GPUI)

```
src/desktop/host/src/
├─ main.rs, lib.rs
├─ instance.rs          # single instance; second launch focuses the window
├─ shell/               # window, custom title bar, three-pane layout, rail collapse
├─ theme/               # generated tokens, Vault and Paper themes, fonts
├─ components/          # the design system parts, in GPUI
├─ views/
│  ├─ start/            # D-01 … D-05
│  ├─ items/            # list, details, forms, search, multi-select, history
│  ├─ vaults/           # create, settings, sharing dialogs
│  ├─ sharing/          # share item, secure links
│  ├─ monitor/          # D-35 … D-40
│  ├─ trash/
│  ├─ settings/         # D-43 … D-50
│  └─ system/           # dialogs, banners, toasts, shortcuts
├─ commands.rs, dispatch.rs   # user actions → kandado-app calls
├─ hotkey.rs            # Ctrl+F, Ctrl+N, Ctrl+L, Ctrl+C on a row, global show hotkey
├─ tray.rs              # D-53
├─ updater.rs, update_transport.rs, installer.rs
├─ window_state.rs
└─ logging.rs
```

## State and data flow

1. A view dispatches a command (`CreateItem`, `CopyPassword`, `Lock`).
2. `dispatch` runs it on a background task through `kandado-app`.
3. `kandado-vault` updates the store, the index and the outbox, and emits a change event.
4. Views subscribed to that part of the state re-render. The UI holds view models (titles, subtitles, masked values); it asks for a secret only at the moment of reveal or copy, and drops it afterwards.

## Design tokens

- `scripts/sync-tokens.mjs` copies `docs/design-system/project/tokens.json` to `src/desktop/resources/tokens.json` and to the extension. `pnpm check:generated` fails CI if the copies drift.
- `host/build.rs` turns the copied file into Rust constants (colours for both themes, type styles, spacing, radii, shadows). No view hard-codes a colour or a size.
- Fonts (Bricolage Grotesque, Figtree, JetBrains Mono) are bundled in `resources/fonts/`.

## Components

Each design system component gets a GPUI counterpart with the same name and props, built once in `host/src/components/` before any screen: `Button`, `IconButton`, `TextField`, `Toggle`, `Badge`, `SearchBar`, `VaultChip`, `ItemIcon`, `ItemRow`, `SecretField`, `StrengthMeter`, `PasswordGenerator`, `TotpCode`, `Toast`, `Logo`, `Icon`. Plus the layout pieces the desktop screens need: dialog, popover, dropdown menu, banner, settings navigation, table (D-33). A `host/examples/gallery.rs` window shows them all in both themes for comparison with the design system previews.

## Windows integration

| Feature | How |
| --- | --- |
| Custom title bar with centred search | GPUI client-side decorations; hit-testing for drag and snap layouts |
| Windows Hello | `Windows.Security.Credentials.KeyCredentialManager`; DPAPI fallback |
| Token storage | Credential Manager |
| Tray menu (D-53) | Shell notify icon; Open, Lock, Generate password, Quit |
| Lock on sleep or session switch | `WM_POWERBROADCAST`, `WM_WTSSESSION_CHANGE` |
| Clipboard | `ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory` formats; timed clear |
| Screen capture protection | `SetWindowDisplayAffinity` (setting in D-44) |
| QR from screen (D-20) | Screen capture of a chosen region, decoded locally |
| Single instance | Named mutex plus activation message |
| Updates (D-54) | Fetch manifest, download, verify Minisign signature and version, apply on restart |
| Installer | NSIS per-user install; registers the native messaging host; portable build too |

## Testing

- Shared crates and `kandado-vault`: unit tests, plus crypto test vectors shared with the WASM build so both produce identical bytes.
- `kandado-vault` sync: integration tests against a real `kandado-server` and PostgreSQL in CI.
- `host`: GPUI test support for view logic; the component gallery for visual review.
- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace`, `cargo audit` on every pull request.
