# Icon

Stroke icons drawn for Hardware: a 24px grid, a 1.9px round stroke in `currentColor`, no fills except the small dots in `password`, `more`, `alert`, `info` and `breach`.

- Inline the SVG with class `hw-ico` (18px, the size on a key), `hw-ico--sm` (16px, on small keys and beside engraved labels) or `hw-ico--lg` (24px, on empty states and tiles). The class sets the stroke, so the markup is only the shapes.
- The colour comes from the text around it: `ink` on keys and rows, `on-green` on the go key, `ink-3` for the search glass in a well header, `danger` beside an error word. Never colour an icon with a vault or LCD colour.
- Most actions stay words on keys ("Copy", "Show", "Open"). Use an icon alone only on square keys (`hw-key--icon`) with an `aria-label`: lock, settings, close, back, more, search. Elsewhere, an icon sits before its word with `space-2` between.
- Decorative icons get `aria-hidden="true"`.
- The 56 names: lock, unlock, key, user, users, invite, mail, alias, card, note, id, password, passkey, shield, breach, health, search, plus, minus, copy, eye, eye-off, globe, star, trash, settings, vault, share, refresh, sync, clock, history, restore, alert, info, check, chevron, chevron-down, arrow-left, external, edit, more, x, link, fingerprint, logout, grid, bolt, download, upload, filter, offline, qr, device, sun, moon.
- Item kinds: login uses `key`, alias `alias`, card `card`, note `note`, identity `id`, password `password`, passkey `passkey`. Monitor uses `health` (the tool), `breach` (a breached site), `alert` (weak or reused).
- Files: every icon is in the Icons asset group as `<name>.svg`, and `assets/Icons/icons.json` holds all of them as one file (`names`, and `icons`: name to inner SVG markup) for code to read.
