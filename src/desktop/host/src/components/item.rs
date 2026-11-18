use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement,
    ParentElement, Pixels, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window,
    div, prelude::FluentBuilder as _, px,
};
use kandado_model::ItemKind;

use super::{ButtonSize, Icon, IconButton};
use crate::{
    assets::IconName,
    theme::{
        DISPLAY_FACE, KandadoTheme, TextStyles as _, color, focus_ring, tokens, tokens::Palette,
    },
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// One of the six hues a vault or an item kind can have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Hue {
    #[default]
    Brass,
    Teal,
    Coral,
    Sky,
    Plum,
    Moss,
}

impl Hue {
    /// `(colour, soft ground)` for this hue.
    pub fn colors(self, p: &Palette) -> (Hsla, Hsla) {
        let (hue, soft) = match self {
            Self::Brass => (p.brass, p.brass_soft),
            Self::Teal => (p.teal, p.teal_soft),
            Self::Coral => (p.coral, p.coral_soft),
            Self::Sky => (p.sky, p.sky_soft),
            Self::Plum => (p.plum, p.plum_soft),
            Self::Moss => (p.moss, p.moss_soft),
        };
        (color(hue), color(soft))
    }

    /// The hue and icon that mean an item kind.
    pub fn of_kind(kind: ItemKind) -> (Self, IconName) {
        match kind {
            ItemKind::Login => (Self::Teal, IconName::Key),
            ItemKind::Alias => (Self::Moss, IconName::Alias),
            ItemKind::Card => (Self::Coral, IconName::Card),
            ItemKind::Note => (Self::Sky, IconName::Note),
            ItemKind::Identity => (Self::Plum, IconName::Id),
            ItemKind::Password => (Self::Brass, IconName::Password),
            ItemKind::Passkey => (Self::Brass, IconName::Passkey),
        }
    }
}

/// The coloured tile that tells an item's kind at a glance.
///
/// 40 px in lists (the default), 32 px dense, 56 px on the item detail header.
#[derive(IntoElement)]
pub struct ItemIcon {
    kind: ItemKind,
    size: Pixels,
    letter: Option<SharedString>,
}

impl ItemIcon {
    pub fn new(kind: ItemKind) -> Self {
        Self {
            kind,
            size: px(40.),
            letter: None,
        }
    }

    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = size.into();
        self
    }

    /// Shows a site's initial instead of the kind's glyph. Favicons are never
    /// fetched by default, for privacy.
    pub fn letter(mut self, letter: impl Into<SharedString>) -> Self {
        self.letter = Some(letter.into());
        self
    }
}

impl RenderOnce for ItemIcon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let (hue, icon) = Hue::of_kind(self.kind);
        let (text, ground) = hue.colors(cx.palette());
        let radius = if self.size >= px(36.) {
            tokens::RADIUS_MD
        } else {
            tokens::RADIUS_SM
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(self.size)
            .rounded(px(radius))
            .bg(ground)
            .text_color(text)
            .map(|tile| match self.letter {
                Some(letter) => {
                    let size = (self.size * 0.42).round();
                    tile.font_family(DISPLAY_FACE)
                        .font_weight(FontWeight::BOLD)
                        .text_size(size)
                        .line_height(size)
                        .child(letter)
                }
                None => tile.child(Icon::new(icon).size((self.size * 0.5).round())),
            })
    }
}

/// One item in a list: icon, title, subtitle, optional badge and copy button.
///
/// The subtitle is the most useful secondary fact: the username for a login,
/// the last four digits for a card, the first line for a note.
#[derive(IntoElement)]
pub struct ItemRow {
    id: ElementId,
    kind: ItemKind,
    title: SharedString,
    subtitle: Option<SharedString>,
    letter: Option<SharedString>,
    selected: bool,
    favorite: bool,
    dense: bool,
    badge: Option<AnyElement>,
    on_copy: Option<ClickHandler>,
    on_click: Option<ClickHandler>,
}

impl ItemRow {
    pub fn new(id: impl Into<ElementId>, kind: ItemKind, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            kind,
            title: title.into(),
            subtitle: None,
            letter: None,
            selected: false,
            favorite: false,
            dense: false,
            badge: None,
            on_copy: None,
            on_click: None,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn letter(mut self, letter: impl Into<SharedString>) -> Self {
        self.letter = Some(letter.into());
        self
    }

    /// Marks the item open in the detail pane and adds the brass leading bar.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Adds the brass star.
    pub fn favorite(mut self, favorite: bool) -> Self {
        self.favorite = favorite;
        self
    }

    pub fn dense(mut self, dense: bool) -> Self {
        self.dense = dense;
        self
    }

    /// A Monitor state, as a [`super::Badge`].
    pub fn badge(mut self, badge: impl IntoElement) -> Self {
        self.badge = Some(badge.into_any_element());
        self
    }

    /// Shows a copy button on hover and selection.
    pub fn on_copy(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_copy = Some(Box::new(handler));
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

impl RenderOnce for ItemRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let group: SharedString = format!("item-row-{:?}", self.id).into();
        let selected = self.selected;
        let mut icon = ItemIcon::new(self.kind).size(px(if self.dense { 32. } else { 40. }));
        if let Some(letter) = self.letter {
            icon = icon.letter(letter);
        }

        div()
            .id(self.id.clone())
            .group(group.clone())
            .relative()
            .flex()
            .items_center()
            .gap(px(tokens::SPACE_3))
            .w_full()
            .map(|row| {
                if self.dense {
                    row.min_h(px(48.)).py(px(6.)).px(px(tokens::SPACE_2))
                } else {
                    row.min_h(px(56.)).py(px(8.)).px(px(tokens::SPACE_3))
                }
            })
            .rounded(px(tokens::RADIUS_MD))
            .cursor_pointer()
            .tab_index(0)
            .focus_visible(|row| row.shadow(focus_ring(p, p.surface_100)))
            .map(|row| {
                if selected {
                    row.bg(color(p.surface_300)).child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(10.))
                            .bottom(px(10.))
                            .w(px(3.))
                            .rounded(px(tokens::RADIUS_PILL))
                            .bg(color(p.brass)),
                    )
                } else {
                    row.hover(|row| row.bg(color(p.surface_200)))
                }
            })
            .child(icon)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_token(tokens::BODY_STRONG)
                            .child(div().truncate().child(self.title))
                            .when(self.favorite, |title| {
                                title.child(
                                    Icon::new(IconName::Star)
                                        .size(px(14.))
                                        .color(color(p.brass)),
                                )
                            }),
                    )
                    .when_some(self.subtitle, |text, subtitle| {
                        text.child(
                            div()
                                .truncate()
                                .text_token(tokens::SMALL)
                                .text_color(color(p.ink_muted))
                                .child(subtitle),
                        )
                    }),
            )
            .when_some(self.badge, |row, badge| row.child(badge))
            .when_some(self.on_copy, |row, handler| {
                row.child(
                    div()
                        .when(!selected, |copy| {
                            copy.invisible().group_hover(group, |copy| copy.visible())
                        })
                        .child(
                            IconButton::new(
                                (ElementId::from("copy"), self.id.to_string()),
                                IconName::Copy,
                                "Copy password",
                            )
                            .size(ButtonSize::Sm)
                            .on_click(move |event, window, cx| {
                                // The row itself must not also react to this click.
                                cx.stop_propagation();
                                handler(event, window, cx);
                            }),
                        ),
                )
            })
            .when_some(self.on_click, |row, handler| row.on_click(handler))
    }
}
