# StrengthMeter

Four-segment strength bar with its word.

- `score` 0–4 maps to Weak, Weak, Fair, Good, Strong (danger, danger, warning, teal, teal). The word always shows.
- `detail` for the estimate ("98 bits"), or the reason on Monitor ("Used on 3 sites").
- Score from estimated entropy or zxcvbn-style analysis, never from length alone.
