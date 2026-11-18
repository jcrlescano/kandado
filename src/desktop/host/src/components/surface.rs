//! Floating and structural pieces the desktop screens are assembled from:
//! toast, banner, navigation entry and table.

use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};

use super::{Icon, Tone};
use crate::{
    assets::IconName,
    theme::{self, KandadoTheme, TextStyles as _, color, focus_ring, tokens},
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// A brief confirmation that floats above the UI, bottom-centre on desktop.
///
/// This is the toast itself. Placing it, dismissing it after 4 s and keeping
/// it while hovered belong to the window's toast layer.
#[derive(IntoElement)]
pub struct Toast {
    id: ElementId,
    message: SharedString,
    danger: bool,
    icon: Option<IconName>,
    action: Option<(SharedString, ClickHandler)>,
}

impl Toast {
    /// Confirm copies with the clear time: "Password copied. Clears in 60 s."
    pub fn new(id: impl Into<ElementId>, message: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            message: message.into(),
            danger: false,
            icon: None,
            action: None,
        }
    }

    /// For failures only: "Couldn't sync. Retrying."
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// One action at most, such as "Undo" after a move to trash.
    pub fn action(
        mut self,
        label: impl Into<SharedString>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.action = Some((label.into(), Box::new(handler)));
        self
    }
}

impl RenderOnce for Toast {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let icon = self.icon.unwrap_or(if self.danger {
            IconName::Alert
        } else {
            IconName::Check
        });
        let has_action = self.action.is_some();
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(tokens::SPACE_3))
            .min_h(px(48.))
            .pl(px(tokens::SPACE_4))
            .pr(px(if has_action {
                tokens::SPACE_2
            } else {
                tokens::SPACE_4
            }))
            .rounded(px(tokens::RADIUS_LG))
            .border_1()
            .border_color(color(p.line))
            .bg(color(p.surface_100))
            .shadow(theme::shadow(tokens::SHADOW_POP, cx.theme_kind()))
            .text_color(color(p.ink))
            .font_family(tokens::FONT_SANS)
            .text_size(px(14.))
            .child(Icon::new(icon).size(px(18.)).color(color(if self.danger {
                p.danger
            } else {
                p.teal
            })))
            .child(self.message)
            .when_some(self.action, |toast, (label, handler)| {
                toast.child(
                    div()
                        .id(self.id)
                        .flex()
                        .items_center()
                        .h(px(32.))
                        .px(px(tokens::SPACE_3))
                        .rounded(px(tokens::RADIUS_SM))
                        .text_color(color(p.teal))
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .tab_index(0)
                        .hover(|action| action.bg(color(p.surface_200)))
                        .focus_visible(|action| action.shadow(focus_ring(p, p.surface_100)))
                        .on_click(handler)
                        .child(label),
                )
            })
    }
}

/// A full-width message at the top of a pane: a breach, being offline, an
/// invitation, an update that is ready.
#[derive(IntoElement)]
pub struct Banner {
    message: SharedString,
    tone: Tone,
    icon: Option<IconName>,
    action: Option<AnyElement>,
}

impl Banner {
    pub fn new(tone: Tone, message: impl Into<SharedString>) -> Self {
        Self {
            message: message.into(),
            tone,
            icon: None,
            action: None,
        }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A small [`super::Button`] at the end, such as "Retry" or "View".
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }
}

impl RenderOnce for Banner {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let (text, ground) = self.tone.colors(p);
        let icon = self.icon.unwrap_or(match self.tone {
            Tone::Danger | Tone::Warning => IconName::Alert,
            Tone::Teal => IconName::Check,
            Tone::Neutral | Tone::Brass => IconName::Shield,
        });
        div()
            .flex()
            .items_center()
            .gap(px(tokens::SPACE_3))
            .min_h(px(48.))
            .py(px(tokens::SPACE_2))
            .px(px(tokens::SPACE_4))
            .rounded(px(tokens::RADIUS_LG))
            .bg(ground)
            .child(Icon::new(icon).size(px(20.)).color(text))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_token(tokens::BODY)
                    .text_color(color(p.ink))
                    .child(self.message),
            )
            .when_some(self.action, |banner, action| banner.child(action))
    }
}

/// One entry of the sidebar or the settings navigation.
#[derive(IntoElement)]
pub struct NavItem {
    id: ElementId,
    icon: IconName,
    label: SharedString,
    selected: bool,
    /// Icon only, for the 64 px rail (D-17).
    rail: bool,
    danger: bool,
    trailing: Option<AnyElement>,
    on_click: Option<ClickHandler>,
}

impl NavItem {
    pub fn new(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            icon,
            label: label.into(),
            selected: false,
            rail: false,
            danger: false,
            trailing: None,
            on_click: None,
        }
    }

    /// A destructive entry, such as Sign out.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn rail(mut self, rail: bool) -> Self {
        self.rail = rail;
        self
    }

    /// A count or a [`super::Badge`] at the end.
    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for NavItem {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(tokens::SPACE_3))
            .h(px(40.))
            .rounded(px(tokens::RADIUS_MD))
            .cursor_pointer()
            .tab_index(0)
            .focus_visible(|item| item.shadow(focus_ring(p, p.surface_100)))
            .map(|item| {
                if self.rail {
                    item.w(px(40.)).justify_center()
                } else {
                    item.px(px(tokens::SPACE_3))
                }
            })
            .map(|item| {
                if self.selected {
                    item.bg(color(p.brass_soft)).text_color(color(p.brass))
                } else if self.danger {
                    item.text_color(color(p.danger))
                        .hover(|item| item.bg(color(p.danger_soft)))
                } else {
                    item.text_color(color(p.ink_muted))
                        .hover(|item| item.bg(color(p.surface_200)).text_color(color(p.ink)))
                }
            })
            .child(Icon::new(self.icon).size(px(24.)))
            .when(!self.rail, |item| {
                item.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_token(tokens::BODY_STRONG)
                        .text_color(color(if self.danger { p.danger } else { p.ink }))
                        .child(self.label),
                )
                .when_some(self.trailing, |item, trailing| item.child(trailing))
            })
            .when_some(self.on_click, |item, handler| item.on_click(handler))
    }
}

/// A plain table for short lists, such as secure links (D-33). Columns share
/// the width equally; the last one is 56 px wide, for a row action.
#[derive(IntoElement)]
pub struct Table {
    headers: Vec<SharedString>,
    rows: Vec<Vec<AnyElement>>,
}

impl Table {
    pub fn new(headers: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        Self {
            headers: headers.into_iter().map(Into::into).collect(),
            rows: Vec::new(),
        }
    }

    /// Adds a row. It must have one cell per header.
    pub fn row(mut self, cells: Vec<AnyElement>) -> Self {
        assert_eq!(
            cells.len(),
            self.headers.len(),
            "a table row needs one cell per column"
        );
        self.rows.push(cells);
        self
    }
}

impl RenderOnce for Table {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let last = self.headers.len().saturating_sub(1);
        let cell = move |index: usize| {
            div().min_w_0().px(px(tokens::SPACE_3)).map(|cell| {
                if index == last {
                    // A fixed width, so the header lines up with rows that hold a button.
                    cell.flex_none().w(px(56.))
                } else {
                    cell.flex_1()
                }
            })
        };
        div()
            .flex()
            .flex_col()
            .rounded(px(tokens::RADIUS_LG))
            .border_1()
            .border_color(color(p.line))
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(40.))
                    .bg(color(p.surface_200))
                    .children(self.headers.into_iter().enumerate().map(|(index, header)| {
                        cell(index)
                            .text_token(tokens::LABEL)
                            .text_color(color(p.ink_muted))
                            .child(SharedString::from(header.to_uppercase()))
                    })),
            )
            .children(self.rows.into_iter().map(|cells| {
                div()
                    .flex()
                    .items_center()
                    .min_h(px(56.))
                    .border_t_1()
                    .border_color(color(p.line))
                    .text_token(tokens::BODY)
                    .children(
                        cells
                            .into_iter()
                            .enumerate()
                            .map(|(index, content)| cell(index).child(content)),
                    )
            }))
    }
}
