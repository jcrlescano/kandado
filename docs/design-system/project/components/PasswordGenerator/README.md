# PasswordGenerator

The password generator: random or memorable, with live strength.

- Defaults: random, 20 characters, uppercase, numbers and symbols on. Memorable defaults to 4 words.
- Uses `crypto.getRandomValues`; ambiguous characters (0 O l 1 I) are excluded from random passwords.
- Numbers show in `sky`, symbols in `brass` so a person can read the password back.
- In create flows pass `onUse` to add "Use password". Desktop: a 360px popover from the password field; mobile: a bottom sheet; extension: inline in the dropdown.
