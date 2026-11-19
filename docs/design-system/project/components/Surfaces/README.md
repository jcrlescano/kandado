# Surfaces

The three depths every screen is built from: panels, wells and raised faces, plus banners.

- `hw-panel` is a raised module on the `body` ground (header, sidebar, list, detail, dialogs). Panels sit 16px from the window edge with 14px between them, and pad 14–24px.
- `hw-well` is cut into a panel: inputs, lists, groups of settings rows. Hairlines between rows inside a well use `edge`.
- `hw-raised` is a key face used as a container: the selected row, tiles, devices in a list.
- Never nest a panel in a panel. A well can hold raised faces; a raised face does not hold wells.
- `hw-banner` (on `warn-soft`) and `hw-banner--danger` (on `danger-soft`) carry a status light and one sentence for warnings that need reading.
