# Security model

Two separate things, kept separate on purpose:

- **Identity** is Google. It proves who you are to the backend. It decrypts nothing.
- **Encryption** is the master password. It never leaves the device and the backend never sees it or anything derived from it that could decrypt data.

Someone who takes over the Google account, or the whole database, gets ciphertext.

## Sign in with Google

Backend-mediated OpenID Connect, so the Google client secret stays on the server. Clients use a PKCE-style handoff.

1. The client creates a random `verifier` and sends its SHA-256 `challenge`.
2. Desktop opens the system browser at `GET /v1/auth/google/start?client=desktop&challenge=…&redirect=http://127.0.0.1:{port}/callback`, listening on a random loopback port. The extension uses `chrome.identity.launchWebAuthFlow` with its own redirect URL.
3. The server runs the Google authorization code flow, checks the ID token (`iss`, `aud`, `nonce`, `email_verified`), and finds or creates the user by Google `sub`.
4. The server redirects to the client with a single-use code (60 s lifetime).
5. The client calls `POST /v1/auth/token` with the code and the `verifier`, and receives an access token (15 min) and a refresh token (rotating, stored hashed, bound to a device row).

Identities live in an `oauth_identities` table keyed by provider, so another provider can be added without touching users.

Sensitive server actions (delete account, sign out other devices, change master password) need a fresh Google sign-in (`max_age`), checked server side.

## Key hierarchy

```
master password ──Argon2id(salt, params)──► master key ─┐
recovery phrase (12 words, BIP39) ──HKDF──► recovery key ─┼─ each wraps ─► user key (random, 256-bit)
device key (Windows Hello / DPAPI) ───────────────────────┘
                                                          user key encrypts ─► user private keys
                                                                               (X25519 for sharing; Ed25519 for signing is added with the first feature that signs)
vault key (random, per vault) ── sealed to each member's X25519 public key
item key (random, per item)   ── wrapped by the vault key
item content                  ── encrypted with the item key
```

- **AEAD:** XChaCha20-Poly1305 everywhere. Associated data binds each ciphertext to its owner ID, object ID and format version, so blobs cannot be swapped between rows.
- **Sealing to a public key** (vault keys to their owner's account key) is HPKE, RFC 9180, base mode: DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, ChaCha20-Poly1305. HPKE fixes its own AEAD and uses each key once, so this is the one place the cipher is not XChaCha20-Poly1305. The same associated data is bound.
- **Recovery key:** HKDF-SHA256 over the phrase's 128 bits of entropy.
- **KDF:** Argon2id, 64 MiB, 3 passes, 1 lane for new accounts. Clients refuse stored parameters below 19 MiB and 2 passes. Salt and parameters are stored per user so they can be raised later.
- **Per-item keys** make single-item sharing possible without exposing the vault key: the item key is sealed to the recipient.
- **Secure links:** the client encrypts a snapshot of the item with a fresh link key. The key travels only in the URL fragment, which browsers never send to the server. The server stores the ciphertext, expiry and view count.
- **Metadata the server can see:** user email, vault membership and roles, item count, sizes and timestamps, monitored email addresses. Item titles, URLs, usernames and vault names are inside the encrypted content.

Every key and blob format carries a version byte. `kandado-crypto` is the only crate that touches key material; secrets are held in zeroizing types and never implement `Debug` or `Serialize` in the clear.

## First run and new devices

| Situation | What happens |
| --- | --- |
| First sign-in ever | Google → create master password (D-02) → client generates user key, key pair and recovery phrase in memory → shows the recovery phrase (D-03) → on Continue, uploads the wrapped keys |
| New device, existing account | Google → download wrapped keys → enter master password (or recovery phrase) → unwrap user key |
| Forgot master password | Recovery phrase unwraps the user key → set a new master password → re-wrap |
| Lost both | Data cannot be recovered. The UI says so plainly at D-03 |
| Change master password | Re-wrap the user key only. No data is re-encrypted |

## Unlock

- **Desktop (D-04):** master password, or Windows Hello. Hello works through a device key: the user key is wrapped by a key held in the Windows credential store (`KeyCredentialManager`, DPAPI fallback). The Hello-wrapped copy is deleted on sign-out.
- **Extension (E-02):** master password on first unlock after the browser starts. Then a 6-digit PIN for the rest of the browser session: the PIN-wrapped key exists only in `chrome.storage.session` (memory), with 5 attempts before it is discarded. A PIN-wrapped key is never written to disk, because six digits cannot resist offline guessing.
- **Unlock with desktop app:** the extension asks the desktop app over native messaging. If the app is unlocked and the user approves, the app sends the user key over a channel encrypted with a key agreed at pairing.
- **Auto-lock** drops the user key and all decrypted data from memory. Timer, lock on sleep and lock on session switch are in D-44.

## Client hardening

- Secrets are masked as 12 bullets; revealed fields re-mask when the window loses focus.
- Clipboard copies are cleared after the configured time and marked to stay out of Windows clipboard history and cloud clipboard.
- Optional: hide the window from screen capture (`SetWindowDisplayAffinity`).
- The local SQLite vault stores only ciphertext plus the sync metadata needed to query it.
- The extension fills only on user action, only into the frame whose origin matches the item, and renders in-page UI in closed shadow roots or extension-origin iframes so pages cannot read it.
- The named pipe is ACL-restricted to the current user; the native host checks the calling extension ID.

## Sharing and trust

Vault sharing is not built (decision 14). An item is shared only through a secure link, whose key stays in the URL fragment and never reaches the server; anyone holding the link can open it until it expires, runs out of views or is revoked. If vault sharing is added later, keys would be sealed to public keys the server hands out, and a key fingerprint and pinning on first use would be needed against a server that substitutes one.

## Out of scope for the first release

Hardware security keys for unlock, organisation accounts and admin recovery, emergency access.
