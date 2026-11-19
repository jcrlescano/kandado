//! Fields (`hw-field`): an engraved label above a value sunk into a well, with
//! keys set into its end. The two-factor code is shown on the LCD.

use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, Entity, Focusable as _, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, RenderOnce, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
    component::input::{Input, InputState},
    div,
    prelude::FluentBuilder as _,
    px,
};

use super::{
    Button, ButtonSize, ButtonVariant, Icon,
    hardware::{lcd, lip, ring, well},
    kbd,
};
use crate::{
    assets::IconName,
    theme::{KandadoTheme, TextStyles as _, color, focus_ring, shadow, tokens},
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// What a concealed value shows: twelve bullets, whatever the real length.
pub const MASK: &str = "••••••••••••";

/// The engraved label above a field (`hw-label`), in capitals.
fn field_label(label: SharedString, cx: &App) -> impl IntoElement {
    div()
        .text_token(tokens::LABEL)
        .text_color(color(cx.palette().ink_3))
        .child(SharedString::from(label.to_uppercase()))
}

/// The sunk input of a field: the well's inner shadow, plus a 2 px ring in
/// `ring_token` when focused or invalid.
fn well_shadow(ring_token: Option<u32>, cx: &App) -> Vec<gpui_kit::BoxShadow> {
    let mut shadows = shadow(tokens::SHADOW_WELL, cx.theme_kind());
    if let Some(token) = ring_token {
        shadows.push(ring(token));
    }
    shadows
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

    /// What went wrong and how to fix it. Replaces the hint and rings the
    /// field in the danger colour.
    pub fn error(mut self, error: impl Into<SharedString>) -> Self {
        self.error = Some(error.into());
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A small key set into the end of the well, such as Generate or Show.
    pub fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }

    /// For passwords, card numbers, usernames and URLs.
    pub fn mono(mut self, mono: bool) -> Self {
        self.mono = mono;
        self
    }
}

impl RenderOnce for TextField {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let focused = self.state.focus_handle(cx).is_focused(window);
        let ring_token = if focused {
            Some(p.focus)
        } else if self.error.is_some() {
            Some(p.danger)
        } else {
            None
        };

        let input = div()
            .flex_1()
            .min_w_0()
            .map(|input| {
                if self.mono {
                    input.text_token(tokens::SECRET)
                } else {
                    input.text_token(tokens::BODY)
                }
            })
            .child(Input::new(&self.state).appearance(false));

        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .when_some(self.label, |field, label| {
                field.child(field_label(label, cx))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .h(px(48.))
                    .pl(px(14.))
                    .pr(px(6.))
                    .rounded(px(tokens::RADIUS_KEY))
                    .bg(color(p.well))
                    .shadow(well_shadow(ring_token, cx))
                    .when_some(self.icon, |field, icon| {
                        field.child(Icon::new(icon).size(px(18.)).color(color(p.ink_2)))
                    })
                    .child(input)
                    .when_some(self.trailing, |field, trailing| field.child(trailing)),
            )
            .map(|field| match (self.error, self.hint) {
                (Some(error), _) => field.child(
                    div()
                        .text_token(tokens::SMALL)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(p.danger))
                        .child(error),
                ),
                (None, Some(hint)) => field.child(
                    div()
                        .text_token(tokens::SMALL)
                        .text_color(color(p.ink_2))
                        .child(hint),
                ),
                (None, None) => field,
            })
    }
}

/// The search across vaults, as a well row with its shortcut keycap at the
/// end. The placeholder, set on the [`InputState`], says the scope: "Search
/// all vaults".
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
            .pl(px(14.))
            .pr(px(tokens::SPACE_2))
            .rounded(px(tokens::RADIUS_KEY))
            .bg(color(p.well))
            .shadow(well_shadow(focused.then_some(p.focus), cx))
            .child(
                Icon::new(IconName::Search)
                    .size(px(18.))
                    .color(color(p.ink_3)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_token(tokens::BODY)
                    .child(Input::new(&self.state).appearance(false)),
            )
            .when_some(self.shortcut, |search, shortcut| {
                search.child(kbd(shortcut, cx))
            })
    }
}

/// A well that stacks [`SecretField`]s and a [`TotpCode`], with hairlines
/// between them.
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
        .rounded(px(tokens::RADIUS_KEY))
        .group_hover(GROUP_ROW, move |layer| layer.bg(color(p.raised)))
}

impl RenderOnce for FieldGroup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        well(cx).flex().flex_col().overflow_hidden().children(
            self.children.into_iter().enumerate().map(|(index, child)| {
                div()
                    .when(index > 0, |row| {
                        row.border_t_1().border_color(color(p.edge))
                    })
                    .child(child)
            }),
        )
    }
}

/// How a [`SecretField`] treats its value.
enum SecretValue {
    /// Shown as it is: usernames, websites, notes.
    Plain(SharedString),
    /// Masked. `Some` only while the person has it revealed.
    Concealed(Option<SharedString>),
}

/// A read-only labelled value on the item detail, with Copy and, for
/// secrets, Show.
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
    stacked: bool,
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
            stacked: false,
            on_reveal: None,
            on_copy: None,
        }
    }

    /// Draws the field on its own, as `hw-field`: the engraved label above
    /// and the value sunk into a well row with its keys at the end. Without
    /// it the field is a row inside a [`FieldGroup`].
    pub fn stacked(mut self, stacked: bool) -> Self {
        self.stacked = stacked;
        self
    }

    /// Shows `value` as it is, with no Show key.
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

    /// For usernames and card numbers. Secrets and links are always mono.
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

    /// Shows "Copied" on the copy key, briefly after a copy.
    pub fn copied(mut self, copied: bool) -> Self {
        self.copied = copied;
        self
    }

    /// Called when the person presses Show or Hide.
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
        let use_mono = self.mono || concealed || self.link;
        let child_id = |part: &'static str| (ElementId::from(part), self.id.to_string());
        let stacked = self.stacked;
        let value = div()
            .text_token(if use_mono {
                tokens::SECRET
            } else {
                tokens::BODY
            })
            .when(self.link, |value| {
                value
                    .text_color(color(p.green))
                    .font_weight(FontWeight::SEMIBOLD)
            })
            .child(text);
        let label_text = self.label.clone();
        let label = field_label(self.label, cx);

        let row = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .map(|row| {
                if stacked {
                    row.min_h(px(48.))
                        .pl(px(14.))
                        .pr(px(6.))
                        .rounded(px(tokens::RADIUS_KEY))
                        .bg(color(p.well))
                        .shadow(shadow(tokens::SHADOW_WELL, cx.theme_kind()))
                } else {
                    row.min_h(px(64.))
                        .py(px(10.))
                        .pl(px(14.))
                        .pr(px(tokens::SPACE_3))
                }
            })
            .when_some(self.icon.filter(|_| !stacked), |field, icon| {
                field.child(Icon::new(icon).size(px(18.)).color(color(p.ink_2)))
            })
            .map(|row| {
                if stacked {
                    row.child(div().flex_1().min_w_0().truncate().child(value))
                } else {
                    row.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(label)
                            .child(div().mt(px(4.)).child(value)),
                    )
                }
            })
            .when(concealed, |field| {
                field.child(
                    Button::new(child_id("reveal"), if shown { "Hide" } else { "Show" })
                        .size(ButtonSize::Sm)
                        .when_some(self.on_reveal, |key, handler| key.on_click(handler)),
                )
            })
            .when(self.copyable, |field| {
                field.child(
                    Button::new(
                        child_id("copy"),
                        if self.copied { "Copied" } else { "Copy" },
                    )
                    .size(ButtonSize::Sm)
                    // The password's Copy is the field's main key.
                    .when(concealed, |key| key.variant(ButtonVariant::Primary))
                    .when_some(self.on_copy, |key, handler| key.on_click(handler)),
                )
            });

        if stacked {
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(field_label(label_text, cx))
                .child(row)
                .into_any_element()
        } else {
            row.into_any_element()
        }
    }
}

/// A two-factor code on the LCD, with ten countdown bars and a COPY key set
/// into the glass (`hw-lcd`).
///
/// The app computes the code and the time left (`kandado_model::totp`); this
/// only displays them. With a 30 s period the bars step once every 3 s.
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

    /// How many of the ten bars are lit: the time left, rounded up, so the
    /// last bar goes out only when the code changes.
    fn lit_bars(remaining: u32, period: u32) -> u32 {
        let period = period.max(1);
        (remaining.min(period) * 10).div_ceil(period)
    }
}

impl RenderOnce for TotpCode {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        // The last five seconds turn the digits to the LCD's warning colour.
        let low = self.remaining <= 5;
        let lit = Self::lit_bars(self.remaining, self.period);
        let child_id = |part: &'static str| (ElementId::from(part), self.id.to_string());

        lcd(cx)
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(20.))
            .py(px(tokens::SPACE_5))
            .px(px(20.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(160.))
                    .gap(px(6.))
                    .child(
                        div()
                            .text_token(tokens::LABEL)
                            .text_color(color(p.lcd_label))
                            .child(SharedString::from(self.label.to_uppercase())),
                    )
                    .child(
                        div()
                            .text_token(tokens::LCD_CODE)
                            .text_color(color(if low { p.lcd_warn } else { p.lcd_ink }))
                            .child(Self::grouped(&self.code)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(tokens::SPACE_1))
                    .children((0..10).map(|bar| {
                        div()
                            .w(px(8.))
                            .h(px(26.))
                            .rounded(px(2.))
                            .bg(color(if bar < lit { p.lcd_ink } else { p.lcd_dim }))
                    })),
            )
            .child(
                div()
                    .id(child_id("copy"))
                    .relative()
                    .flex()
                    .items_center()
                    .h(px(38.))
                    .px(px(14.))
                    .rounded(px(10.))
                    .bg(color(p.lcd_dim))
                    .text_color(color(p.lcd_ink))
                    .font_family(tokens::FONT_MONO)
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .shadow(vec![lip(p.lcd_edge, 3.)])
                    .cursor_pointer()
                    .tab_index(0)
                    .focus_visible(|key| key.shadow(focus_ring(p, p.lcd)))
                    .active(|key| key.top(px(2.)).shadow(vec![lip(p.lcd_edge, 1.)]))
                    .when_some(self.on_copy, |key, handler| key.on_click(handler))
                    .child(if self.copied { "COPIED" } else { "COPY" }),
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

    #[test]
    fn the_countdown_bars_step_every_tenth_of_the_period() {
        assert_eq!(TotpCode::lit_bars(30, 30), 10);
        assert_eq!(TotpCode::lit_bars(18, 30), 6);
        assert_eq!(TotpCode::lit_bars(28, 30), 10);
        assert_eq!(TotpCode::lit_bars(27, 30), 9);
        assert_eq!(TotpCode::lit_bars(1, 30), 1);
        assert_eq!(TotpCode::lit_bars(0, 30), 0);
        assert_eq!(TotpCode::lit_bars(45, 30), 10);
    }
}
