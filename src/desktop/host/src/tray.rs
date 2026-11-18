//! D-53 System tray menu: Open Kandado, Quick search, Generate password, Lock
//! now, Settings and Quit. Closing the window keeps Kandado here when the
//! setting at D-43 says so.
//!
//! The icon lives on the main thread, whose message loop GPUI runs; menu
//! clicks are passed to the window as [`Request`]s.

use std::sync::mpsc;

use tray_icon::{
    Icon, MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
};

use crate::{app::Request, theme::tokens};

/// The menu, in the design's order and words.
pub const ITEMS: [(&str, Request); 7] = [
    ("Open Kandado", Request::Show),
    ("Quick search…", Request::QuickSearch),
    ("Generate password", Request::GeneratePassword),
    ("Lock now", Request::Lock),
    ("Sync now", Request::Sync),
    ("Settings", Request::Settings),
    ("Quit Kandado", Request::Quit),
];

/// A 32 px tray image: the brass tile with a padlock cut out of it.
fn image() -> Icon {
    const SIZE: i32 = 32;
    let [r, g, b, _] = tokens::DARK.brass.to_be_bytes();
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            // Rounded tile.
            let corner = |cx: i32, cy: i32| (x - cx).pow(2) + (y - cy).pow(2) > 36;
            let outside = (x < 6 && y < 6 && corner(6, 6))
                || (x > 25 && y < 6 && corner(25, 6))
                || (x < 6 && y > 25 && corner(6, 25))
                || (x > 25 && y > 25 && corner(25, 25));
            // Padlock: a body and a shackle, left transparent.
            let body = (9..=22).contains(&x) && (15..=25).contains(&y);
            let ring = (x - 16).pow(2) + (y - 14).pow(2);
            let shackle = y <= 15 && (16..=36).contains(&ring);
            let keyhole = (15..=16).contains(&x) && (18..=21).contains(&y);
            let alpha = if outside || ((body || shackle) && !keyhole) {
                0
            } else {
                255
            };
            pixels.extend([r, g, b, alpha]);
        }
    }
    Icon::from_rgba(pixels, SIZE as u32, SIZE as u32).expect("valid icon size")
}

/// The tray icon, its menu, and where clicks go. Keep it alive for as long
/// as the app runs.
pub struct Tray {
    _icon: TrayIcon,
    ids: Vec<(MenuId, Request)>,
    sender: mpsc::Sender<Request>,
}

impl Tray {
    /// Adds the icon. Requests arrive on the returned receiver.
    pub fn install() -> Result<(Self, mpsc::Receiver<Request>), String> {
        let menu = Menu::new();
        let mut ids = Vec::new();
        for (index, (label, request)) in ITEMS.into_iter().enumerate() {
            // Separators as the design draws them: after Generate and after Settings.
            if index == 3 || index == 6 {
                menu.append(&PredefinedMenuItem::separator())
                    .map_err(|error| error.to_string())?;
            }
            let item = MenuItem::new(label, true, None);
            menu.append(&item).map_err(|error| error.to_string())?;
            ids.push((item.id().clone(), request));
        }
        let icon = TrayIconBuilder::new()
            .with_tooltip("Kandado")
            .with_icon(image())
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|error| error.to_string())?;
        let (sender, receiver) = mpsc::channel();
        Ok((
            Self {
                _icon: icon,
                ids,
                sender,
            },
            receiver,
        ))
    }

    /// Passes on menu clicks and left clicks on the icon. Call often.
    pub fn poll(&self) {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some((_, request)) = self.ids.iter().find(|(id, _)| *id == event.id) {
                let _ = self.sender.send(*request);
            }
        }
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            } = event
            {
                let _ = self.sender.send(Request::Show);
            }
        }
    }

    /// Sends requests from elsewhere, such as the global shortcut.
    pub fn sender(&self) -> mpsc::Sender<Request> {
        self.sender.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_follows_the_design() {
        let labels: Vec<&str> = ITEMS.iter().map(|(label, _)| *label).collect();
        assert_eq!(
            labels,
            [
                "Open Kandado",
                "Quick search…",
                "Generate password",
                "Lock now",
                "Sync now",
                "Settings",
                "Quit Kandado"
            ]
        );
        // The image is a valid 32 px RGBA picture.
        let _ = image();
    }
}
