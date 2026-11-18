# Button

Text button for actions, in four variants.

- `primary` (brass) is the single main action of a view: "Unlock", "Save", "Create login". One per view.
- `secondary` for other actions beside it; `ghost` for low-emphasis and toolbar actions; `danger` only for irreversible actions ("Delete forever"), `danger-quiet` for reversible ones ("Move to trash").
- Labels are verbs in sentence case. Add `icon` when it speeds recognition (plus for create, share for share).
- `size`: `sm` (32px) in dense toolbars, `md` (40px) default, `lg` (48px) on mobile and unlock screens. `block` stretches to the container.
