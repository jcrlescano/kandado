use gpui_kit::{
    App, FontWeight, IntoElement, ParentElement, Pixels, RenderOnce, Styled, Window, div, px, svg,
};

use crate::{
    assets::LOGO_PATH,
    theme::{DISPLAY_FACE, KandadoTheme, color},
};

/// The Kandado padlock mark, alone or with the wordmark.
///
/// The mark is always brass and the word always ink. `size` is the mark's height.
#[derive(IntoElement)]
pub struct Logo {
    size: Pixels,
    wordmark: bool,
}

impl Logo {
    pub fn new() -> Self {
        Self {
            size: px(32.),
            wordmark: false,
        }
    }

    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = size.into();
        self
    }

    /// Adds "Kandado" in the display face, for the sidebar header, onboarding and unlock.
    pub fn wordmark(mut self) -> Self {
        self.wordmark = true;
        self
    }
}

impl Default for Logo {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderOnce for Logo {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = cx.palette();
        let mark = svg()
            .path(LOGO_PATH)
            .size(self.size)
            .flex_none()
            .text_color(color(palette.brass));
        let word_size = (self.size * 0.66).round();
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(word_size * 0.35)
            .child(mark)
            .when(self.wordmark, |logo| {
                logo.child(
                    div()
                        .font_family(DISPLAY_FACE)
                        .font_weight(FontWeight::BOLD)
                        .text_size(word_size)
                        .line_height(word_size)
                        .text_color(color(palette.ink))
                        .child("Kandado"),
                )
            })
    }
}

use gpui_kit::prelude::FluentBuilder as _;
