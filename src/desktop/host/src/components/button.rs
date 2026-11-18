//! Keys (`hw-key`): every button is a key face with a lip under it, and
//! pressing moves it down 2 px. Hardware keys do not change colour on hover;
//! the pointer cursor is enough.

use gpui_kit::{
    App, BoxShadow, ClickEvent, CursorStyle, ElementId, FontWeight, InteractiveElement,
    IntoElement, ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled,
    Window, component::tooltip::Tooltip, div, prelude::FluentBuilder as _, px,
};

use super::{
    Icon,
    hardware::{lip, ring},
};
use crate::{
    assets::IconName,
    theme::{KandadoTheme, color, focus_ring, tokens, tokens::Palette},
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// What a disabled control looks like everywhere in the design system.
pub(crate) const DISABLED_OPACITY: f32 = 0.45;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    /// The green go key: the single main action of a view.
    Primary,
    /// A raised key face, for most actions: Copy, Show, Share, Cancel.
    #[default]
    Secondary,
    /// A flat key with no face, for low-emphasis actions inside rows.
    Ghost,
    /// Irreversible actions only, such as "Delete forever".
    Danger,
    /// Reversible destructive actions, such as "Move to trash": a raised face
    /// with the label in the danger colour.
    DangerQuiet,
    /// The contrast key, for Lock and similar device controls.
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    /// 32 px, inside fields and rows.
    Sm,
    /// 40 px.
    #[default]
    Md,
    /// 52 px, on the unlock screen and onboarding.
    Lg,
}

impl ButtonSize {
    fn height(self) -> f32 {
        match self {
            Self::Sm => 32.,
            Self::Md => 40.,
            Self::Lg => 52.,
        }
    }

    fn radius(self) -> f32 {
        match self {
            Self::Sm => tokens::RADIUS_KEY_SM,
            Self::Md => tokens::RADIUS_KEY,
            Self::Lg => tokens::RADIUS_WELL,
        }
    }
}

/// `(face, label, lip)` for a key. `None` lip means a flat key.
fn finish(variant: ButtonVariant, p: &Palette) -> (Option<u32>, u32, Option<u32>) {
    match variant {
        ButtonVariant::Primary => (Some(p.green), p.on_green, Some(p.green_edge)),
        ButtonVariant::Secondary => (Some(p.raised), p.ink, Some(p.edge_key)),
        ButtonVariant::Ghost => (None, p.ink, None),
        ButtonVariant::Danger => (Some(p.danger), p.on_danger, Some(p.danger_edge)),
        ButtonVariant::DangerQuiet => (Some(p.raised), p.danger, Some(p.edge_key)),
        ButtonVariant::Dark => (Some(p.dark_key), p.on_dark_key, Some(p.dark_key_edge)),
    }
}

/// The shadows of a key at rest or pressed: its lip (3 px, 1 px pressed) and,
/// when it is a checked toggle, the green ring.
fn key_shadow(edge: Option<u32>, pressed: bool, checked: bool, p: &Palette) -> Vec<BoxShadow> {
    let mut shadows: Vec<BoxShadow> = edge
        .map(|edge| lip(edge, if pressed { 1. } else { 3. }))
        .into_iter()
        .collect();
    if checked {
        shadows.push(ring(p.green));
    }
    shadows
}

/// A text key. Labels are verbs in sentence case.
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    variant: ButtonVariant,
    size: ButtonSize,
    icon: Option<IconName>,
    trailing_icon: Option<IconName>,
    block: bool,
    checked: bool,
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
            checked: false,
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

    /// For keys that toggle: draws the green ring while on.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
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
        let (face, text, edge) = finish(self.variant, p);
        let (padding, text_size, icon_size) = match self.size {
            ButtonSize::Sm => (tokens::SPACE_3, 13., 16.),
            ButtonSize::Md => (tokens::SPACE_5, 14., 18.),
            ButtonSize::Lg => (22., 16., 18.),
        };
        let checked = self.checked;
        let resting = key_shadow(edge, false, checked, p);
        let pressed = key_shadow(edge, true, checked, p);
        let mut focused = resting.clone();
        focused.extend(focus_ring(p, p.panel));

        div()
            .id(self.id)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(tokens::SPACE_2))
            .h(px(self.size.height()))
            .px(px(padding))
            .when(self.block, |button| button.w_full())
            .rounded(px(self.size.radius()))
            .when_some(face, |button, face| button.bg(color(face)))
            .shadow(resting)
            .text_color(color(text))
            .font_family(tokens::FONT_SANS)
            .font_weight(FontWeight::BOLD)
            .text_size(px(text_size))
            .line_height(px(text_size))
            .whitespace_nowrap()
            .focus_visible(move |button| button.shadow(focused))
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
                        .active(move |button| button.top(px(2.)).shadow(pressed))
                        .tab_index(0)
                        .when_some(self.on_click, |button, handler| button.on_click(handler))
                }
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconButtonVariant {
    /// A flat key, inside rows and fields.
    #[default]
    Ghost,
    /// A raised key face, in toolbars.
    Secondary,
    /// The green go key.
    Primary,
    /// The contrast key: the lock button.
    Dark,
}

/// A square icon-only key for copy, reveal, more and toolbar actions.
///
/// `label` is required: it is the tooltip and the accessible name, and says
/// what the key acts on ("Copy password", not "Copy").
#[derive(IntoElement)]
pub struct IconButton {
    id: ElementId,
    icon: IconName,
    label: SharedString,
    variant: IconButtonVariant,
    size: ButtonSize,
    /// Draws the icon in the go colour, for the moment after a copy.
    confirmed: bool,
    checked: bool,
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
            checked: false,
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

    /// For keys that toggle, such as Favourite: draws the green ring while on.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
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
        let variant = match self.variant {
            IconButtonVariant::Ghost => ButtonVariant::Ghost,
            IconButtonVariant::Secondary => ButtonVariant::Secondary,
            IconButtonVariant::Primary => ButtonVariant::Primary,
            IconButtonVariant::Dark => ButtonVariant::Dark,
        };
        let (face, text, edge) = finish(variant, p);
        // Icons on a flat key are secondary until hovered.
        let (text, hover_text) = match (self.confirmed, self.variant) {
            (true, _) => (p.green, p.green),
            (false, IconButtonVariant::Ghost) => (p.ink_2, p.ink),
            (false, _) => (text, text),
        };
        let side = px(self.size.height());
        let small = self.size == ButtonSize::Sm;
        let label = self.label;
        let checked = self.checked;
        let resting = key_shadow(edge, false, checked, p);
        let pressed = key_shadow(edge, true, checked, p);
        let mut focused = resting.clone();
        focused.extend(focus_ring(p, p.panel));

        div()
            .id(self.id)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(side)
            .rounded(px(self.size.radius()))
            .when_some(face, |button, face| button.bg(color(face)))
            .shadow(resting)
            .text_color(color(text))
            .focus_visible(move |button| button.shadow(focused))
            .child(
                Icon::new(self.icon)
                    .flipped(self.flipped)
                    .size(px(if small { 16. } else { 18. })),
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
                        .hover(move |button| button.text_color(color(hover_text)))
                        .active(move |button| button.top(px(2.)).shadow(pressed))
                        .tab_index(0)
                        .when_some(self.on_click, |button, handler| button.on_click(handler))
                }
            })
    }
}
