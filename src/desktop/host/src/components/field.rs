use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, Entity, Focusable as _, Hsla, InteractiveElement as _,
    IntoElement, ParentElement, RenderOnce, SharedString, Styled, Window,
    component::{
        input::{Input, InputState},
        progress::ProgressCircle,
    },
    div,
    prelude::FluentBuilder as _,
    px,
};

use super::{ButtonSize, Icon, IconButton, Tone};
use crate::{
    assets::IconName,
    theme::{KandadoTheme, TextStyles as _, color, tokens},
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// What a concealed value shows: twelve bullets, whatever the real length.
pub const MASK: &str = "••••••••••••";

fn mono(element: gpui_kit::Div) -> gpui_kit::Div {
    element.font_family(tokens::FONT_MONO)
}

/// A labelled single-line input for create and edit forms.
///
/// Always give a visible label; a placeholder is an example, never the label.
/// The text lives in the [`InputState`], which the form owns.
#[derive(IntoElement)]
pub struct TextField {
    state: Entity<InputState>,
    label: Option<SharedString>,
    hint: Option<SharedString>,
    error: Option<SharedString>,
    icon: Option<IconName>,
    trailing: Option<AnyElement>,
    mono: bool,
}

impl TextField {
    pub fn new(state: &Entity<InputState>) -> Self {
        Self {
            state: state.clone(),
            label: None,
            hint: None,
            error: None,
            icon: None,
            trailing: None,
            mono: false,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Helpful context under the field.
    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// What went wrong and how to fix it. Replaces the hint and marks the field.
    pub fn error(mut self, error: impl Into<SharedString>) -> Self {
        self.error = Some(error.into());
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// An [`IconButton`] for generate or reveal.
    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }

    /// For passwords, card numbers and codes.
    pub fn mono(mut self, mono: bool) -> Self {
        self.mono = mono;
        self
    }
}

impl RenderOnce for TextField {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let focused = self.state.focus_handle(cx).is_focused(window);
        let border = if focused {
            color(p.focus_ring)
        } else if self.error.is_some() {
            color(p.danger)
        } else {
            color(p.line_strong)
        };

        let input = div()
            .flex_1()
            .min_w_0()
            .text_token(tokens::BODY)
            .when(self.mono, mono)
            .child(Input::new(&self.state).appearance(false));

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .when_some(self.label, |field, label| {
                field.child(
                    div()
                        .text_token(tokens::SMALL)
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .text_color(color(p.ink_muted))
                        .child(label),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(tokens::SPACE_2))
                    .h(px(44.))
                    .pl(px(tokens::SPACE_3))
                    .pr(px(tokens::SPACE_2))
                    .rounded(px(tokens::RADIUS_MD))
                    .bg(color(p.surface_300))
                    // 1 px normally; the focus ring is the same border drawn 2 px wide.
                    .border(px(if focused { 2. } else { 1. }))
                    .border_color(border)
                    .when_some(self.icon, |field, icon| {
                        field.child(Icon::new(icon).size(px(18.)).color(color(p.ink_muted)))
                    })
                    .child(input)
                    .when_some(self.trailing, |field, trailing| field.child(trailing)),
            )
            .map(|field| match (self.error, self.hint) {
                (Some(error), _) => field.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(tokens::SPACE_1))
                        .text_token(tokens::SMALL)
                        .text_color(color(p.danger))
                        .child(Icon::new(IconName::Alert).size(px(14.)))
                        .child(error),
                ),
                (None, Some(hint)) => field.child(
                    div()
                        .text_token(tokens::SMALL)
                        .text_color(color(p.ink_muted))
                        .child(hint),
                ),
                (None, None) => field,
            })
    }
}

/// The pill-shaped search across vaults. The placeholder, set on the
/// [`InputState`], says the scope: "Search in all vaults".
#[derive(IntoElement)]
pub struct SearchBar {
    state: Entity<InputState>,
    shortcut: Option<SharedString>,
}

impl SearchBar {
    pub fn new(state: &Entity<InputState>) -> Self {
        Self {
            state: state.clone(),
            shortcut: None,
        }
    }

    /// The keyboard shortcut shown at the end, such as "Ctrl F".
    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }
}

impl RenderOnce for SearchBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let focused = self.state.focus_handle(cx).is_focused(window);
        div()
            .flex()
            .items_center()
            .gap(px(tokens::SPACE_2))
            .h(px(40.))
            .pl(px(tokens::SPACE_3))
            .pr(px(tokens::SPACE_2))
            .rounded(px(tokens::RADIUS_PILL))
            .bg(color(p.surface_200))
            .border(px(if focused { 2. } else { 1. }))
            .border_color(color(if focused { p.focus_ring } else { p.line }))
            .child(
                Icon::new(IconName::Search)
                    .size(px(18.))
                    .color(color(p.ink_muted)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_token(tokens::BODY)
                    .child(Input::new(&self.state).appearance(false)),
            )
            .when_some(self.shortcut, |search, shortcut| {
                search.child(
                    mono(div())
                        .flex_none()
                        .text_size(px(11.))
                        .line_height(px(16.))
                        .px(px(6.))
                        .py(px(2.))
                        .rounded(px(tokens::RADIUS_SM))
                        .border_1()
                        .border_color(color(p.line_strong))
                        .text_color(color(p.ink_muted))
                        .child(shortcut),
                )
            })
    }
}

/// The raised container that stacks [`SecretField`]s and a [`TotpCode`], with
/// hairlines between them.
#[derive(IntoElement, Default)]
pub struct FieldGroup {
    children: Vec<AnyElement>,
}

impl FieldGroup {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ParentElement for FieldGroup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// The group name a clickable row inside a [`FieldGroup`] gives itself, so
/// its [`group_row_hover`] layer lights up with it.
pub const GROUP_ROW: &str = "field-group-row";

/// The hover highlight of a clickable row inside a [`FieldGroup`].
///
/// GPUI clips a group's children to a plain rectangle, so a row that painted
/// its own background on hover covered the group's rounded corners. Instead the
/// row stays transparent and carries this layer as its first child: absolutely
/// placed, inset from the row's edges and rounded, so it never reaches the
/// group's corners. Give the row `.relative().group(GROUP_ROW)`.
pub fn group_row_hover(cx: &App) -> gpui_kit::Div {
    let p = cx.palette();
    div()
        .absolute()
        .top(px(3.))
        .bottom(px(3.))
        .left(px(4.))
        .right(px(4.))
        .rounded(px(tokens::RADIUS_MD))
        .group_hover(GROUP_ROW, move |layer| layer.bg(color(p.surface_300)))
}

impl RenderOnce for FieldGroup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        div()
            .flex()
            .flex_col()
            .rounded(px(tokens::RADIUS_LG))
            .bg(color(p.surface_200))
            .overflow_hidden()
            .children(self.children.into_iter().enumerate().map(|(index, child)| {
                div()
                    .when(index > 0, |row| {
                        row.border_t_1().border_color(color(p.line))
                    })
                    .child(child)
            }))
    }
}

fn field_label(label: SharedString, ink_muted: Hsla) -> impl IntoElement {
    div()
        .text_token(tokens::LABEL)
        .text_color(ink_muted)
        .child(SharedString::from(label.to_uppercase()))
}

/// How a [`SecretField`] treats its value.
enum SecretValue {
    /// Shown as it is: usernames, websites, notes.
    Plain(SharedString),
    /// Masked. `Some` only while the person has it revealed.
    Concealed(Option<SharedString>),
}

/// A read-only labelled value on the item detail, with copy and, for secrets, reveal.
///
/// The field holds no state and never keeps a secret: the view passes the
/// revealed text only while it is revealed, and re-masks when the window
/// loses focus.
#[derive(IntoElement)]
pub struct SecretField {
    id: ElementId,
    label: SharedString,
    value: SecretValue,
    icon: Option<IconName>,
    mono: bool,
    link: bool,
    copyable: bool,
    copied: bool,
    on_reveal: Option<ClickHandler>,
    on_copy: Option<ClickHandler>,
}

impl SecretField {
    /// A concealed field, showing [`MASK`].
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            value: SecretValue::Concealed(None),
            icon: None,
            mono: false,
            link: false,
            copyable: true,
            copied: false,
            on_reveal: None,
            on_copy: None,
        }
    }

    /// Shows `value` as it is, with no reveal button.
    pub fn plain(mut self, value: impl Into<SharedString>) -> Self {
        self.value = SecretValue::Plain(value.into());
        self
    }

    /// The secret itself, while revealed. Pass `None` to mask it again.
    pub fn revealed(mut self, value: Option<SharedString>) -> Self {
        self.value = SecretValue::Concealed(value);
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// For card numbers. Revealed secrets are always mono.
    pub fn mono(mut self, mono: bool) -> Self {
        self.mono = mono;
        self
    }

    /// Draws the value in the link colour, for website URLs.
    pub fn link(mut self, link: bool) -> Self {
        self.link = link;
        self
    }

    pub fn copyable(mut self, copyable: bool) -> Self {
        self.copyable = copyable;
        self
    }

    /// Shows the check mark in place of the copy icon, briefly after a copy.
    pub fn copied(mut self, copied: bool) -> Self {
        self.copied = copied;
        self
    }

    /// Called when the person presses reveal or hide.
    pub fn on_reveal(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_reveal = Some(Box::new(handler));
        self
    }

    pub fn on_copy(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_copy = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for SecretField {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let concealed = matches!(self.value, SecretValue::Concealed(_));
        let (text, shown) = match self.value {
            SecretValue::Plain(value) => (value, true),
            SecretValue::Concealed(Some(value)) => (value, true),
            SecretValue::Concealed(None) => (SharedString::from(MASK), false),
        };
        let use_mono = self.mono || (concealed && shown);
        let label_lower = self.label.to_lowercase();
        let child_id = |part: &'static str| (ElementId::from(part), self.id.to_string());

        div()
            .flex()
            .items_center()
            .gap(px(tokens::SPACE_3))
            .min_h(px(64.))
            .py(px(10.))
            .pl(px(tokens::SPACE_4))
            .pr(px(tokens::SPACE_3))
            .when_some(self.icon, |field, icon| {
                field.child(Icon::new(icon).size(px(18.)).color(color(p.ink_muted)))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(field_label(self.label, color(p.ink_muted)))
                    .child(
                        div()
                            .mt(px(2.))
                            .text_token(if use_mono {
                                tokens::SECRET
                            } else {
                                tokens::BODY
                            })
                            .when(self.link, |value| value.text_color(color(p.link)))
                            .child(text),
                    ),
            )
            .when(concealed, |field| {
                let (icon, label) = if shown {
                    (IconName::EyeOff, "Hide")
                } else {
                    (IconName::Eye, "Reveal")
                };
                field.child(
                    IconButton::new(child_id("reveal"), icon, label)
                        .size(ButtonSize::Sm)
                        .when_some(self.on_reveal, |button, handler| button.on_click(handler)),
                )
            })
            .when(self.copyable, |field| {
                let (icon, label) = if self.copied {
                    (IconName::Check, SharedString::from("Copied"))
                } else {
                    (
                        IconName::Copy,
                        SharedString::from(format!("Copy {label_lower}")),
                    )
                };
                field.child(
                    IconButton::new(child_id("copy"), icon, label)
                        .size(ButtonSize::Sm)
                        .confirmed(self.copied)
                        .when_some(self.on_copy, |button, handler| button.on_click(handler)),
                )
            })
    }
}

/// A two-factor code with its countdown ring, inside a [`FieldGroup`].
///
/// The app computes the code and the time left (`kandado_model::totp`); this
/// only displays them.
#[derive(IntoElement)]
pub struct TotpCode {
    id: ElementId,
    code: SharedString,
    period: u32,
    remaining: u32,
    label: SharedString,
    copied: bool,
    on_copy: Option<ClickHandler>,
}

impl TotpCode {
    pub fn new(id: impl Into<ElementId>, code: impl Into<SharedString>, remaining: u32) -> Self {
        Self {
            id: id.into(),
            code: code.into(),
            period: 30,
            remaining,
            label: "Two-factor code".into(),
            copied: false,
            on_copy: None,
        }
    }

    pub fn period(mut self, period: u32) -> Self {
        self.period = period.max(1);
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    pub fn copied(mut self, copied: bool) -> Self {
        self.copied = copied;
        self
    }

    pub fn on_copy(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_copy = Some(Box::new(handler));
        self
    }

    /// Six digits read as two groups of three: "482 913".
    fn grouped(code: &str) -> String {
        if code.len() == 6 && code.is_ascii() {
            format!("{} {}", &code[..3], &code[3..])
        } else {
            code.to_owned()
        }
    }
}

impl RenderOnce for TotpCode {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        // The last five seconds turn to the warning colour.
        let low = self.remaining <= 5;
        let tone = if low { Tone::Warning } else { Tone::Teal }.solid(p);
        let fraction = self.remaining.min(self.period) as f32 / self.period as f32;
        let child_id = |part: &'static str| (ElementId::from(part), self.id.to_string());

        div()
            .flex()
            .items_center()
            .gap(px(tokens::SPACE_3))
            .min_h(px(64.))
            .py(px(10.))
            .pl(px(tokens::SPACE_4))
            .pr(px(tokens::SPACE_3))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(field_label(self.label, color(p.ink_muted)))
                    .child(
                        div()
                            .mt(px(2.))
                            .text_token(tokens::TOTP)
                            .when(low, |code| code.text_color(tone))
                            .child(Self::grouped(&self.code)),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(28.))
                    .child(
                        ProgressCircle::new(child_id("ring"))
                            .value(fraction * 100.)
                            .color(tone)
                            .size(px(28.)),
                    )
                    .child(
                        mono(div())
                            .absolute()
                            .text_size(px(9.))
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .text_color(color(p.ink_muted))
                            .child(self.remaining.to_string()),
                    ),
            )
            .child(
                IconButton::new(
                    child_id("copy"),
                    if self.copied {
                        IconName::Check
                    } else {
                        IconName::Copy
                    },
                    if self.copied {
                        "Copied"
                    } else {
                        "Copy two-factor code"
                    },
                )
                .size(ButtonSize::Sm)
                .confirmed(self.copied)
                .when_some(self.on_copy, |button, handler| button.on_click(handler)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mask_is_twelve_bullets_whatever_the_secret() {
        assert_eq!(MASK.chars().count(), 12);
        assert!(MASK.chars().all(|c| c == '•'));
    }

    #[test]
    fn six_digit_codes_are_grouped_in_threes() {
        assert_eq!(TotpCode::grouped("482913"), "482 913");
        assert_eq!(TotpCode::grouped("04829137"), "04829137");
        assert_eq!(TotpCode::grouped(""), "");
    }
}
