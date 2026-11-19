# Dialog

A panel that floats over the device on a scrim, for confirmations and short tasks.

- `hw-scrim` covers the screen; `hw-dialog` sits centred with `shadow-pop`. Use `role="dialog"` or `role="alertdialog"` with an `aria-label`.
- Order: an optional LCD strip naming what happens ("SIGN OUT · THIS COMPUTER"), the heading as a question ("Sign out of Kandado?"), one or two sentences of consequence, then keys bottom-right: Cancel (raised) and the action (green, or danger for irreversible ones).
- Typed confirmation for the most destructive actions: a Field with "Type DELETE to confirm".
- Width 420–540px. On mobile the same content becomes a bottom sheet with `radius-sheet` top corners and a grab handle.
