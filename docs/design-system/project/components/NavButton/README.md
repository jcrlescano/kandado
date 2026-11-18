# NavButton

A row in a navigation panel: vaults, tools, settings pages, mobile dock tabs.

- `hw-navbtn` is flat; `hw-navbtn--raised` is a key face (used for vaults, so each vault reads as something you press); `hw-navbtn--on` is the current page: green, sunk, white text. Set `aria-current="page"`.
- Counts go at the end in `hw-count` (mono). A vault shows its colour as a small status light before the name.
- Tools that need attention show an orange light and the count ("3 TO FIX").
- Height 44px; on the mobile dock, centre the label.
