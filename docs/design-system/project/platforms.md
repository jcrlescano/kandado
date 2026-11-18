# Platforms

One system, three shells. The components are the same; the layout and density change.

## Desktop (Windows, macOS, Linux)

- Three panes, left to right: **sidebar** 240px (`surface-100`: logo, vaults, Monitor, Trash, settings), **item list** 340px (`surface-100`, separated by a `line` hairline), **item detail** fills the rest (`surface-000`, padding `space-5`, field groups on `surface-200` with `radius-lg`).
- Minimum window 960 × 600. Below 1120px wide, the sidebar collapses to a 64px icon rail.
- The title bar is the app's own: drag area on `surface-000`, the global search centred, window controls on the platform's side.
- Keyboard first: Ctrl/Cmd + F search, Ctrl/Cmd + N new item, Ctrl/Cmd + C on a selected row copies its password, Ctrl/Cmd + L locks.

## Mobile (iOS, Android)

- Bottom tab bar, four tabs: **Items**, **Search**, **Monitor**, **Settings**. Active tab in `brass` with its label; inactive in `ink-muted`.
- A floating 56px brass "+" button above the tab bar opens the Create sheet (one row per item kind, each with its `ItemIcon`).
- Screen gutter `space-4`; list rows 64px; every target at least 44 × 44px.
- Sheets rise from the bottom with `radius-xl` top corners and `shadow-sheet` on `scrim`.
- Unlock is biometric first with PIN fallback; the unlock screen is the one place `display-lg` appears.

## Browser extension (Chrome, Firefox, Edge, Safari)

- Popup is 380 × 600, `surface-000` inside a `radius-lg` frame.
- Header: logo mark, vault filter (`VaultChip`), lock button. Below it a `SearchBar`, then **Suggestions for this site** first, then all items.
- Rows are 56px; tapping a row autofills, the trailing copy button copies.
- In-page: a small Kandado mark inside focused username, password and email fields opens the **autofill dropdown** (`surface-100`, `radius-lg`, `shadow-pop`, 320px wide), listing matching logins, "Generate password" for new-password fields, "Create alias" for email fields.
- After a sign-in submits with new credentials, a save banner slides in top-right: "Save login for github.com?" with Save (brass) and Not now (ghost).
