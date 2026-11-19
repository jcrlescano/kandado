# Segmented

A row of options sunk into a well, where the chosen one rises as a key face.

- `<div class="hw-seg" role="radiogroup">` with `<button role="radio" aria-checked>` options; tabs use `role="tablist"` / `aria-selected`.
- Option text is the `segment` style: mono capitals, short ("5 MIN", "RANDOM", "LOGIN").
- Use it for 2–5 exclusive options that apply at once: lock timers, kind filters, Random / Memorable, theme.
- The Lock / Open switch in the desktop header is a two-option Segmented.
- On a well background, set the control's own background to `panel` so it still reads as sunk.
