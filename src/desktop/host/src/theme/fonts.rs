//! The two Kandado Hardware typefaces, both under the SIL Open Font Licence
//! (the licence texts sit beside the files in `resources/fonts`). Bundling them
//! means the app looks the same on every machine and never asks the network
//! for a font. The browser extension ships the same files.

use std::borrow::Cow;

use gpui_kit::{App, Result};

const FILES: [&[u8]; 2] = [
    include_bytes!("../../../resources/fonts/SchibstedGrotesk.ttf"),
    include_bytes!("../../../resources/fonts/MartianMono.ttf"),
];

/// Makes the fonts named by `tokens::FONT_*` available. Call once at startup.
pub fn register(cx: &App) -> Result<()> {
    cx.text_system()
        .add_fonts(FILES.into_iter().map(Cow::Borrowed).collect())
}
