use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, MouseButton, ParentElement, RenderOnce,
    SharedString, Styled, Window, div, prelude::FluentBuilder as _, px,
};

use super::{Icon, Tone};
use crate::{
    assets::IconName,
    theme::{KandadoTheme, TextStyles as _, color, shadow, tokens},
};

/// A confirmation or small form over a dimmed window.
///
/// Draw it as the last child of a view that fills the window. The scrim eats
/// clicks, so nothing behind the dialog can be pressed while it is open.
/// `actions` are [`super::Button`]s, cancel first; they are right-aligned.
#[derive(IntoElement)]
pub struct Dialog {
    icon: Option<IconName>,
    tone: Tone,
    title: SharedString,
    body: Option<SharedString>,
    content: Vec<AnyElement>,
    actions: Vec<AnyElement>,
    wide: bool,
}

impl Dialog {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            icon: None,
            tone: Tone::Brass,
            title: title.into(),
            body: None,
            content: Vec::new(),
            actions: Vec::new(),
            wide: false,
        }
    }

    pub fn icon(mut self, icon: IconName, tone: Tone) -> Self {
        self.icon = Some(icon);
        self.tone = tone;
        self
    }

    pub fn body(mut self, body: impl Into<SharedString>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }

    pub fn wide(mut self) -> Self {
        self.wide = true;
        self
    }
}

impl ParentElement for Dialog {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.content.extend(elements);
    }
}

impl RenderOnce for Dialog {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let kind = cx.theme_kind();
        let (tone_text, tone_ground) = self.tone.colors(p);
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(color(p.scrim))
            // The scrim eats clicks, so nothing behind it can be pressed.
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .w(px(if self.wide { 520. } else { 400. }))
                    .max_w_full()
                    .p(px(20.))
                    .rounded(px(tokens::RADIUS_LG))
                    .border_1()
                    .border_color(color(p.line))
                    .bg(color(p.surface_100))
                    .shadow(shadow(tokens::SHADOW_POP, kind))
                    .when_some(self.icon, |card, icon| {
                        card.child(
                            div()
                                .size(px(44.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(tokens::RADIUS_MD))
                                .bg(tone_ground)
                                .child(Icon::new(icon).size(px(24.)).color(tone_text)),
                        )
                    })
                    .child(
                        div()
                            .text_token(tokens::TITLE_MD)
                            .text_color(color(p.ink))
                            .child(self.title),
                    )
                    .when_some(self.body, |card, body| {
                        card.child(
                            div()
                                .text_size(px(14.))
                                .line_height(px(20.))
                                .text_color(color(p.ink_muted))
                                .child(body),
                        )
                    })
                    .when(!self.content.is_empty(), |card| {
                        card.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(tokens::SPACE_3))
                                .children(self.content),
                        )
                    })
                    .when(!self.actions.is_empty(), |card| {
                        card.child(
                            div()
                                .flex()
                                .gap(px(tokens::SPACE_2))
                                .justify_end()
                                .mt(px(6.))
                                .children(self.actions),
                        )
                    }),
            )
    }
}
