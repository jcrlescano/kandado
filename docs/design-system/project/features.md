# Feature map

What Kandado does, and where each feature shows up. Proton Pass is the reference product for scope; Kandado's own UI and names below are what screens should use.

| Feature | What it is | Desktop | Mobile | Extension |
| --- | --- | --- | --- | --- |
| Logins | Username or email, password, websites, notes, custom fields | Full create and edit | Full create and edit | Autofill, save on submit, quick edit |
| Email aliases | Forwarding addresses that hide the real inbox; enable, disable, delete | Manage list and mailboxes | Manage list | Create alias in any email field |
| Cards | Cardholder, number, expiry, CVV, PIN | Full | Full, plus autofill | Autofill checkout forms |
| Notes | Free text, encrypted | Full | Full | Read only |
| Identities | Name, address, phone, company, IDs | Full | Full | Autofill address forms |
| Passwords | Standalone generated passwords and history | Generator panel | Generator sheet | Generator in any password field |
| Passkeys | WebAuthn credentials stored in the vault | View, delete | Create and use (OS provider) | Create and use on sites |
| Two-factor codes | TOTP stored on the login it belongs to | Code with countdown | Code, plus scan QR | Autofill code field |
| Vaults | Separate collections, each with a name, colour and icon | Sidebar | Vault switcher sheet | Vault filter |
| Sharing | Share a vault or a single item with others; roles: viewer, editor, manager | Full | Full | — |
| Secure links | A one-time or time-limited link to a single item | Create, revoke | Create | — |
| Monitor | Breach alerts, weak, reused and missing-2FA reports | Monitor screen | Monitor tab | Badge on breached sites |
| Search | Across all vaults by title, username, URL, note text | Global search | Search tab | Popup search, current site first |
| Trash | Deleted items kept 30 days | Trash view | Trash view | — |
| Lock | Auto-lock timer, PIN, biometrics (Windows Hello, Touch ID, Face ID, fingerprint) | Settings | Settings | Follows desktop app or own PIN |
| Import and export | From other managers (CSV, JSON), encrypted export | Settings | — | — |

## Naming rules

- Use the Kandado names in the Feature column in UI copy; use "two-factor code" in copy and `TotpCode` in code.
- Monitor statuses are, in order of severity: **Breached**, **Weak**, **Reused**, **Missing 2FA**, **Healthy**. Each has a fixed tone: danger, danger, warning, warning, teal.
