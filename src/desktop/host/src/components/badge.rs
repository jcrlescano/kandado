use gpui_kit::{
    App, Hsla, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};

use super::{
    Icon,
    hardware::{LightState, StatusLight},
};
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
            Self::Neutral => (p.ink_2, p.well),
            Self::Brass => (p.green, p.raised),
            Self::Teal => (p.green, p.raised),
            Self::Danger => (p.danger, p.danger_soft),
            Self::Warning => (p.danger, p.warn_soft),
        };
        (color(text), color(ground))
    }

    /// The solid colour of this tone, for meters and rings.
    pub fn solid(self, p: &Palette) -> Hsla {
        self.colors(p).0
    }
}

/// A short status label, drawn as an engraved tag (hw-tag): one or two words
/// in mono capitals. Danger and warning add an orange light, so the state
/// never rests on colour alone.
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
        let p = cx.palette();
        let (border, text) = match self.tone {
            Tone::Neutral => (p.tag, p.ink_2),
            Tone::Brass | Tone::Teal => (p.green, p.green),
            Tone::Danger => (p.danger, p.danger),
            Tone::Warning => (p.led_warn, p.danger),
        };
        let light = matches!(self.tone, Tone::Danger | Tone::Warning);
        // An icon only when the caller asked for one; tags are words.
        let icon = self.icon.flatten();
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .h(px(20.))
            .px(px(6.))
            .rounded(px(tokens::RADIUS_TAG))
            .border_1()
            .border_color(color(border))
            .text_color(color(text))
            .text_token(tokens::TAG)
            .whitespace_nowrap()
            .when(light && icon.is_none(), |badge| {
                badge.child(StatusLight::new(LightState::Warn).size(6.))
            })
            .when_some(icon, |badge, icon| {
                badge.child(Icon::new(icon).size(px(12.)))
            })
            .child(SharedString::from(self.label.to_uppercase()))
    }
}
