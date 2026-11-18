Kandado is a password manager for desktop, mobile and the browser. The name is Tagalog for *padlock*: the brand is a solid brass lock on a dark, quiet vault. It should feel calm, exact and trustworthy, never alarmist and never cute.

## Content fundamentals

- Speak plainly, in the second person: "Your vault is locked", not "The user's vault has been locked".
- Sentence case everywhere: buttons, titles, menu items ("Create login", not "Create Login"). Field labels above values are the one uppercase style (`label`).
- Name things by what they are: **login**, **alias**, **card**, **note**, **identity**, **password**, **passkey**, **vault**. Never "entry", "record" or "secret item".
- Verbs on buttons: "Unlock", "Copy password", "Generate", "Share vault". No "OK" or "Submit".
- Security warnings state the fact and the next step, no fear words: "This password appeared in a data breach. Change it on github.com." Not "DANGER! Your account is compromised!"
- Confirm copies quietly: a toast reading "Password copied. Clears in 60 s."
- No emoji, no exclamation marks.
- Write "Kandado" with a capital K, never all caps, never "KanDado".

## Visual foundations

**Colour.** Dark (Vault) is the primary theme and the fallback; Light (Paper) is a full alternative. Build every screen from four grounds, back to front: `surface-000` (window), `surface-100` (panels), `surface-200` (raised groups, hover), `surface-300` (inputs, selection). Text is `ink`, secondary text `ink-muted`; `ink-faint` is for placeholders only.

- `brass` is the one brand colour. Spend it on the primary action of a view (one per view), the active vault and the logo. Text on a brass fill is `on-brass`, never literal white or black.
- `teal` means secure and healthy, and is also links and the focus ring.
- Each item kind has its own hue, used only in its item icon and its "Create" menu entry: `kind-login` (teal), `kind-alias` (moss), `kind-card` (coral), `kind-note` (sky), `kind-identity` (plum), `kind-password` (brass). The icon sits on the matching `*-soft` ground.
- `danger` and `warning` are always paired with an icon and a word ("Breached", "Reused", "Weak"). Never signal state by colour alone.
- Dividers use `line`; anything you can type into or toggle uses `line-strong` so its edge holds 3:1.

**Type.** Three families. `display` (Bricolage Grotesque) for titles, `sans` (Figtree) for everything you read, `mono` (JetBrains Mono) for anything a person must type back exactly: revealed passwords, card numbers, codes. Use `title-lg` for the item name on detail, `body-strong` for list row titles, `small` for row subtitles, `label` above field values, `secret` for revealed values and `totp` for one-time codes, grouped "482 913".

**Spacing and layout.** A 4px grid: `space-1` to `space-7`. Panels pad `space-4`; the item detail pads `space-5`. List rows are 56px tall on desktop and extension, 64px on mobile (touch). Minimum touch target 44 × 44px.

**Radii.** `radius-md` for buttons, inputs, rows and item icons; `radius-lg` for field groups, cards, dialogs and the extension popup; `radius-pill` for vault chips, toggles and search; `radius-xl` for the top corners of mobile sheets. Nothing is fully square.

**Elevation.** Flat by default: panels are told apart by their surface step, not shadows. `shadow-pop` only on floating things (menus, the autofill dropdown, toasts); `shadow-sheet` on mobile bottom sheets. Overlays sit on `scrim`.

**Secrets.** Passwords show masked as 12 bullets regardless of length, never the real length. Reveal is a toggle per field (eye icon), and revealing re-masks when the window loses focus. Copy buttons sit at the end of every copyable field.

**States.** Hover: one surface step up. Selected row: `surface-300` with a 3px `brass` bar on its leading edge inside the row radius. Focus: 2px solid `focus-ring`, 2px offset, on every interactive element. Disabled: 40% opacity, no pointer.

**Motion.** Short and functional: 120ms for hover and press, 200ms ease-out for sheets and dialogs. The TOTP ring animates continuously; nothing else loops. Respect reduced motion by dropping the ring animation to a stepped update.

## Iconography

Kandado's icons are the 24px outline set in `Kandado.Icon` (1.75px stroke, round caps and joins, `currentColor`), drawn for this system: lock, unlock, key, user, mail, card, note, id, password, passkey, shield, search, plus, copy, eye, eye-off, globe, star, trash, settings, vault, share, refresh, clock, alert, check, chevron, more, x, link, fingerprint, logout. Render them 16px in dense rows, 20px in buttons, 24px in navigation. No emoji, no filled icon variants, no third-party icon fonts.

The mark (`Kandado.Logo`) is a padlock built from a rounded body (`radius-lg` proportions) and a keyhole; it draws in `brass` on dark and light grounds. It is a proposed first mark; replace it with the final logo files in a Logos asset group when they exist.

## Components

Everything lives in `components/bundle.js` as `window.Kandado` and expects React 18. Build screens from these parts: `Button`, `IconButton`, `TextField`, `Toggle`, `Badge`, `SearchBar`, `VaultChip`, `ItemIcon`, `ItemRow`, `SecretField`, `StrengthMeter`, `PasswordGenerator`, `TotpCode`, `Toast`, `Logo`, `Icon`. The Screens groups draw every screen of the product, 160 in all, built from these parts: Mobile (M-01 to M-75), Desktop (D-01 to D-54) and Browser extension (E-01 to E-31). The `screens.md` section lists each one with its states and where its actions lead; implement against that list. The cards use `Kandado.Kit`, a layout helper in the same bundle (frames, sheets, dialogs, shared item bodies). It documents layouts and is not a product component.
