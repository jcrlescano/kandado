# StatusLight

A small round light that shows state, always beside a word.

- `hw-led` (lit green, `led-ok`): synced, healthy, done, a completed setup step, an active device.
- `hw-led--warn` (glowing orange, `led-warn`): locked, needs attention, waiting, the current step.
- `hw-led--off` (unlit, `tag`): signed out, idle, not set, a step still to come.
- Never let the light carry meaning alone: "SYNCED", "3 TO FIX", "Strong and unique". Orange and green differ in lightness too, so the pair reads in greyscale.
- Vault colours reuse the light: a vault shows a small light in its colour beside its name.
- Setup steps are a row of lights in a well: done (green), current (orange, bold label), to come (off).
