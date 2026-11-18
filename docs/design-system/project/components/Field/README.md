# Field

A labelled input or value, sunk into a well, with keys set into its end.

- Structure: `hw-field` holding a `hw-label` (engraved, above) and either an `hw-input` (editable) or an `hw-well-row` (a read-only value with keys at the end).
- Every field has a visible label; placeholders are examples ("example.com"), never the label.
- Secret values (usernames, passwords, URLs, card numbers) use `hw-input--mono` or the `secret` style. Passwords show as 12 bullets until Show.
- Put Copy, Show, Open, Generate as `hw-key--sm` at the end of the well; the main one may be green.
- Errors: set `aria-invalid="true"` (draws a danger ring) and add an `hw-error` line saying what to do: "The two passwords don’t match." Hints use `hw-hint`.
