# ItemRow

One item in a list: icon, title, subtitle, optional badge and copy.

- `title` is the item name; `subtitle` is the most useful secondary fact: username for logins, alias address, last four digits for cards ("•••• 4821"), first line for notes.
- `selected` marks the item open in the detail pane (desktop) and adds the brass leading bar.
- Pass `onCopy` to show a copy button on hover and selection; it copies the password (logins) or the primary value.
- `badge` for a Monitor state. `favorite` adds the brass star.
