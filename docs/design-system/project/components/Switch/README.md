# Switch

An on/off switch sunk into the panel, for settings that apply at once.

- Use `<button role="switch" class="hw-switch" aria-checked="true|false">` with an `aria-label` that matches the visible setting name.
- On fills `green`; off shows the sunken `well` track. The knob is a key face.
- Put the setting name (`body-strong`) and a one-line consequence (`small`, `ink-2`) to its left in a row inside a well: "Lock when the computer sleeps or locks".
- For choices that need Save, use Segmented or radio tiles instead. A disabled switch (always on) keeps its explanation: "Always on: the keys are kept only in memory."
