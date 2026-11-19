# Lcd

The dark glass readout where Kandado shows codes and state: two-factor codes, the recovery phrase, lock state, pairing codes and generated passwords.

- `hw-lcd` is dark in both themes. Everything on it uses `lcd-ink` (`hw-lcd-digits`), `lcd-label` (`hw-lcd-label`) or `lcd-warn`.
- Two-factor codes: `lcd-code` style, grouped in threes ("482 913"), with ten countdown bars (`hw-lcd-bar`, unlit `hw-lcd-bar--off`) that step every 3 seconds, and a COPY key set into the glass (`hw-lcd-key`). Give the bars an `aria-label` with the seconds left.
- Status strips: a status light, a short capitalised readout ("LOCKED · 09:42", "GITHUB.COM · 2 LOGINS"), and an optional label at the end.
- Keep text short. One LCD per area; never use it as a decorative card.
