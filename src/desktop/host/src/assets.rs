//! Everything the app draws from embedded files: Kandado's own icons and
//! logo, then GPUI Kit's bundled icons for the parts it draws itself (window
//! controls, the input's reveal button).

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

/// Generated from `resources/icons`.
mod generated {
    include!(concat!(env!("OUT_DIR"), "/icons.rs"));
}

pub use generated::{IconName, LOGO_PATH};

/// The Kandado Hardware mark for light grounds (Enamel). Unlike the icons it
/// has three colours, so it is drawn as an image, not as a one-colour mask.
pub const MARK_LIGHT_PATH: &str = "kandado/hardware/logo-light.svg";
/// The Kandado Hardware mark for dark grounds (Graphite).
pub const MARK_DARK_PATH: &str = "kandado/hardware/logo-dark.svg";

/// Files from `resources/hardware`, synced by `scripts/sync-icons.mjs`.
const HARDWARE_FILES: &[(&str, &[u8])] = &[
    (
        MARK_LIGHT_PATH,
        include_bytes!("../../resources/hardware/logos/logo-light.svg"),
    ),
    (
        MARK_DARK_PATH,
        include_bytes!("../../resources/hardware/logos/logo-dark.svg"),
    ),
];

fn embedded() -> impl Iterator<Item = &'static (&'static str, &'static [u8])> {
    generated::FILES.iter().chain(HARDWARE_FILES)
}

/// Pass to `Application::with_assets` at startup.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match embedded().find(|(name, _)| *name == path) {
            Some((_, bytes)) => Ok(Some(Cow::Borrowed(bytes))),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut entries: Vec<SharedString> = embedded()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| (*name).into())
            .collect();
        entries.extend(gpui_kit::assets::Assets.list(path)?);
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_of_the_design_system_is_embedded() {
        // The 56 icons of the Hardware set (assets/Icons/icons.json).
        assert_eq!(IconName::ALL.len(), 56);
        for icon in IconName::ALL {
            let bytes = Assets.load(icon.path()).unwrap().expect("embedded");
            assert!(bytes.starts_with(b"<svg"), "{}", icon.name());
        }
        assert_eq!(IconName::EyeOff.name(), "eye-off");
        assert_eq!(
            IconName::ChevronDown.path(),
            "kandado/icons/chevron-down.svg"
        );
        assert!(Assets.load(LOGO_PATH).unwrap().is_some());
    }

    #[test]
    fn the_kit_icons_are_still_reachable_and_unknown_paths_are_not_found() {
        assert!(Assets.load("icons/close.svg").unwrap().is_some());
        // GPUI Kit's source reports a missing file as an error, and so does this one.
        assert!(!matches!(
            Assets.load("kandado/icons/nope.svg"),
            Ok(Some(_))
        ));
    }
}
