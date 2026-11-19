# Windows installer

`kandado.nsi` is the NSIS script for `kandado-<version>-setup.exe`. The
release workflow (`.github/workflows/release.yml`) builds it on a version tag,
and on pull requests that touch packaging (as an artifact, without publishing).

- **Per user.** Installs to `%LOCALAPPDATA%\Programs\Kandado`, with no
  administrator prompt, and registers in "Installed apps" under the current user.
- **Two programs, side by side.** `kandado.exe` and `kandado-native-host.exe`
  (P5-31). The app registers the browser link itself when it is turned on in
  D-45; the installer does not touch the browsers.
- **Upgrades in place.** Running a newer installer replaces the programs; it
  asks to quit Kandado (and close browsers using its link) if they are running.
- **The uninstaller** removes the programs, the Start menu shortcut, "Start with
  Windows" (D-43) and the native host keys for Chrome, Edge and Firefox. It
  keeps the vault in `%APPDATA%\Kandado`; Sign out in the app (D-52) removes it.
- **Unsigned** until there is a code-signing certificate, so SmartScreen warns
  on first run. Signing will be a step after `makensis` in the workflow.

To build one by hand (NSIS 3 installed, `cargo build --release -p kandado -p kandado-bridge --bins` done):

```sh
mkdir stage
cp target/release/kandado.exe target/release/kandado-native-host.exe stage/
cp LICENSE stage/LICENSE.txt
makensis -DVERSION=0.1.0 -DSOURCE="$PWD\stage" -DOUTFILE="$PWD\kandado-0.1.0-setup.exe" src/desktop/packaging/windows/kandado.nsi
```

## Updates (D-54)

Each release also carries `latest.json` (version, installer address, SHA-256
and size) and `latest.json.minisig`, made by `src/desktop/tools/update-signer`
in the publish job. The app (`kandado-app/src/update.rs`) checks
`releases/latest/download/latest.json` a minute after it starts and every 12
hours (Settings, General, Updates turns this off; Check now checks at once).

- It trusts the manifest only with a Minisign signature from the key in
  `kandado-app/src/update_key.pub`, whose trusted comment names `latest.json`,
  and only for a version newer than its own.
- It downloads the installer to `%APPDATA%\Kandado\updates` and keeps it only
  if size and SHA-256 match the manifest. Then the banner says "Kandado X is
  ready" with Later and Restart.
- Restart locks the vault, runs `setup.exe /S /RELAUNCH` and quits. The
  installer waits for Kandado to close, moves a running native host aside
  (`kandado-native-host.exe.old`, removed next time), installs, and opens
  Kandado again.
- A copy run from the zip (no `uninstall.exe` beside it) cannot replace
  itself, so it only says a new version is available, with a link to the release.

`KANDADO_UPDATE_URL` points the app at another folder of release files.
Development builds check only when it is set, or when Check now is pressed.

### The signing key

The secret key signs every update; whoever has it can ship one to every
install. Keep it offline and backed up; GitHub holds a copy as the
`UPDATE_SIGNING_KEY` secret:

```sh
gh secret set UPDATE_SIGNING_KEY < update-signing.key
```

A new key (`cargo run -p update-signer -- keygen <folder>`) means a new
`update_key.pub`, and only releases built with it will trust updates signed
with the new key: rotate by shipping one release that has the new public key
while still signing with the old one.
