//! The Kandado Hardware primitives every component is built from: panels,
//! wells and raised faces (Surfaces), the lip under a key, status lights,
//! engraved tags and the LCD glass. Their counterparts are the `hw-panel`,
//! `hw-well`, `hw-raised`, `hw-led`, `hw-tag` and `hw-lcd` classes in
//! `docs/design-system/project/components/bundle.css`.

use gpui_kit::{
    App, BoxShadow, Div, Hsla, IntoElement, ParentElement, RenderOnce, SharedString, Styled,
    Window, div, point, prelude::FluentBuilder as _, px,
};

use crate::theme::{KandadoTheme, TextStyles as _, color, shadow, tokens};

/// A raised module on the `body` ground: header, sidebar, list, detail.
/// Panels never nest.
pub fn panel(cx: &App) -> Div {
    let p = cx.palette();
    div()
        .bg(color(p.panel))
        .rounded(px(tokens::RADIUS_PANEL))
        .shadow(shadow(tokens::SHADOW_PANEL, cx.theme_kind()))
}

/// A recess cut into a panel: lists, groups of settings rows, value rows.
pub fn well(cx: &App) -> Div {
    let p = cx.palette();
    div()
        .bg(color(p.well))
        .rounded(px(tokens::RADIUS_WELL))
        .shadow(shadow(tokens::SHADOW_WELL, cx.theme_kind()))
}

/// A key face used as a container: the selected row, tiles, devices.
pub fn raised(cx: &App) -> Div {
    let p = cx.palette();
    div()
        .bg(color(p.raised))
        .rounded(px(tokens::RADIUS_KEY))
        .shadow(vec![lip(p.edge_key, 2.)])
}

/// The dark glass where Kandado shows codes and state. Dark in both themes;
/// everything on it uses the `lcd-*` colours.
pub fn lcd(cx: &App) -> Div {
    let p = cx.palette();
    div()
        .bg(color(p.lcd))
        .text_color(color(p.lcd_ink))
        .rounded(px(tokens::RADIUS_WELL))
        .shadow(shadow(tokens::SHADOW_LCD, cx.theme_kind()))
}

/// A hard-edged lip of `height` px under a key or a raised face.
pub fn lip(token: u32, height: f32) -> BoxShadow {
    BoxShadow {
        color: color(token),
        offset: point(px(0.), px(height)),
        blur_radius: px(0.),
        spread_radius: px(0.),
        inset: false,
    }
}

/// The 2 px ring around a selected row, a checked key or an invalid input.
pub fn ring(token: u32) -> BoxShadow {
    BoxShadow {
        color: color(token),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(2.),
        inset: false,
    }
}

/// A token colour with its alpha replaced, for the glow around a light.
fn with_alpha(token: u32, alpha: f32) -> Hsla {
    let mut hsla = color(token);
    hsla.a = alpha;
    hsla
}

/// What a status light says. Always beside a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LightState {
    /// Lit green: synced, healthy, done.
    #[default]
    Ok,
    /// Glowing orange: locked, needs attention, waiting, the current step.
    Warn,
    /// Unlit: signed out, idle, not set, a step still to come.
    Off,
}

/// A small round light that shows state (`hw-led`).
#[derive(IntoElement)]
pub struct StatusLight {
    state: LightState,
    size: f32,
    color: Option<u32>,
}

impl StatusLight {
    pub fn new(state: LightState) -> Self {
        Self {
            state,
            size: 10.,
            color: None,
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// A vault's own colour, drawn without a glow.
    pub fn color(mut self, token: u32) -> Self {
        self.color = Some(token);
        self
    }
}

impl RenderOnce for StatusLight {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let glow = |token: u32, alpha: f32, blur: f32, spread: f32| BoxShadow {
            color: with_alpha(token, alpha),
            offset: point(px(0.), px(0.)),
            blur_radius: px(blur),
            spread_radius: px(spread),
            inset: false,
        };
        let (fill, shadows) = match (self.color, self.state) {
            (Some(token), _) => (token, Vec::new()),
            (None, LightState::Ok) => (p.led_ok, vec![glow(p.led_ok, 0.22, 0., 3.)]),
            (None, LightState::Warn) => (p.led_warn, vec![glow(p.led_warn, 0.6, 8., 0.)]),
            (None, LightState::Off) => (p.tag, Vec::new()),
        };
        div()
            .flex_none()
            .size(px(self.size))
            .rounded(px(tokens::RADIUS_ROUND))
            .bg(color(fill))
            .shadow(shadows)
    }
}

/// How an engraved tag reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagTone {
    /// Item kinds and plain facts.
    #[default]
    Plain,
    /// A positive state: "This device", "On", "Now".
    Go,
    /// A difference that needs a look: "Differs".
    Warn,
}

/// An engraved label stamped onto a row (`hw-tag`): item kinds, step
/// numbers, small states. Mono capitals, one or two words.
#[derive(IntoElement)]
pub struct Tag {
    label: SharedString,
    tone: TagTone,
}

impl Tag {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            tone: TagTone::default(),
        }
    }

    pub fn tone(mut self, tone: TagTone) -> Self {
        self.tone = tone;
        self
    }
}

impl RenderOnce for Tag {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let (border, text) = match self.tone {
            TagTone::Plain => (p.tag, p.ink_2),
            TagTone::Go => (p.green, p.green),
            TagTone::Warn => (p.led_warn, p.danger),
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .px(px(6.))
            .py(px(3.))
            .rounded(px(tokens::RADIUS_TAG))
            .border_1()
            .border_color(color(border))
            .text_token(tokens::TAG)
            .text_color(color(text))
            .whitespace_nowrap()
            .child(SharedString::from(self.label.to_uppercase()))
    }
}

/// A status readout on the LCD (`hw-lcd` strip): a light, a short
/// capitalised line ("LOCKED · 09:42") and an optional label at the end.
#[derive(IntoElement)]
pub struct LcdStatus {
    light: Option<LightState>,
    text: SharedString,
    label: Option<SharedString>,
}

impl LcdStatus {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            light: None,
            text: text.into(),
            label: None,
        }
    }

    pub fn light(mut self, light: LightState) -> Self {
        self.light = Some(light);
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl RenderOnce for LcdStatus {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        lcd(cx)
            .flex()
            .items_center()
            .gap(px(tokens::SPACE_3))
            .px(px(tokens::SPACE_5))
            .py(px(tokens::SPACE_3))
            .when_some(self.light, |strip, light| {
                strip.child(StatusLight::new(light))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_token(tokens::LCD_TEXT)
                    .child(SharedString::from(self.text.to_uppercase())),
            )
            .when_some(self.label, |strip, label| {
                strip.child(
                    div()
                        .text_token(tokens::LABEL)
                        .text_color(color(p.lcd_label))
                        .child(SharedString::from(label.to_uppercase())),
                )
            })
    }
}
