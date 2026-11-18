# Kandado Hardware

Kandado is a password manager for desktop, the browser extension and mobile. The name is Tagalog for *padlock*, and this system draws the app as one: a physical lock made of enamel and steel, with keys you press, wells you type into, a small LCD for codes and status lights that tell you what state it is in. It should feel solid, calm and exact. It is not glossy, not futuristic and never alarming.

The system has two themes from the same tokens: **Enamel** (light, the default) and **Graphite** (dark). Both keep the same green, the same LCD and the same lights.

## The metaphor, in four parts

1. **Panels** are the body of the device. Everything sits on a raised module (`panel`, `radius-panel`, `shadow-panel`) over the `body` ground, 16px in from the window edge with 14px between modules. There are no full-bleed sidebars: panels float.
2. **Wells** are cut into a panel. Anything you type into, or any list you scroll, sits in a recessed well (`well`, `shadow-well`). A field is never a line or a box outline.
3. **Keys** are pressed. Every button is a key face (`raised`) with a 3px lip under it (`shadow-key`). Pressing moves it down 2px. The one main action in a view is the green key (`green`, `on-green`). Destructive actions are the danger key.
4. **The LCD** is where Kandado talks back. Two-factor codes, the recovery phrase, lock state, pairing codes and generated passwords show on the dark glass (`lcd`, `lcd-ink`), in mono. The LCD stays dark in both themes, so a code always looks the same.

**Status lights** (`led-ok`, `led-warn`, `tag` for off) sit next to a word wherever state matters: Synced, Locked, 3 to fix, step done. A light is never the only signal.

## Content fundamentals

- Speak plainly, in the second person: "Your vault is locked", not "The vault has been locked".
- Sentence case on keys, titles and menus ("Create login", not "Create Login"). Engraved labels (`label`, `tag`, `segment`) are the one place for capitals.
- Name things by what they are: **login**, **alias**, **card**, **note**, **identity**, **password**, **passkey**, **vault**, **two-factor code**. Never "entry" or "secret".
- Keys say what they do: "Unlock", "Copy password", "Generate", "Restore". Never "OK" or "Submit".
- Warnings state the fact and the next step: "JSON and CSV are not encrypted. Anyone with the file can read every password." No fear words.
- Confirm copies quietly: "Password copied. Clears in 60 s."
- LCD text is short and in capitals: "LOCKED · 09:42", "WAITING FOR GOOGLE…", "3 TO FIX · 2 REUSED · 1 WEAK".
- No emoji and no exclamation marks. Write "Kandado" with a capital K; the logotype is set in spaced mono capitals, KANDADO.

## Visual foundations

**Colour.** Build every screen from four grounds, back to front: `body` (window), `panel` (modules), `well` (recessed), `raised` (key faces, selected rows). Text is `ink`; secondary text `ink-2`; engraved labels `ink-3`, on panel and raised only.

- `green` is the one brand colour. Spend it on the main key of a view (one per view), the active vault, the ring around the selected row, switches that are on, links and the logo body.
- The LCD colours (`lcd`, `lcd-ink`, `lcd-dim`, `lcd-label`, `lcd-warn`) appear only inside the LCD.
- `danger` is for destructive keys and errors, always with a word. `warn-soft` and `danger-soft` are banner grounds.
- Vault colours are lights: `vault-green`, `vault-orange`, `vault-sky`, `vault-plum`, `vault-amber`, `vault-graphite`. A vault shows its light (`hw-led hw-led--vault hw-vault-<colour>`), never a coloured fill, on panel or raised (never straight on a well, where Enamel drops under 3:1), and always beside the vault's name.

**Type.** Two families. `sans` (Schibsted Grotesk) for everything you read, heavy weights for headings. `mono` (Martian Mono) for engraved labels, tags, segmented options, every secret value and everything on the LCD. Styles: `display` on sign-in and unlock only; `title` for the item name; `heading` for panels and dialogs; `body` and `small` for text; `label` above every field; `secret` for values; `lcd-code` for two-factor codes, grouped "482 913".

**Spacing and layout.** `space-5` (16px) from the window edge, `space-4` (14px) between and inside panels, `space-6` (24px) inside dialogs and detail. Touch targets are at least 44 × 44px on mobile.

**Radii.** Engraved tags `radius-tag`; keys, inputs and nav buttons `radius-key` (`radius-key-sm` for small keys); wells and the LCD `radius-well`; panels `radius-panel`; dialogs `radius-dialog`; mobile sheets `radius-sheet` on top. Nothing is square, and nothing is a pill except switch tracks and lights.

**Depth.** Depth is physical, not floating. Panels have a lit top edge and a lip; wells are sunk; keys have a lip. The only real drop shadow is `shadow-pop`, for things that float above the device: dialogs, popovers, the autofill dropdown, the save banner and bottom sheets, over `scrim`.

**States.** Hover: no colour change, the key's cursor is enough. Press: down 2px, lip to 1px. Selected row: `raised` with a 2px `green` ring. Selected option in a segmented control: a raised key face. Focus: 2px `focus` ring, 2px offset. Disabled: 45% opacity.

**Motion.** 120ms for presses and switches. The two-factor countdown bars step once per 3 seconds; nothing else moves. Reduced motion drops all transitions.

**Secrets.** Passwords show as 12 bullets regardless of length. Show is a key per field; copy is a key at the end of every copyable field.

## Iconography

Icons are minimal and drawn inline: 24px viewBox, 1.9px round stroke in `currentColor` (`.hw-ico`). The set has 56 icons (see the Icon component and the Icons assets): the item kinds (`key` for logins, `alias`, `card`, `note`, `id`, `password`, `passkey`), actions (`copy`, `eye`, `edit`, `share`, `trash`, `restore`, `download`, `upload`…), state (`check`, `alert`, `info`, `breach`, `offline`, `sync`) and navigation (`arrow-left`, `chevron`, `more`, `x`). Most actions are still words on keys ("Copy", "Show", "Open"), not icons; an icon goes before its word, or alone on a square key with an `aria-label`. There is no icon font. Icons sit on keys at 18px, at 16px on small keys (`hw-ico--sm`) and at 24px on tiles and empty states (`hw-ico--lg`).

## Vault colours, old to new

The desktop app stored vault colours under the previous system's names. Map them like this when reading saved vaults; write only the new names.

| Stored (old) | Hardware token |
| --- | --- |
| teal | `vault-green` |
| coral | `vault-orange` |
| sky | `vault-sky` |
| plum | `vault-plum` |
| brass | `vault-amber` |
| moss | `vault-graphite` (moss was a second green; Hardware keeps one green, so moss vaults turn graphite) |

Amber is darker in Enamel (#B07F0A) than in the first redesign draft (#D9A21B), which was 2.0:1 on panel.

## Screens

`screens.md` lists every desktop and extension screen by its ID (D-01…, E-01…), which board in the Kandado Redesign canvas draws it, and which still have no board.

## The mark

The padlock mark is a `green` body with a `body`-coloured keyhole and an `ink` shackle, drawn inline from the `.hw-logo-*` classes so it follows the theme. Closed means locked; the open shackle is used on sign-in and signed-out screens. The logotype is KANDADO in `mono`, 600, letter-spacing 0.18em. Static files for both themes are in the Logos group.

## Theming

Load `tokens.css` and `components/bundle.css`, wrap the app in `class="hw"`, and set `data-theme="light"` or `data-theme="dark"` on `<html>`. Every colour comes from a token, so the same markup renders in both themes. Offer System, Light and Dark in settings; System follows the device.

## Platforms

- **Desktop:** a header panel (logo, search with Ctrl F, sync light, Lock / Open switch), then the vault panel, the list panel and the detail panel side by side.
- **Extension:** the popup is 380 × 600 with a 10px inset. Header panel with vault switcher, settings and a dark lock key; an LCD strip names the site and its logins; the list sits in a well; Generate and Create login keys at the bottom.
- **Mobile:** a header panel with the vault switcher and lock key, search well and kind filter; the list in a well of raised rows; a bottom dock with a green create key in the middle.

## Components

Key, Switch, Segmented, Field, Lcd, StatusLight, Tag, ItemRow, NavButton, Surfaces, Dialog, Logo and Icon. Each has guidelines and a live preview. They are CSS classes in `components/bundle.css` (prefix `hw-`) applied to plain HTML, so any stack can use them.
