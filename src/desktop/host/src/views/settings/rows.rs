//! The rows of a settings page: an icon, a label with an optional
//! description, and something at the end (a value, a toggle, a badge or a
//! button).

use gpui_kit::{
    AnyElement, App, ClickEvent, CursorStyle, ElementId, InteractiveElement as _, IntoElement,
    ParentElement, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};

use crate::{
    assets::IconName,
    components::{GROUP_ROW, Icon, group_row_hover},
    theme::{KandadoTheme, TextStyles as _, color, focus_ring, tokens},
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct SettingRow {
    id: ElementId,
    icon: IconName,
    label: SharedString,
    description: Option<SharedString>,
    value: Option<SharedString>,
    trailing: Option<AnyElement>,
    chevron: bool,
    danger: bool,
    on_click: Option<ClickHandler>,
}

impl SettingRow {
    pub fn new(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            icon,
            label: label.into(),
            description: None,
            value: None,
            trailing: None,
            chevron: false,
            danger: false,
            on_click: None,
        }
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Text at the end, such as the current choice.
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// A toggle, badge or button at the end.
    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }

    /// Marks the row as opening something, and makes the whole row clickable.
    pub fn opens(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.chevron = true;
        self.on_click = Some(Box::new(handler));
        self
    }

    /// A destructive row, drawn in the danger colour and clickable.
    pub fn danger(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.danger = true;
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for SettingRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let ink = if self.danger { p.danger } else { p.ink };
        let clickable = self.on_click.is_some();
        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(tokens::SPACE_3))
            .min_h(px(56.))
            .px(px(tokens::SPACE_4))
            .py(px(tokens::SPACE_2))
            .when(clickable, |row| {
                row.relative()
                    .group(GROUP_ROW)
                    .child(group_row_hover(cx))
                    .cursor(CursorStyle::PointingHand)
                    .tab_index(0)
                    .focus_visible(|row| row.shadow(focus_ring(p, p.surface_200)))
            })
            .child(
                Icon::new(self.icon)
                    .size(px(20.))
                    .color(color(if self.danger { p.danger } else { p.ink_muted })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_token(tokens::BODY)
                            .text_color(color(ink))
                            .child(self.label),
                    )
                    .when_some(self.description, |text, description| {
                        text.child(
                            div()
                                .text_token(tokens::SMALL)
                                .text_color(color(p.ink_muted))
                                .child(description),
                        )
                    }),
            )
            .when_some(self.value, |row, value| {
                row.child(
                    div()
                        .text_token(tokens::BODY)
                        .text_color(color(p.ink_muted))
                        .child(value),
                )
            })
            .when_some(self.trailing, |row, trailing| row.child(trailing))
            .when(self.chevron, |row| {
                row.child(
                    Icon::new(IconName::Chevron)
                        .size(px(18.))
                        .color(color(p.ink_muted)),
                )
            })
            .when_some(self.on_click, |row, handler| {
                row.on_click(move |event, window, cx| handler(event, window, cx))
            })
    }
}

/// A small uppercase heading over a group of rows.
pub fn group_label(text: &'static str, cx: &App) -> impl IntoElement {
    div()
        .mt(px(tokens::SPACE_3))
        .text_token(tokens::LABEL)
        .text_color(color(cx.palette().ink_muted))
        .child(SharedString::from(text.to_uppercase()))
}
