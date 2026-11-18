# ItemRow

One item in a list: title, the most useful secondary fact, and its kind tag, inside a list well.

- Put rows in a `hw-well` with 6px padding and 4px between rows. Each row is `hw-item`.
- `hw-item-title` is the item name; `hw-item-sub` is the username for logins, "•••• 4821" for cards, the address for aliases, the first line for notes, or "28 days left" in Trash.
- The open item gets `hw-item--on`: it rises to a key face with a 2px green ring. Set `aria-selected="true"`.
- End the row with a `hw-tag` for the kind, or a small Copy key where copying is the main use (extension, mobile).
- On mobile, rows are raised faces (`hw-raised`, at least 62px tall) so each reads as a separate key.
