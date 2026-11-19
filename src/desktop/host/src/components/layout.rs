//! Small layout pieces the vault screens share: section labels, the
//! segmented control, filter chips, keyboard hints, the empty state, the
//! selection check, a labelled multi-line field and the layer dialogs sit on.

use gpui_kit::{
    AnyElement, App, ClickEvent, Div, ElementId, Entity, Focusable as _, FontWeight,
    InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString,
    StatefulInteractiveElement, Styled, Window, anchored,
    component::input::{Textarea, TextareaState},
    deferred, div, point,
    prelude::FluentBuilder as _,
    px,
};

use super::{
    Hue, Icon,
    hardware::{lip, raised, ring, well},
};
use crate::{
    assets::IconName,
    theme::{KandadoTheme, TextStyles as _, color, focus_ring, shadow, tokens},
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
type SelectHandler = Box<dyn Fn(usize, &mut Window, &mut App) + 'static>;

/// An engraved heading over a group of fields or rows (hw-label).
pub fn section_label(text: impl Into<SharedString>, cx: &App) -> Div {
    let text: SharedString = text.into();
    div()
        .text_token(tokens::LABEL)
        .text_color(color(cx.palette().ink_3))
        .child(SharedString::from(text.to_uppercase()))
}

/// A small raised keycap for a key or key combination, such as "Ctrl F" (hw-kbd).
pub fn kbd(text: impl Into<SharedString>, cx: &App) -> Div {
    let p = cx.palette();
    div()
        .flex_none()
        .font_family(tokens::FONT_MONO)
        .text_size(px(11.))
        .line_height(px(14.))
        .px(px(8.))
        .py(px(4.))
        .rounded(px(7.))
        .bg(color(p.raised))
        .shadow(vec![lip(p.edge_key, 2.)])
        .text_color(color(p.ink))
        .child(text.into())
}

/// The square that shows whether a row is selected (D-15).
pub fn check(on: bool, cx: &App) -> Div {
    let p = cx.palette();
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(20.))
        .rounded(px(tokens::RADIUS_KEY_SM))
        .map(|check| {
            if on {
                check.bg(color(p.green)).child(
                    Icon::new(IconName::Check)
                        .size(px(14.))
                        .color(color(p.on_green)),
                )
            } else {
                check
                    .bg(color(p.well))
                    .shadow(shadow(tokens::SHADOW_WELL, cx.theme_kind()))
            }
        })
}

/// The round mark of a single choice in a list (D-16, D-34).
pub fn radio(on: bool, cx: &App) -> Div {
    let p = cx.palette();
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(20.))
        .rounded(px(tokens::RADIUS_ROUND))
        .bg(color(p.well))
        .shadow(shadow(tokens::SHADOW_WELL, cx.theme_kind()))
        .when(on, |radio| {
            radio.child(
                div()
                    .size(px(10.))
                    .rounded(px(tokens::RADIUS_ROUND))
                    .bg(color(p.green)),
            )
        })
}

/// Two to five exclusive options sunk into a well, where the chosen one rises
/// as a key face (hw-seg): lock timers, kind filters, the theme. Option text
/// is engraved: mono capitals, short ("5 MIN", "LOGIN").
#[derive(IntoElement)]
pub struct Segmented {
    id: SharedString,
    options: Vec<SharedString>,
    selected: usize,
    on_select: Option<SelectHandler>,
}

impl Segmented {
    pub fn new(
        id: impl Into<SharedString>,
        options: impl IntoIterator<Item = impl Into<SharedString>>,
        selected: usize,
    ) -> Self {
        Self {
            id: id.into(),
            options: options.into_iter().map(Into::into).collect(),
            selected,
            on_select: None,
        }
    }

    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Segmented {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let handler = self.on_select.map(std::rc::Rc::new);
        well(cx)
            .flex()
            .gap(px(tokens::SPACE_1))
            .p(px(tokens::SPACE_1))
            .rounded(px(tokens::RADIUS_KEY))
            .children(self.options.into_iter().enumerate().map(|(index, label)| {
                let on = index == self.selected;
                let handler = handler.clone();
                div()
                    .id(ElementId::NamedInteger(self.id.clone(), index as u64))
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .h(px(34.))
                    .px(px(14.))
                    .rounded(px(tokens::RADIUS_KEY_SM))
                    .text_token(tokens::SEGMENT)
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .tab_index(0)
                    .focus_visible(|segment| segment.shadow(focus_ring(p, p.well)))
                    .map(|segment| {
                        if on {
                            segment
                                .bg(color(p.raised))
                                .text_color(color(p.ink))
                                .shadow(vec![lip(p.edge_key, 2.)])
                        } else {
                            segment
                                .font_weight(FontWeight::NORMAL)
                                .text_color(color(p.ink_2))
                        }
                    })
                    .when_some(handler, |segment, handler| {
                        segment.on_click(move |_, window, cx| handler(index, window, cx))
                    })
                    .child(SharedString::from(label.to_uppercase()))
            }))
    }
}

/// A small key that filters a list: All, Logins, Cards. Ringed in green while on.
#[derive(IntoElement)]
pub struct FilterChip {
    id: ElementId,
    label: SharedString,
    on: bool,
    on_click: Option<ClickHandler>,
}

impl FilterChip {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            on: false,
            on_click: None,
        }
    }

    pub fn on(mut self, on: bool) -> Self {
        self.on = on;
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

impl RenderOnce for FilterChip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        div()
            .id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .h(px(28.))
            .px(px(10.))
            .rounded(px(tokens::RADIUS_KEY_SM))
            .bg(color(p.raised))
            .text_token(tokens::SEGMENT)
            .cursor_pointer()
            .tab_index(0)
            .focus_visible(|chip| chip.shadow(focus_ring(p, p.panel)))
            .map(|chip| {
                if self.on {
                    chip.text_color(color(p.ink))
                        .shadow(vec![lip(p.edge_key, 2.), ring(p.green)])
                } else {
                    chip.text_color(color(p.ink_2))
                        .font_weight(FontWeight::NORMAL)
                        .shadow(vec![lip(p.edge_key, 2.)])
                        .hover(|chip| chip.text_color(color(p.ink)))
                }
            })
            .when_some(self.on_click, |chip, handler| chip.on_click(handler))
            .child(SharedString::from(self.label.to_uppercase()))
    }
}

/// What a pane shows when it has nothing: an icon, a title, a sentence and
/// the next step as buttons.
#[derive(IntoElement)]
pub struct EmptyState {
    icon: IconName,
    hue: Hue,
    title: SharedString,
    body: Option<SharedString>,
    children: Vec<AnyElement>,
}

impl EmptyState {
    pub fn new(icon: IconName, title: impl Into<SharedString>) -> Self {
        Self {
            icon,
            hue: Hue::Brass,
            title: title.into(),
            body: None,
            children: Vec::new(),
        }
    }

    pub fn hue(mut self, hue: Hue) -> Self {
        self.hue = hue;
        self
    }

    pub fn body(mut self, body: impl Into<SharedString>) -> Self {
        self.body = Some(body.into());
        self
    }
}

impl ParentElement for EmptyState {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let (ink, _) = self.hue.colors(p);
        div()
            .flex()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(tokens::SPACE_2))
            .p(px(tokens::SPACE_5))
            .child(
                raised(cx)
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(56.))
                    .mb(px(tokens::SPACE_2))
                    .rounded(px(tokens::RADIUS_WELL))
                    .child(Icon::new(self.icon).size(px(28.)).color(ink)),
            )
            .child(
                div()
                    .text_token(tokens::HEADING)
                    .text_color(color(p.ink))
                    .child(self.title),
            )
            .when_some(self.body, |empty, body| {
                empty.child(
                    div()
                        .max_w(px(360.))
                        .text_center()
                        .text_size(px(14.))
                        .line_height(px(20.))
                        .text_color(color(p.ink_2))
                        .child(body),
                )
            })
            .children(self.children)
    }
}

/// A labelled multi-line input, for notes.
#[derive(IntoElement)]
pub struct TextArea {
    state: Entity<TextareaState>,
    label: Option<SharedString>,
}

impl TextArea {
    pub fn new(state: &Entity<TextareaState>) -> Self {
        Self {
            state: state.clone(),
            label: None,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl RenderOnce for TextArea {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let focused = self.state.focus_handle(cx).is_focused(window);
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .when_some(self.label, |field, label| {
                field.child(
                    div()
                        .text_token(tokens::LABEL)
                        .text_color(color(p.ink_3))
                        .child(SharedString::from(label.to_uppercase())),
                )
            })
            .child(
                div()
                    .px(px(tokens::SPACE_3))
                    .py(px(tokens::SPACE_2))
                    .rounded(px(tokens::RADIUS_KEY))
                    .bg(color(p.well))
                    .shadow({
                        let mut shadows = shadow(tokens::SHADOW_WELL, cx.theme_kind());
                        if focused {
                            shadows.push(ring(p.focus));
                        }
                        shadows
                    })
                    .text_token(tokens::BODY)
                    .child(Textarea::new(&self.state).appearance(false)),
            )
    }
}

/// Puts a [`super::Dialog`] over the whole window, whichever pane draws it.
/// The dialog's scrim then covers every pane, so nothing behind it can be
/// pressed.
pub fn window_layer(content: impl IntoElement, window: &Window) -> impl IntoElement {
    let size = window.viewport_size();
    deferred(
        anchored()
            .position(point(px(0.), px(0.)))
            .child(div().relative().w(size.width).h(size.height).child(content)),
    )
    .with_priority(2)
}

/// A bar that fills as work gets done, such as an import.
#[derive(IntoElement)]
pub struct ProgressBar {
    fraction: f32,
}

impl ProgressBar {
    /// `fraction` is 0 to 1.
    pub fn new(fraction: f32) -> Self {
        Self {
            fraction: fraction.clamp(0., 1.),
        }
    }
}

impl RenderOnce for ProgressBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        well(cx)
            .h(px(10.))
            .w_full()
            .rounded(px(tokens::RADIUS_ROUND))
            .overflow_hidden()
            .child(
                div()
                    .h_full()
                    .w(gpui_kit::relative(self.fraction))
                    .rounded(px(tokens::RADIUS_ROUND))
                    .bg(color(p.green)),
            )
    }
}

/// The box a file is dropped on, or chosen from: its icon, a bold line and a
/// small one.
pub fn drop_zone(
    id: impl Into<ElementId>,
    icon: IconName,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    cx: &App,
) -> gpui_kit::Stateful<Div> {
    let p = cx.palette();
    div()
        .id(id)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(tokens::SPACE_1))
        .p(px(tokens::SPACE_6))
        .rounded(px(tokens::RADIUS_WELL))
        .bg(color(p.well))
        .shadow(shadow(tokens::SHADOW_WELL, cx.theme_kind()))
        .child(Icon::new(icon).size(px(28.)).color(color(p.ink_2)))
        .child(
            div()
                .text_token(tokens::BODY_STRONG)
                .text_color(color(p.ink))
                .child(title.into()),
        )
        .child(
            div()
                .text_token(tokens::SMALL)
                .text_color(color(p.ink_2))
                .child(detail.into()),
        )
}
