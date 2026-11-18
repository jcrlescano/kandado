use gpui_kit::{
    App, Hsla, IntoElement, Pixels, RenderOnce, Styled, Transformation, Window, px, size, svg,
};

use crate::assets::IconName;

/// An icon from the Kandado set: 24 px outline, drawn in the current text
/// colour unless [`Icon::color`] is set.
///
/// Sizes: 16 in dense rows, 20 in buttons (the default), 24 in navigation.
#[derive(IntoElement)]
pub struct Icon {
    name: IconName,
    size: Pixels,
    color: Option<Hsla>,
    flipped: bool,
}

impl Icon {
    pub fn new(name: IconName) -> Self {
        Self {
            name,
            size: px(20.),
            color: None,
            flipped: false,
        }
    }

    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = size.into();
        self
    }

    /// Mirrors the icon left to right, so one chevron serves both directions.
    pub fn flipped(mut self, flipped: bool) -> Self {
        self.flipped = flipped;
        self
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let color = self.color.unwrap_or_else(|| window.text_style().color);
        let mut icon = svg();
        if self.flipped {
            icon = icon.with_transformation(Transformation::scale(size(-1., 1.)));
        }
        icon.path(self.name.path())
            .size(self.size)
            .flex_none()
            .text_color(color)
    }
}
