use gpui_kit::{
    App, Hsla, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};

use super::Icon;
use crate::{
    assets::IconName,
    theme::{KandadoTheme, TextStyles as _, color, tokens, tokens::Palette},
};

/// The meaning a status colour carries. Each has a fixed pair of text and ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    /// Counts and plain tags.
    #[default]
    Neutral,
    /// Shared, Owner.
    Brass,
    /// Healthy, Strong.
    Teal,
    /// Breached, Weak.
    Danger,
    /// Reused, Missing 2FA, Expiring.
    Warning,
}

impl Tone {
    /// `(text, ground)` for this tone.
    pub fn colors(self, p: &Palette) -> (Hsla, Hsla) {
        let (text, ground) = match self {
            Self::Neutral => (p.ink_muted, p.surface_300),
            Self::Brass => (p.brass, p.brass_soft),
            Self::Teal => (p.teal, p.teal_soft),
            Self::Danger => (p.danger, p.danger_soft),
            Self::Warning => (p.warning, p.warning_soft),
        };
        (color(text), color(ground))
    }

    /// The solid colour of this tone, for meters and rings.
    pub fn solid(self, p: &Palette) -> Hsla {
        self.colors(p).0
    }

    /// The icon that keeps the state from resting on colour alone.
    fn default_icon(self) -> Option<IconName> {
        match self {
            Self::Danger | Self::Warning => Some(IconName::Alert),
            Self::Teal => Some(IconName::Check),
            Self::Neutral | Self::Brass => None,
        }
    }
}

/// A short status label: one or two words, sentence case.
#[derive(IntoElement)]
pub struct Badge {
    label: SharedString,
    tone: Tone,
    icon: Option<Option<IconName>>,
}

impl Badge {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            tone: Tone::default(),
            icon: None,
        }
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Replaces the tone's default icon.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(Some(icon));
        self
    }

    /// Drops the icon. Only when the word alone is unambiguous.
    pub fn no_icon(mut self) -> Self {
        self.icon = Some(None);
        self
    }
}

impl RenderOnce for Badge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let (text, ground) = self.tone.colors(cx.palette());
        let icon = self.icon.unwrap_or_else(|| self.tone.default_icon());
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(tokens::SPACE_1))
            .h(px(22.))
            .px(px(tokens::SPACE_2))
            .rounded(px(tokens::RADIUS_SM))
            .bg(ground)
            .text_color(text)
            .text_token(tokens::LABEL)
            .whitespace_nowrap()
            .when_some(icon, |badge, icon| {
                badge.child(Icon::new(icon).size(px(13.)))
            })
            .child(self.label)
    }
}
