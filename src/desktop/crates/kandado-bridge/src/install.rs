//! Registering the native messaging host with the browsers (D-45).
//!
//! A browser finds a native host through a JSON manifest that names the host
//! program and the extensions allowed to start it, and a registry key under
//! `HKEY_CURRENT_USER` that points at the manifest. Everything is per user: no
//! administrator rights, nothing outside this user's registry and files.
//!
//! The manifests are written next to `kandado-native-host.exe`, which reads them
//! back to check which extension started it.

use std::{fs, io, path::Path};

use serde_json::json;

use crate::protocol::HOST_NAME;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Browser {
    Chrome,
    Edge,
    Firefox,
}

impl Browser {
    pub const ALL: [Browser; 3] = [Browser::Chrome, Browser::Edge, Browser::Firefox];

    pub fn name(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome",
            Self::Edge => "Edge",
            Self::Firefox => "Firefox",
        }
    }

    /// The key under `HKEY_CURRENT_USER` whose default value is the manifest path.
    pub fn registry_key(self) -> String {
        let vendor = match self {
            Self::Chrome => r"Software\Google\Chrome",
            Self::Edge => r"Software\Microsoft\Edge",
            Self::Firefox => r"Software\Mozilla",
        };
        format!(r"{vendor}\NativeMessagingHosts\{HOST_NAME}")
    }

    pub fn manifest_file(self) -> String {
        let suffix = match self {
            Self::Chrome => "chrome",
            Self::Edge => "edge",
            Self::Firefox => "firefox",
        };
        format!("{HOST_NAME}.{suffix}.json")
    }

    fn chromium(self) -> bool {
        !matches!(self, Self::Firefox)
    }
}

/// Which extensions may start the host.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllowedExtensions {
    /// Chrome and Edge extension IDs (32 letters a to p).
    pub chromium_ids: Vec<String>,
    /// Firefox add-on IDs, such as `kandado@kandado.app`.
    pub firefox_ids: Vec<String>,
}

impl AllowedExtensions {
    /// Keeps only well-formed IDs.
    pub fn cleaned(mut self) -> Self {
        self.chromium_ids
            .retain(|id| id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b)));
        self.firefox_ids.retain(|id| {
            !id.is_empty()
                && id.len() <= 80
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"@._-{}".contains(&b))
        });
        self
    }

    /// Whether the origin a browser passed to the host belongs to an allowed extension.
    pub fn allows(&self, origin: &str) -> bool {
        if let Some(id) = origin
            .strip_prefix("chrome-extension://")
            .map(|rest| rest.trim_end_matches('/'))
        {
            return self.chromium_ids.iter().any(|allowed| allowed == id);
        }
        self.firefox_ids.iter().any(|allowed| allowed == origin)
    }
}

/// The manifest a browser reads.
pub fn manifest(browser: Browser, host_exe: &Path, allowed: &AllowedExtensions) -> String {
    let mut manifest = json!({
        "name": HOST_NAME,
        "description": "Kandado desktop app link",
        "path": host_exe.to_string_lossy(),
        "type": "stdio",
    });
    if browser.chromium() {
        manifest["allowed_origins"] = allowed
            .chromium_ids
            .iter()
            .map(|id| format!("chrome-extension://{id}/"))
            .collect();
    } else {
        manifest["allowed_extensions"] = allowed.firefox_ids.clone().into();
    }
    serde_json::to_string_pretty(&manifest).unwrap_or_default()
}

/// Reads the allowed extensions back from the manifests in `dir` (the native
/// host does this to check who started it).
pub fn read_allowed(dir: &Path) -> AllowedExtensions {
    let mut allowed = AllowedExtensions::default();
    for browser in Browser::ALL {
        let Ok(text) = fs::read_to_string(dir.join(browser.manifest_file())) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let list = |key: &str| -> Vec<String> {
            value[key]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        for origin in list("allowed_origins") {
            if let Some(id) = origin
                .strip_prefix("chrome-extension://")
                .map(|rest| rest.trim_end_matches('/').to_owned())
            {
                allowed.chromium_ids.push(id);
            }
        }
        allowed.firefox_ids.extend(list("allowed_extensions"));
    }
    allowed.cleaned()
}

/// The per-user registry, behind a trait so the logic is tested without it.
pub trait Registry {
    /// Sets the default value of a key under `HKEY_CURRENT_USER`, creating it.
    fn set_default(&self, key: &str, value: &str) -> io::Result<()>;
    fn default_value(&self, key: &str) -> Option<String>;
    /// Removes the key; a missing key is not an error.
    fn delete(&self, key: &str) -> io::Result<()>;
}

/// Writes the manifests next to the host program and points every browser at them.
pub fn install(
    host_exe: &Path,
    allowed: &AllowedExtensions,
    registry: &dyn Registry,
) -> io::Result<()> {
    let dir = host_exe
        .parent()
        .ok_or_else(|| io::Error::other("the host program has no folder"))?;
    let allowed = allowed.clone().cleaned();
    for browser in Browser::ALL {
        let path = dir.join(browser.manifest_file());
        fs::write(&path, manifest(browser, host_exe, &allowed))?;
        registry.set_default(&browser.registry_key(), &path.to_string_lossy())?;
    }
    Ok(())
}

/// Removes the registry keys and the manifests. The host program stays: it
/// belongs to the app's installation.
pub fn uninstall(host_exe: &Path, registry: &dyn Registry) -> io::Result<()> {
    let dir = host_exe.parent().map(Path::to_path_buf);
    for browser in Browser::ALL {
        // Only remove a key that points at our own manifest.
        if registered(browser, host_exe, registry) {
            registry.delete(&browser.registry_key())?;
        }
        if let Some(dir) = &dir {
            match fs::remove_file(dir.join(browser.manifest_file())) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
    }
    Ok(())
}

/// Whether a browser's registry key points at this installation's manifest.
pub fn registered(browser: Browser, host_exe: &Path, registry: &dyn Registry) -> bool {
    let Some(dir) = host_exe.parent() else {
        return false;
    };
    let expected = dir.join(browser.manifest_file());
    registry
        .default_value(&browser.registry_key())
        .is_some_and(|value| Path::new(&value) == expected && expected.exists())
}

/// `HKEY_CURRENT_USER`.
#[cfg(windows)]
pub struct WindowsRegistry;

#[cfg(windows)]
#[allow(unsafe_code)]
impl Registry for WindowsRegistry {
    fn set_default(&self, key: &str, value: &str) -> io::Result<()> {
        use windows::{
            Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW},
            core::{HSTRING, PCWSTR},
        };
        let key = HSTRING::from(key);
        let value = HSTRING::from(value);
        let bytes = (value.len() + 1) * 2;
        // SAFETY: both strings are null-terminated and outlive the call; a
        // null value name sets the key's default value.
        let result = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &key,
                PCWSTR::null(),
                REG_SZ.0,
                Some(value.as_ptr().cast()),
                bytes as u32,
            )
        };
        match result.0 {
            0 => Ok(()),
            code => Err(io::Error::from_raw_os_error(code as i32)),
        }
    }

    fn default_value(&self, key: &str) -> Option<String> {
        use windows::{
            Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW},
            core::{HSTRING, PCWSTR},
        };
        let key = HSTRING::from(key);
        let mut buffer = vec![0u16; 1024];
        let mut size = (buffer.len() * 2) as u32;
        // SAFETY: `buffer` holds `size` bytes; Windows writes a null-terminated string.
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &key,
                PCWSTR::null(),
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if result.0 != 0 {
            return None;
        }
        let chars = (size as usize / 2).saturating_sub(1);
        Some(String::from_utf16_lossy(&buffer[..chars]))
    }

    fn delete(&self, key: &str) -> io::Result<()> {
        use windows::{
            Win32::System::Registry::{HKEY_CURRENT_USER, RegDeleteTreeW},
            core::HSTRING,
        };
        let key = HSTRING::from(key);
        // SAFETY: a valid null-terminated key path under HKCU.
        let result = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &key) };
        match result.0 {
            0 | 2 => Ok(()), // 2: not found, already gone
            code => Err(io::Error::from_raw_os_error(code as i32)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use super::*;

    #[derive(Default)]
    struct MemoryRegistry(Mutex<HashMap<String, String>>);

    impl Registry for MemoryRegistry {
        fn set_default(&self, key: &str, value: &str) -> io::Result<()> {
            self.0.lock().unwrap().insert(key.into(), value.into());
            Ok(())
        }
        fn default_value(&self, key: &str) -> Option<String> {
            self.0.lock().unwrap().get(key).cloned()
        }
        fn delete(&self, key: &str) -> io::Result<()> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
    }

    const ID: &str = "abcdefghijklmnopabcdefghijklmnop";

    fn allowed() -> AllowedExtensions {
        AllowedExtensions {
            chromium_ids: vec![ID.into(), "not-an-id".into()],
            firefox_ids: vec!["kandado@kandado.app".into(), "bad id\"".into()],
        }
    }

    #[test]
    fn manifests_name_the_host_and_only_valid_extensions() {
        let exe = Path::new(r"C:\Kandado\kandado-native-host.exe");
        let clean = allowed().cleaned();
        let chrome: serde_json::Value =
            serde_json::from_str(&manifest(Browser::Chrome, exe, &clean)).unwrap();
        assert_eq!(chrome["name"], HOST_NAME);
        assert_eq!(chrome["type"], "stdio");
        assert_eq!(chrome["path"], r"C:\Kandado\kandado-native-host.exe");
        assert_eq!(
            chrome["allowed_origins"],
            json!([format!("chrome-extension://{ID}/")])
        );
        let firefox: serde_json::Value =
            serde_json::from_str(&manifest(Browser::Firefox, exe, &clean)).unwrap();
        assert_eq!(
            firefox["allowed_extensions"],
            json!(["kandado@kandado.app"])
        );
    }

    #[test]
    fn install_points_every_browser_at_its_manifest_and_uninstall_undoes_it() {
        let dir = std::env::temp_dir().join(format!("kandado-bridge-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("kandado-native-host.exe");
        let registry = MemoryRegistry::default();

        install(&exe, &allowed(), &registry).unwrap();
        for browser in Browser::ALL {
            assert!(registered(browser, &exe, &registry), "{}", browser.name());
        }
        let back = read_allowed(&dir);
        assert!(back.allows(&format!("chrome-extension://{ID}/")));
        assert!(back.allows("kandado@kandado.app"));
        assert!(!back.allows("chrome-extension://ponmlkjihgfedcbaponmlkjihgfedcba/"));
        assert!(!back.allows("not-an-id"));

        // A key that points somewhere else is not ours to remove.
        registry
            .set_default(&Browser::Edge.registry_key(), r"C:\Other\host.json")
            .unwrap();
        uninstall(&exe, &registry).unwrap();
        assert!(!registered(Browser::Chrome, &exe, &registry));
        assert_eq!(
            registry
                .default_value(&Browser::Edge.registry_key())
                .as_deref(),
            Some(r"C:\Other\host.json")
        );
        assert!(!dir.join(Browser::Chrome.manifest_file()).exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
