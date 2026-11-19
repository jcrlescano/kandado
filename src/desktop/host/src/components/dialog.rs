use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, MouseButton, ParentElement, RenderOnce,
    SharedString, Styled, Window, div, prelude::FluentBuilder as _, px,
};

use super::{Icon, Tone, hardware::raised};
use crate::{
    assets::IconName,
    theme::{KandadoTheme, TextStyles as _, color, shadow, tokens},
};

/// A confirmation or small form floating over the device on a scrim
/// (`hw-dialog`): the heading as a question, one or two sentences of
/// consequence, then keys bottom-right, Cancel first.
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
        let (tone_text, _) = self.tone.colors(p);
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
                    .gap(px(tokens::SPACE_5))
                    .w(px(if self.wide { 540. } else { 440. }))
                    .max_w_full()
                    .p(px(tokens::SPACE_6))
                    .rounded(px(tokens::RADIUS_DIALOG))
                    .bg(color(p.panel))
                    .shadow(shadow(tokens::SHADOW_POP, kind))
                    .when_some(self.icon, |card, icon| {
                        card.child(
                            raised(cx)
                                .size(px(44.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(Icon::new(icon).size(px(22.)).color(tone_text)),
                        )
                    })
                    .child(
                        div()
                            .text_token(tokens::HEADING)
                            .text_color(color(p.ink))
                            .child(self.title),
                    )
                    .when_some(self.body, |card, body| {
                        card.child(
                            div()
                                .text_token(tokens::BODY)
                                .text_color(color(p.ink_2))
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
                                .children(self.actions),
                        )
                    }),
            )
    }
}
