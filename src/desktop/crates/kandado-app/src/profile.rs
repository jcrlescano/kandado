//! Which Kandado this is: the installed app, or a development build
//! (`cargo run`). They keep everything apart, so a development build signed
//! in to a local server never touches the real vault, and the reverse:
//!
//! | | Installed | Development |
//! | --- | --- | --- |
//! | Data folder | `%APPDATA%\Kandado` | `%APPDATA%\Kandado Dev` |
//! | Credential Manager | `Kandado/…` | `Kandado Dev/…` |
//! | Windows Hello key | `Kandado` | `Kandado Dev` |
//! | Start with Windows | `Run\Kandado` | `Run\Kandado Dev` |
//! | Browser link pipe | `kandado-bridge-{SID}` | `kandado-bridge-dev-{SID}` |
//!
//! The browsers' native host registration (D-45) is one per extension, so
//! whichever app last turned on "Unlock the browser extension" owns it.

/// True for a development (debug) build.
pub const DEVELOPMENT: bool = cfg!(debug_assertions);

/// The name everything above is filed under.
pub const NAME: &str = if DEVELOPMENT {
    "Kandado Dev"
} else {
    "Kandado"
};

/// `%APPDATA%\<NAME>`: the vault, settings and downloaded updates.
pub fn data_dir() -> std::path::PathBuf {
    std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join(NAME)
}
