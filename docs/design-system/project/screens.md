# Screens and flows

Every screen Kandado ships, by platform. IDs match the captions on the Screens cards (M = mobile, D = desktop, E = browser extension). When you build a screen, build all of its states listed here. "→" names where an action goes.

## Shared flows (all platforms)

1. **First run:** Welcome → Sign in *or* Create account → Verify email → Recovery phrase → Set PIN → Biometrics → (mobile: Turn on autofill) → Import → Items.
2. **Unlock:** biometrics first, PIN second, account password as fallback. 5 wrong PINs → Locked out → password unlock. Auto-lock follows Settings › Security.
3. **Create:** Create menu (one row per kind) → kind form → Save → item detail, toast "Login created".
4. **Edit:** detail › Edit → same form, filled → Save → detail. Leaving with changes → Discard changes dialog.
5. **Delete:** detail › More › Move to trash → toast with Undo → Trash (30 days) → Restore *or* Delete forever (confirm).
6. **Share:** vault › Share → invite by email + role → invitee gets Invitation → Accept → vault appears. Single item: More › Share → people *or* Secure link (expiry, max views) → link ready → Secure links list (revoke).
7. **Monitor:** overview → category list → item → fix (change password, add 2FA) → item leaves the list. Breach → Breach detail → Mark as resolved.
8. **Sign out:** Settings › Sign out → confirm → Welcome. Local data is wiped.

## Mobile

| ID | Screen | Contents and states | Actions |
| --- | --- | --- | --- |
| M-01 | Welcome | Logo, `display-lg` headline, three promises | Create account → M-03 · Sign in → M-02 |
| M-02 | Sign in | Email, password (reveal), forgot password link, error state under field | Sign in → M-04 if new device, else M-06 |
| M-03 | Create account | Email, password with StrengthMeter, confirm, terms line | Continue → M-04 |
| M-04 | Verify email | 6 code boxes, resend timer, wrong-code error | Code complete → M-05 |
| M-05 | Recovery phrase | 12 numbered words, Copy, Save as file, "I saved it" checkbox gating Continue | Continue → M-06 |
| M-06 | Set PIN | 6 dots, keypad, confirm step ("Enter it again") | Done → M-07 |
| M-07 | Biometrics | Face ID / fingerprint explainer | Turn on / Not now → M-08 |
| M-08 | Turn on autofill | OS steps (iOS: Settings › Passwords › AutoFill; Android: Autofill service) | Open settings · Later → M-09 |
| M-09 | Import | Source list (Proton Pass, Bitwarden, 1Password, LastPass, browsers, CSV) | Pick → M-63 · Skip → M-15 |
| M-10 | Unlock, biometrics | Logo, account email, fingerprint button | Success → last screen · Use PIN → M-11 |
| M-11 | Unlock, PIN | Dots, keypad, biometric key on keypad | Wrong → M-12 |
| M-12 | Wrong PIN | Dots shake in `danger`, "3 attempts left" | 0 left → M-13 |
| M-13 | Locked out | Explainer, Unlock with password | → M-14 |
| M-14 | Unlock, password | Account password field, Forgot password | Unlock → M-15 |
| M-15 | Items | Vault chip, search, kind chips, list, FAB, tab bar | Row → detail · FAB → M-32 · chip → M-17 · long-press → M-22 |
| M-16 | Filter and sort sheet | Kinds (multi), sort: Recent, A–Z, Created, Most used | Apply |
| M-17 | Vault switcher sheet | All vaults + each vault with count, Manage vaults | Manage → M-42 |
| M-18 | Empty vault | Illustration-free empty state, create buttons per kind, Import | → M-33 / M-09 |
| M-19 | Search, recent | Recent searches, clear | Type → M-20 |
| M-20 | Search results | Results grouped by kind, match highlighted, vault scope chip | Row → detail |
| M-21 | No results | Message + "Create login for “…”" | → M-33 |
| M-22 | Multi-select | Checkboxes, count in header, bar: Move, Share, Trash | Move → M-47 · Trash → toast |
| M-23 | Login detail | Header, username, password, 2FA code, websites, note, custom fields, passkey link, meta | Edit → M-33 · More → M-30 |
| M-24 | Alias detail | Address, forwards to, enabled toggle, stats (forwarded, replied, blocked), note | Disable → confirm toast |
| M-25 | Card detail | Holder, number, expiry, CVV, PIN (concealed) | Copy each |
| M-26 | Note detail | Full text | Copy all |
| M-27 | Identity detail | Personal, Address, Contact, Work sections | Copy each |
| M-28 | Password detail | Revealed-on-tap value, strength, created, used for | Save as login → M-33 |
| M-29 | Passkey detail | Username, site, created, key ID | Delete (confirm) |
| M-30 | Item actions sheet | Favourite, Share, Move, History, Duplicate, Pin to home, Move to trash | → M-48, M-47, M-31 |
| M-31 | Item history | Revisions by date, selected diff | Restore this version |
| M-32 | Create sheet | One row per kind with ItemIcon | → M-33 … M-40, M-34 |
| M-33 | Login form | Title, username (+ alias action), password (+ generate), strength, Add 2FA, websites (+ add), note, custom fields, vault | Save · Generate → M-34 · Add 2FA → M-35 · Add field → M-36 |
| M-34 | Generator sheet | PasswordGenerator with Use password | Use → back to form |
| M-35 | Scan 2FA QR code | Camera viewfinder, Enter key manually | Detected → field filled |
| M-36 | Add custom field sheet | Text, Hidden, Two-factor code, Date | Pick → new field |
| M-37 | Alias form | Prefix, domain suffix, mailbox, note, preview of full address | Create alias |
| M-38 | Card form | Holder, number, expiry, CVV, PIN, note | Save |
| M-39 | Note form | Title, text area | Save |
| M-40 | Identity form | Sections with fields, Add section | Save |
| M-41 | Discard changes | Dialog | Keep editing · Discard |
| M-42 | Vaults | Vault rows (colour, count, shared badge), Create vault | Row › More → edit, share, leave, delete |
| M-43 | Create / edit vault | Name, colour (6 hues), icon grid | Save |
| M-44 | Share vault | Email, role (Viewer, Editor, Manager), Send invite | → M-45 |
| M-45 | Members and invites | Members with role, pending invites (Resend, Cancel), Leave vault / Delete vault | Change role |
| M-46 | Invitation received | Inviter, vault preview, item count | Accept · Decline |
| M-47 | Move to vault sheet | Vault radio list | Move |
| M-48 | Share item sheet | Share with people, Create secure link | → M-44 (item), M-49 |
| M-49 | Create secure link | Expires (1 h, 1 day, 7 days, 30 days), max views | Create → M-50 |
| M-50 | Secure link ready | Link, Copy, Share, details | Done |
| M-51 | Secure links | Active links with expiry and views, Revoke | Revoke (confirm) |
| M-52 | Monitor overview | Status banner, four counts, categories, email monitoring entry | → M-53 … M-57 |
| M-53 | Breach detail | Site, date, exposed data, affected items, steps | Change password · Mark as resolved |
| M-54 | Weak passwords | Items with StrengthMeter | Change → M-33 |
| M-55 | Reused passwords | Groups of items sharing a password | Change |
| M-56 | Missing 2FA | Items whose sites support 2FA | Add code → M-35 |
| M-57 | Email monitoring | Account email and aliases with breach counts, toggles | Add address |
| M-58 | Settings | Account card, sections | → M-59 … M-67 |
| M-59 | Security | Biometrics, Change PIN, Auto-lock, Lock on sleep, Clear clipboard, Extra password, Recovery phrase | |
| M-60 | Autofill | Autofill on, provider status, suggest aliases, generate passwords, save prompts, passkeys | |
| M-61 | Appearance | Theme (System, Dark, Light), show site icons (privacy note), language | |
| M-62 | Aliases settings | Default domain, default mailbox, mailboxes (verified), alias count | Add mailbox |
| M-63 | Import | Source, file pick, progress, result (imported, skipped) | Done → M-15 |
| M-64 | Export | Format (Encrypted, JSON, CSV), warning, password confirm | Export |
| M-65 | Devices | Sessions with current tag | Sign out a device · Sign out all others |
| M-66 | Account | Email, plan, change password, 2FA for account, delete account | |
| M-67 | Sign out dialog | Warning that local data is removed | Cancel · Sign out → M-01 |
| M-68 | Trash | Deleted items with days left | Restore · Delete forever · Empty trash |
| M-69 | Empty trash dialog | Count, cannot be undone | Cancel · Delete forever |
| M-70 | Keyboard suggestion (OS) | Another app's login form, keyboard with Kandado suggestion | Tap → filled |
| M-71 | Autofill picker (OS) | Kandado sheet with matches and search | Pick → filled |
| M-72 | Save login prompt (OS) | "Save password to Kandado?" | Save · Not now |
| M-73 | Passkey sheet (OS) | Create or use passkey with Kandado | Continue |
| M-74 | Offline and sync error | Banner on list, retry | Retry |
| M-75 | Toasts | Copied, Moved to trash + Undo, error | |

## Desktop (Windows, macOS, Linux)

| ID | Screen | Contents and states | Actions |
| --- | --- | --- | --- |
| D-01 | Sign in | Window split: brand panel + form | Create account → D-02 |
| D-02 | Create account | Email, password + strength, confirm | → verify (as M-04) → D-03 |
| D-03 | Recovery phrase | 12 words, Copy, Save as file, checkbox | Continue → D-04 |
| D-04 | Unlock | Password, Windows Hello / Touch ID button, Sign out link | |
| D-05 | First-run import | Source grid, Skip | → D-47 |
| D-06 | Empty vault | Three panes, empty list, create shortcuts | |
| D-07 | Login detail | List + detail (see DesktopApp card) | Edit → D-19 |
| D-08 … D-13 | Alias, card, note, identity, password, passkey detail | Same layout, kind fields | |
| D-14 | Search results | Global search open, grouped results, keyboard hints | Enter → detail |
| D-15 | Multi-select | Checkboxes, bulk bar: Move, Share, Trash | |
| D-16 | Item history | Revisions list + preview | Restore |
| D-17 | Narrow window | Sidebar collapsed to 64px rail | |
| D-18 | Create menu | Dropdown from Create item | → forms |
| D-19 | Login form | Inline in detail pane, generator popover open | Save · Cancel |
| D-20 | Add 2FA | Paste setup key or scan QR from screen | |
| D-21 … D-24 | Alias, card, note, identity forms | | |
| D-25 | Delete forever dialog | | |
| D-26 | Discard changes dialog | | |
| D-27 | Create vault dialog | Name, colour, icon | |
| D-28 | Vault settings | Members, roles, pending invites, Leave / Delete | |
| D-29 | Share vault dialog | Emails, role | |
| D-30 | Invitation received | Banner + dialog | Accept · Decline |
| D-31 | Share item dialog | People or secure link | |
| D-32 | Create secure link dialog | Expiry, max views → link | |
| D-33 | Secure links | Table: item, created, expires, views, Revoke | |
| D-34 | Move items dialog | Vault list | |
| D-35 … D-40 | Monitor: overview, breach, weak, reused, missing 2FA, email monitoring | As mobile, in the main pane | |
| D-41 | Trash | List + detail with Restore / Delete forever | Empty trash → D-42 |
| D-42 | Empty trash dialog | | |
| D-43 … D-50 | Settings: General and appearance, Security, Autofill and browser, Aliases, Import, Export, Devices, Account | Settings nav + page | |
| D-51 | Keyboard shortcuts | Dialog listing shortcuts | |
| D-52 | Sign out dialog | | |
| D-53 | System tray menu | Open, Lock, Generate password, Quit | |
| D-54 | Banners | Offline, update ready (Restart) | |

## Browser extension

| ID | Screen | Contents and states | Actions |
| --- | --- | --- | --- |
| E-01 | Popup, signed out | Logo, Sign in, Create account (opens tab) | |
| E-02 | Popup, locked | PIN entry, "Unlock with desktop app" | |
| E-03 | Popup, home | Suggestions for this site, all items, Create / Generate | Row → fill · copy |
| E-04 | Popup, search | Results | |
| E-05 | Popup, nothing for this site | Empty suggestion, Create login for site | |
| E-06 … E-08 | Popup detail: login, card, alias | Back header, fields with copy | Edit opens form |
| E-09 | Popup create menu | Kinds | |
| E-10 | Popup create login | Prefilled site, generator | Save |
| E-11 | Popup generator | PasswordGenerator | Copy |
| E-12 | Popup create alias | Prefix from site | Create and copy |
| E-13 | Popup menu | Account, Settings, Monitor, Lock, Sign out | |
| E-14 | Popup vault filter | Vault list | |
| E-15 | Popup breached site | Warning banner on suggestions | Open Monitor |
| E-16 | In-page login dropdown | Matches for site | Pick → fill |
| E-17 | In-page locked dropdown | "Unlock Kandado to fill" | → E-02 |
| E-18 | In-page new password | Generated suggestion | Use → fill both fields |
| E-19 | In-page email field | Create alias, existing aliases | |
| E-20 | In-page card | Cards list for checkout | |
| E-21 | In-page identity | Identities for address forms | |
| E-22 | In-page 2FA code | Current code for the site | |
| E-23 | Save login prompt | Site, username, vault | Save · Not now · Never for this site |
| E-24 | Update password prompt | Old → new | Update |
| E-25 | Passkey create | Site, username, vault | Save passkey |
| E-26 | Passkey sign in | Pick passkey | |
| E-27 | Welcome tab | After install: pin to toolbar, sign in | |
| E-28 … E-31 | Settings tab: General, Autofill, Excluded sites, Security | Settings nav + page | |
