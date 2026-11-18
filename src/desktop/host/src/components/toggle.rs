use gpui_kit::{
    App, CursorStyle, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder as _, px,
};

use super::button::DISABLED_OPACITY;
use crate::theme::{KandadoTheme, TextStyles as _, color, focus_ring, tokens};

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
        let (track, border, thumb) = if on {
            (color(p.brass), color(p.brass), color(p.on_brass))
        } else {
            (
                color(p.surface_300),
                color(p.line_strong),
                color(p.ink_muted),
            )
        };

        let switch = div()
            .relative()
            .flex_none()
            .w(px(40.))
            .h(px(24.))
            .rounded(px(tokens::RADIUS_PILL))
            .border(px(1.5))
            .border_color(border)
            .bg(track)
            .child(
                div()
                    .absolute()
                    .top(px(3.))
                    .left(px(if on { 19. } else { 3. }))
                    .size(px(15.))
                    .rounded(px(tokens::RADIUS_PILL))
                    .bg(thumb),
            );

        div()
            .id(self.id)
            .flex()
            .items_center()
            .justify_between()
            .gap(px(tokens::SPACE_4))
            .min_h(px(40.))
            .rounded(px(tokens::RADIUS_SM))
            .focus_visible(|toggle| toggle.shadow(focus_ring(p, p.surface_100)))
            .when_some(self.label, |toggle, label| {
                toggle.child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(div().text_token(tokens::BODY).child(label))
                        .when_some(self.description, |text, description| {
                            text.child(
                                div()
                                    .text_token(tokens::SMALL)
                                    .text_color(color(p.ink_muted))
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
