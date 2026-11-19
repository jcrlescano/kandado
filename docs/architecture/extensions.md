# Browser extension

A Manifest V3 extension for Chrome and Edge first, Firefox second, Safari later. It is a full client: it signs in, syncs and decrypts on its own. The desktop app is optional.

## Why not all Rust here

The extension's surfaces are web pages, and the design system already ships its components as a React 18 bundle. So the UI is TypeScript and React, reusing those components directly, and everything that must match the other clients byte for byte (crypto, item encoding, generator, TOTP, strength, URL matching) is the shared Rust code compiled to WebAssembly.

## Layout

```
src/extensions/
├─ wasm/                    # crate kandado-wasm: wasm-bindgen facade over kandado-crypto and kandado-model
├─ manifests/               # chrome.json, edge.json, firefox.json (merged with a base at build)
├─ src/
│  ├─ background/           # service worker
│  │  ├─ session.ts         # unlocked keys, auto-lock, PIN (memory only)
│  │  ├─ store.ts           # IndexedDB: encrypted vaults, items, outbox, cursor
│  │  ├─ sync.ts            # same protocol as the desktop sync engine
│  │  ├─ api.ts             # typed client generated from kandado-protocol
│  │  ├─ auth.ts            # Google sign-in through chrome.identity.launchWebAuthFlow
│  │  ├─ matcher.ts         # which items fit this tab's origin
│  │  ├─ autofill.ts        # fill requests, save and update detection
│  │  ├─ passkeys.ts        # WebAuthn create and get, backed by vault passkeys
│  │  ├─ native.ts          # native messaging to the desktop app
│  │  └─ router.ts          # message routing with sender checks
│  ├─ content/
│  │  ├─ detect.ts          # find login, sign-up, card, address, email and code fields
│  │  ├─ field-icon.ts      # the Kandado mark inside focused fields
│  │  ├─ dropdown/          # E-16 … E-22, in an extension-origin iframe
│  │  ├─ prompts/           # E-23 … E-26, top-right banner iframe
│  │  ├─ fill.ts            # writes values, fires the events frameworks expect
│  │  └─ webauthn-main.ts   # main-world shim for navigator.credentials
│  ├─ popup/                # E-01 … E-15 (380 × 600)
│  ├─ pages/
│  │  ├─ welcome/           # E-27
│  │  └─ settings/          # E-28 … E-31
│  └─ ui/                   # design system bundle, tokens, shared hooks
├─ package.json             # Vite, TypeScript, React 18
└─ tests/                   # Playwright against fixture pages
```

## Runtime pieces

| Piece | Lives | Holds secrets | Talks to |
| --- | --- | --- | --- |
| Service worker | Background | Yes, in memory and `chrome.storage.session` | Backend, native host, every other piece |
| Popup and pages | Extension origin | Only what is on screen | Service worker |
| Dropdown and prompt iframes | Extension origin, inside the page | Titles and usernames to list; never passwords | Service worker |
| Content script | Page, isolated world | Receives a value only at the moment of fill | Service worker |
| WebAuthn shim | Page, main world | Nothing | Content script |

MV3 service workers are stopped when idle. The unlocked key is kept in `chrome.storage.session` (memory-backed, cleared when the browser closes) so a restarted worker stays unlocked until the auto-lock time passes.

## Autofill rules

- Match by registrable domain by default; each login can be set to exact host, or to never fill.
- Fill only after the user picks an item in the dropdown or popup, never on page load.
- Never fill a cross-origin iframe unless the item's website matches that iframe's origin.
- Never fill over HTTP when the item was saved for HTTPS, without a warning.
- Excluded sites (E-30) get no icon, no dropdown and no save prompt.
- Save (E-23) and update (E-24) prompts come from watching form submission and navigation; the captured password is held in the service worker for a short time and dropped if the user ignores the prompt.

## Passkeys

`webauthn-main.ts` wraps `navigator.credentials.create` and `.get` in the top frame's main world (registered for `document_start`). It turns the options into plain values and posts them to the content script, which asks the worker (`passkey-start`). The worker (`background/passkeys.ts`) decides at once whether Kandado answers; if not (an iframe, a conditional request, passkeys off in E-29, an excluded site, no ES256, or no stored passkey that fits a sign-in) the shim calls the browser's own function with the page's original options. Otherwise the prompt iframe shows E-25 or E-26 and the answer comes back as `kandado-passkey-outcome`. The origin is always the one the browser reports for the frame.

The WASM core makes the credential (ES256, "none" attestation) and signs with it (`kandado_model::webauthn`, `kandado_crypto::passkey`). The private key never reaches JavaScript: `itemContent` blanks it and `sealItemUpdate` refuses a passkey. "Use another device" hands the call to the browser; Cancel answers `NotAllowedError` (or `InvalidStateError` when the site already has the passkey); the page's `AbortSignal` closes the prompt. Details and limits are P8-1 to P8-10 in [decisions.md](../implementation-plan/decisions.md).

## Desktop link

Native messaging to `kandado-native-host` (see [desktop.md](desktop.md#kandado-bridge-local-ipc)). Used for: detecting the desktop app, pairing, "Unlock with desktop app" (E-02), and locking both together. If the host is missing, those options are hidden and nothing else changes.

## Permissions

`storage`, `alarms`, `identity`, `nativeMessaging`, `scripting`, `activeTab`, `clipboardWrite`, `webNavigation`, and host access to all sites (needed for in-page autofill). Host access is requested on the welcome tab (E-27) with an explanation, not silently at install.

## Build and test

- `wasm-pack` builds `kandado-wasm`; Vite builds the rest; one output folder per browser.
- Unit tests in Vitest. Crypto vectors are the same files the Rust tests use.
- Playwright loads the built extension and runs autofill against local fixture pages (login, sign-up, change password, checkout, address, 2FA code, and a WebAuthn relying party that checks every answer with Web Crypto).
- Biome for lint and format, as the root workspace.
