use gpui_kit::{
    App, FontWeight, IntoElement, ParentElement, Pixels, RenderOnce, Styled, Window, div, img,
    prelude::FluentBuilder as _, px,
};

use crate::{
    assets::{MARK_DARK_PATH, MARK_LIGHT_PATH},
    theme::{KandadoTheme, ThemeKind, color, tokens},
};

/// The Kandado padlock mark, alone or with the KANDADO logotype.
///
/// The mark is a green body with a keyhole in the ground colour and an ink
/// shackle, from the Hardware Logos group, one file per theme. `size` is the
/// mark's height; its width is 30/32 of it.
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

    /// Adds the logotype, KANDADO in spaced mono capitals, for the header,
    /// onboarding and unlock.
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
        let path = match cx.theme_kind() {
            ThemeKind::Enamel => MARK_LIGHT_PATH,
            ThemeKind::Graphite => MARK_DARK_PATH,
        };
        let mark = img(path)
            .flex_none()
            .h(self.size)
            .w((self.size * (30. / 32.)).round());
        // 13 px beside a 30 px mark in the design.
        let word_size = (self.size * 0.44).round().max(px(11.));
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .child(mark)
            .when(self.wordmark, |logo| {
                logo.child(
                    div()
                        .font_family(tokens::FONT_MONO)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(word_size)
                        .line_height(word_size)
                        .text_color(color(palette.ink))
                        // GPUI has no letter spacing; thin spaces stand in for 0.18 em.
                        .child("K\u{2009}A\u{2009}N\u{2009}D\u{2009}A\u{2009}D\u{2009}O"),
                )
            })
    }
}
