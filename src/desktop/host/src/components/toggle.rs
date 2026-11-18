use gpui_kit::{
    App, CursorStyle, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder as _, px,
};

use super::{button::DISABLED_OPACITY, hardware::lip};
use crate::theme::{KandadoTheme, TextStyles as _, color, focus_ring, shadow, tokens};

type ChangeHandler = Box<dyn Fn(&bool, &mut Window, &mut App) + 'static>;

/// An on/off switch for settings that apply immediately.
///
/// `label` says what is on; `description` adds the consequence.
#[derive(IntoElement)]
pub struct Toggle {
    id: ElementId,
    label: Option<SharedString>,
    description: Option<SharedString>,
    checked: bool,
    disabled: bool,
    on_change: Option<ChangeHandler>,
}

impl Toggle {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            label: None,
            description: None,
            checked: false,
            disabled: false,
            on_change: None,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Called with the new value when the person flips the switch. Takes the
    /// value by reference so `cx.listener` can be passed directly.
    pub fn on_change(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Toggle {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let on = self.checked;
        // The track is sunk into the panel (`hw-switch`): a well when off,
        // green with a darker inner shadow when on. The knob is a key face.
        let track_shadow = if on {
            vec![gpui_kit::BoxShadow {
                color: gpui_kit::hsla(0., 0., 0., 0.3),
                offset: gpui_kit::point(px(0.), px(2.)),
                blur_radius: px(4.),
                spread_radius: px(0.),
                inset: true,
            }]
        } else {
            shadow(tokens::SHADOW_WELL, cx.theme_kind())
        };

        let switch = div()
            .relative()
            .flex_none()
            .w(px(54.))
            .h(px(30.))
            .rounded(px(tokens::RADIUS_ROUND))
            .bg(color(if on { p.green } else { p.well }))
            .shadow(track_shadow)
            .child(
                div()
                    .absolute()
                    .top(px(3.))
                    .left(px(if on { 27. } else { 3. }))
                    .size(px(24.))
                    .rounded(px(tokens::RADIUS_ROUND))
                    .bg(color(p.raised))
                    .shadow(vec![lip(p.edge_key, 2.)]),
            );

        div()
            .id(self.id)
            .flex()
            .items_center()
            .justify_between()
            .gap(px(tokens::SPACE_4))
            .min_h(px(40.))
            .rounded(px(tokens::RADIUS_KEY_SM))
            .focus_visible(|toggle| toggle.shadow(focus_ring(p, p.panel)))
            .when_some(self.label, |toggle, label| {
                toggle.child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(div().text_token(tokens::BODY_STRONG).child(label))
                        .when_some(self.description, |text, description| {
                            text.child(
                                div()
                                    .text_token(tokens::SMALL)
                                    .text_color(color(p.ink_2))
                                    .child(description),
                            )
                        }),
                )
            })
            .child(switch)
            .map(|toggle| {
                if self.disabled {
                    toggle
                        .opacity(DISABLED_OPACITY)
                        .cursor(CursorStyle::OperationNotAllowed)
                } else {
                    toggle.cursor_pointer().tab_index(0).when_some(
                        self.on_change,
                        |toggle, handler| {
                            toggle.on_click(move |_, window, cx| handler(&!on, window, cx))
                        },
                    )
                }
            })
    }
}
