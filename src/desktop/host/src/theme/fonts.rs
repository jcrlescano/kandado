//! The three bundled typefaces, all under the SIL Open Font Licence (the
//! licence texts sit beside the files in `resources/fonts`). Bundling them
//! means the app looks the same on every machine and never asks the network
//! for a font.

use std::borrow::Cow;

use gpui_kit::{App, Result};

const FILES: [&[u8]; 3] = [
    include_bytes!("../../../resources/fonts/BricolageGrotesque.ttf"),
    include_bytes!("../../../resources/fonts/Figtree.ttf"),
    include_bytes!("../../../resources/fonts/JetBrainsMono.ttf"),
];

/// Makes the fonts named by `tokens::FONT_*` available. Call once at startup.
pub fn register(cx: &App) -> Result<()> {
    cx.text_system()
        .add_fonts(FILES.into_iter().map(Cow::Borrowed).collect())
}
