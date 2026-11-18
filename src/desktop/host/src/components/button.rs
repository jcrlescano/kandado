use gpui_kit::{
    App, ClickEvent, CursorStyle, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window,
    component::tooltip::Tooltip, div, prelude::FluentBuilder as _, px,
};

use super::Icon;
use crate::{
    assets::IconName,
    theme::{KandadoTheme, color, focus_ring, tokens},
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// What a disabled control looks like everywhere in the design system.
pub(crate) const DISABLED_OPACITY: f32 = 0.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    /// Brass. The single main action of a view.
    Primary,
    /// Other actions beside the primary one.
    #[default]
    Secondary,
    /// Low emphasis and toolbar actions.
    Ghost,
    /// Irreversible actions only, such as "Delete forever".
    Danger,
    /// Reversible destructive actions, such as "Move to trash".
    DangerQuiet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    /// 32 px, for dense toolbars.
    Sm,
    /// 40 px.
    #[default]
    Md,
    /// 48 px, for the unlock screen.
    Lg,
}

impl ButtonSize {
    fn height(self) -> f32 {
        match self {
            Self::Sm => 32.,
            Self::Md => 40.,
            Self::Lg => 48.,
        }
    }
}

/// A text button. Labels are verbs in sentence case.
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    variant: ButtonVariant,
    size: ButtonSize,
    icon: Option<IconName>,
    trailing_icon: Option<IconName>,
    block: bool,
    disabled: bool,
    on_click: Option<ClickHandler>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            variant: ButtonVariant::default(),
            size: ButtonSize::default(),
            icon: None,
            trailing_icon: None,
            block: false,
            disabled: false,
            on_click: None,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn primary(self) -> Self {
        self.variant(ButtonVariant::Primary)
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// An icon before the label, when it speeds recognition.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn trailing_icon(mut self, icon: IconName) -> Self {
        self.trailing_icon = Some(icon);
        self
    }

    /// Stretches to the width of the container.
    pub fn block(mut self) -> Self {
        self.block = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
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

impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let transparent = gpui_kit::transparent_black();
        let (background, hover, text, border) = match self.variant {
            ButtonVariant::Primary => (
                color(p.brass),
                color(p.brass_strong),
                color(p.on_brass),
                transparent,
            ),
            ButtonVariant::Secondary => (
                color(p.surface_200),
                color(p.surface_300),
                color(p.ink),
                color(p.line),
            ),
            ButtonVariant::Ghost => (transparent, color(p.surface_200), color(p.ink), transparent),
            ButtonVariant::Danger => (
                color(p.danger),
                color(p.danger),
                color(p.on_danger),
                transparent,
            ),
            ButtonVariant::DangerQuiet => (
                color(p.danger_soft),
                color(p.danger_soft),
                color(p.danger),
                transparent,
            ),
        };
        let (padding, text_size, icon_size) = match self.size {
            ButtonSize::Sm => (tokens::SPACE_3, 13., 16.),
            ButtonSize::Md => (tokens::SPACE_4, 15., 18.),
            ButtonSize::Lg => (tokens::SPACE_5, 15., 18.),
        };

        div()
            .id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(tokens::SPACE_2))
            .h(px(self.size.height()))
            .px(px(padding))
            .when(self.block, |button| button.w_full())
            .rounded(px(tokens::RADIUS_MD))
            .border_1()
            .border_color(border)
            .bg(background)
            .text_color(text)
            .font_family(tokens::FONT_SANS)
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(text_size))
            .whitespace_nowrap()
            .focus_visible(|button| button.shadow(focus_ring(p, p.surface_100)))
            .when_some(self.icon, |button, icon| {
                button.child(Icon::new(icon).size(px(icon_size)))
            })
            .child(self.label)
            .when_some(self.trailing_icon, |button, icon| {
                button.child(Icon::new(icon).size(px(16.)))
            })
            .map(|button| {
                if self.disabled {
                    button
                        .opacity(DISABLED_OPACITY)
                        .cursor(CursorStyle::OperationNotAllowed)
                } else {
                    button
                        .cursor_pointer()
                        .hover(move |button| button.bg(hover))
                        .tab_index(0)
                        .when_some(self.on_click, |button, handler| button.on_click(handler))
                }
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconButtonVariant {
    /// Inside rows and fields.
    #[default]
    Ghost,
    /// In toolbars.
    Secondary,
    /// Brass. Only for the mobile create button.
    Primary,
}

/// A square icon-only button for copy, reveal, more and toolbar actions.
///
/// `label` is required: it is the tooltip and the accessible name, and says
/// what the button acts on ("Copy password", not "Copy").
#[derive(IntoElement)]
pub struct IconButton {
    id: ElementId,
    icon: IconName,
    label: SharedString,
    variant: IconButtonVariant,
    size: ButtonSize,
    /// Draws the icon in teal, for the moment after a copy.
    confirmed: bool,
    flipped: bool,
    disabled: bool,
    on_click: Option<ClickHandler>,
}

impl IconButton {
    pub fn new(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            icon,
            label: label.into(),
            variant: IconButtonVariant::default(),
            size: ButtonSize::default(),
            confirmed: false,
            flipped: false,
            disabled: false,
            on_click: None,
        }
    }

    /// Mirrors the icon, such as a chevron that points back.
    pub fn flipped(mut self) -> Self {
        self.flipped = true;
        self
    }

    pub fn variant(mut self, variant: IconButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    pub fn confirmed(mut self, confirmed: bool) -> Self {
        self.confirmed = confirmed;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
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

impl RenderOnce for IconButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let transparent = gpui_kit::transparent_black();
        let (background, hover, text, hover_text, border) = match self.variant {
            IconButtonVariant::Ghost => (
                transparent,
                color(p.surface_300),
                color(p.ink_muted),
                color(p.ink),
                transparent,
            ),
            IconButtonVariant::Secondary => (
                color(p.surface_200),
                color(p.surface_300),
                color(p.ink),
                color(p.ink),
                color(p.line),
            ),
            IconButtonVariant::Primary => (
                color(p.brass),
                color(p.brass_strong),
                color(p.on_brass),
                color(p.on_brass),
                transparent,
            ),
        };
        let (text, hover_text) = if self.confirmed {
            (color(p.teal), color(p.teal))
        } else {
            (text, hover_text)
        };
        let side = px(self.size.height());
        let small = self.size == ButtonSize::Sm;
        let label = self.label;

        div()
            .id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(side)
            .rounded(px(if small {
                tokens::RADIUS_SM
            } else {
                tokens::RADIUS_MD
            }))
            .border_1()
            .border_color(border)
            .bg(background)
            .text_color(text)
            .focus_visible(|button| button.shadow(focus_ring(p, p.surface_100)))
            .child(
                Icon::new(self.icon)
                    .flipped(self.flipped)
                    .size(px(if small { 16. } else { 20. })),
            )
            .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
            .map(|button| {
                if self.disabled {
                    button
                        .opacity(DISABLED_OPACITY)
                        .cursor(CursorStyle::OperationNotAllowed)
                } else {
                    button
                        .cursor_pointer()
                        .hover(move |button| button.bg(hover).text_color(hover_text))
                        .tab_index(0)
                        .when_some(self.on_click, |button, handler| button.on_click(handler))
                }
            })
    }
}
