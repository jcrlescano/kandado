# TotpCode

A two-factor code with its countdown ring, inside a field group.

- The app computes the code; this component only displays it, grouped in threes ("482 913"), in the `totp` style.
- The ring counts down the period (default 30 s) in `teal`, turning `warning` in the last 5 seconds.
- Copy is always available; on copy near expiry, copy the next code too and say so in the toast.
