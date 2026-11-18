//! `settings.json`: what the person chose. Written as they change it.
//!
//! Window position and pane widths belong in `preferences.json`, which the
//! host owns; this file holds only choices that change behaviour.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};

/// "Lock after" at D-44.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockAfter {
    Immediately,
    Minutes1,
    Minutes5,
    Minutes15,
    Minutes60,
    OnRestart,
    Never,
}

impl LockAfter {
    pub const ALL: [LockAfter; 7] = [
        Self::Immediately,
        Self::Minutes1,
        Self::Minutes5,
        Self::Minutes15,
        Self::Minutes60,
        Self::OnRestart,
        Self::Never,
    ];

    /// The idle time after which the app locks, if it locks on a timer.
    pub fn idle_limit(self) -> Option<Duration> {
        match self {
            Self::Immediately => Some(Duration::ZERO),
            Self::Minutes1 => Some(Duration::from_secs(60)),
            Self::Minutes5 => Some(Duration::from_secs(5 * 60)),
            Self::Minutes15 => Some(Duration::from_secs(15 * 60)),
            Self::Minutes60 => Some(Duration::from_secs(60 * 60)),
            Self::OnRestart | Self::Never => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Immediately => "Immediately",
            Self::Minutes1 => "1 minute",
            Self::Minutes5 => "5 minutes",
            Self::Minutes15 => "15 minutes",
            Self::Minutes60 => "1 hour",
            Self::OnRestart => "On restart",
            Self::Never => "Never",
        }
    }
}

/// "Clear clipboard after" at D-44.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardClear {
    Seconds30,
    #[default]
    Seconds60,
    Minutes2,
    Never,
}

impl ClipboardClear {
    pub const ALL: [ClipboardClear; 4] = [
        Self::Seconds30,
        Self::Seconds60,
        Self::Minutes2,
        Self::Never,
    ];

    /// How long a copied secret stays on the clipboard.
    pub fn delay(self) -> Option<Duration> {
        match self {
            Self::Seconds30 => Some(Duration::from_secs(30)),
            Self::Seconds60 => Some(Duration::from_secs(60)),
            Self::Minutes2 => Some(Duration::from_secs(120)),
            Self::Never => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Seconds30 => "30 seconds",
            Self::Seconds60 => "1 minute",
            Self::Minutes2 => "2 minutes",
            Self::Never => "Never",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub lock_after: LockAfter,
    /// Lock when the computer sleeps or the Windows session changes.
    pub lock_on_sleep: bool,
    /// Unlock with Windows Hello (D-04).
    pub windows_hello: bool,
    /// Keep the window out of screenshots and screen sharing.
    pub hide_from_capture: bool,
    pub theme: ThemeChoice,
    /// How long a copied secret stays on the clipboard.
    pub clipboard_clear: ClipboardClear,
    /// Start Kandado when the person signs in to Windows.
    pub start_at_login: bool,
    /// Closing the window keeps Kandado in the system tray.
    pub keep_in_tray: bool,
    /// Ctrl Shift Space opens Kandado from anywhere.
    pub global_shortcut: bool,
    /// The first-run import offer (D-05) has been shown and answered.
    pub import_offered: bool,
    /// The backend this app talks to.
    pub server_url: String,
    /// D-45: paired browser extensions unlock when this app does, and the
    /// native messaging host is registered with the browsers.
    pub unlock_extension: bool,
}

/// Where the app talks to unless told otherwise. Development builds use the
/// local server (`docker compose up`, `cargo run -p kandado-server`). The
/// release address is a placeholder until the backend host is chosen
/// (decision A9 in `docs/implementation-plan/decisions.md`).
pub const DEFAULT_SERVER_URL: &str = if cfg!(debug_assertions) {
    "http://127.0.0.1:8080"
} else {
    "https://api.kandado.app"
};

impl Default for Settings {
    fn default() -> Self {
        Self {
            lock_after: LockAfter::Minutes5,
            lock_on_sleep: true,
            windows_hello: false,
            hide_from_capture: false,
            theme: ThemeChoice::System,
            clipboard_clear: ClipboardClear::Seconds60,
            start_at_login: false,
            keep_in_tray: true,
            global_shortcut: true,
            import_offered: false,
            server_url: DEFAULT_SERVER_URL.to_owned(),
            unlock_extension: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("settings could not be saved: {0}")]
    Io(#[from] std::io::Error),
    #[error("settings could not be written: {0}")]
    Json(#[from] serde_json::Error),
}

impl Settings {
    /// Reads the file. A missing or unreadable file gives the defaults, so a
    /// damaged settings file never keeps anyone out of their vault.
    pub fn load(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Writes the file atomically: a crash leaves the old one or the new one.
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut temporary = path.as_os_str().to_owned();
        temporary.push(".tmp");
        let temporary = PathBuf::from(temporary);
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        fs::rename(&temporary, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("kandado-settings-{}-{name}", std::process::id()))
    }

    #[test]
    fn defaults_follow_the_design() {
        let settings = Settings::default();
        assert_eq!(settings.lock_after, LockAfter::Minutes5);
        assert!(settings.lock_on_sleep);
        assert!(!settings.windows_hello);
        assert_eq!(
            settings.clipboard_clear.delay(),
            Some(Duration::from_secs(60))
        );
        assert!(settings.keep_in_tray && !settings.start_at_login);
    }

    #[test]
    fn settings_round_trip_through_the_file() {
        let path = temp("round-trip").join("settings.json");
        let settings = Settings {
            lock_after: LockAfter::Never,
            windows_hello: true,
            ..Settings::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_missing_or_broken_file_gives_the_defaults() {
        let path = temp("broken").join("settings.json");
        assert_eq!(Settings::load(&path), Settings::default());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"{ not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let path = temp("partial").join("settings.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, br#"{"lock_after":"minutes1","future_setting":true}"#).unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(loaded.lock_after, LockAfter::Minutes1);
        assert!(loaded.lock_on_sleep);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn lock_after_limits() {
        assert_eq!(LockAfter::Immediately.idle_limit(), Some(Duration::ZERO));
        assert_eq!(
            LockAfter::Minutes15.idle_limit(),
            Some(Duration::from_secs(900))
        );
        assert_eq!(LockAfter::OnRestart.idle_limit(), None);
        assert_eq!(LockAfter::Never.idle_limit(), None);
        assert_eq!(LockAfter::ALL.len(), 7);
    }
}
