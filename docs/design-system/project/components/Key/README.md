# Key

A pressable button: a key face with a lip under it, in five finishes.

- `hw-key` (raised face) for most actions: Copy, Show, Share, Cancel.
- `hw-key--go` (green) is the one main action of a view: Unlock, Save, Create login, Continue. One per view.
- `hw-key--dark` is the contrast key for Lock and similar device controls.
- `hw-key--danger` only for irreversible actions: Delete forever, Delete account, Sign out on its confirm dialog.
- `hw-key--flat` drops the face for low-emphasis actions inside rows ("Never for this site").
- Sizes: `hw-key--sm` (32px) inside fields and rows, default 40px, `hw-key--lg` (52px) on unlock screens, mobile and the extension popup. `hw-key--block` stretches to the container; `hw-key--icon` makes it square (give it an `aria-label`).
- Labels are verbs in sentence case. Use a real `<button>` or `<a href>`; set `aria-pressed` or `aria-checked` on keys that toggle, which draws the green ring.
