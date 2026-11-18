# SecretField

A read-only labelled value on the item detail, with copy and (for secrets) reveal.

- Stack fields inside a `.kd-group` container (`surface-200`, `radius-lg`, hairlines between).
- Secrets (`concealed`, the default) show 12 bullets regardless of real length; reveal switches to mono and re-masks when the window loses focus.
- Set `concealed={false}` for usernames, websites and notes; `mono` for card numbers. `valueColor="var(--link)"` for website URLs.
