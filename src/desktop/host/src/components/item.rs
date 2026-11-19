use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement,
    ParentElement, Pixels, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window,
    div, prelude::FluentBuilder as _, px,
};
use kandado_model::ItemKind;

use super::{
    Button, ButtonSize, Icon,
    hardware::{Tag, lip, ring},
};
use crate::{
    assets::IconName,
    theme::{KandadoTheme, TextStyles as _, color, focus_ring, tokens, tokens::Palette},
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
            Self::Brass => (p.green, p.raised),
            Self::Teal => (p.led_ok, p.raised),
            Self::Coral => (p.ink_2, p.raised),
            Self::Sky => (p.ink_2, p.raised),
            Self::Plum => (p.ink_2, p.raised),
            Self::Moss => (p.ink_2, p.raised),
        };
        (color(hue), color(soft))
    }

    /// The colour of a vault's light: Hardware's `vault-*` tokens, mapped
    /// from the names vaults are stored under as the design system's README
    /// says (teal to green, coral to orange, brass to amber, moss to graphite).
    pub fn light(self, p: &Palette) -> u32 {
        match self {
            Self::Teal => p.vault_green,
            Self::Coral => p.vault_orange,
            Self::Sky => p.vault_sky,
            Self::Plum => p.vault_plum,
            Self::Brass => p.vault_amber,
            Self::Moss => p.vault_graphite,
        }
    }

    /// The engraved word that names an item kind on its tag. Kinds are told
    /// apart by the word, not by colour.
    pub fn kind_label(kind: ItemKind) -> &'static str {
        match kind {
            ItemKind::Login => "Login",
            ItemKind::Alias => "Alias",
            ItemKind::Card => "Card",
            ItemKind::Note => "Note",
            ItemKind::Identity => "ID",
            ItemKind::Password => "Password",
            ItemKind::Passkey => "Passkey",
        }
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

/// The contrast badge on the item detail header (`hw-badge-ico`): a dark key
/// with the site's initial, or the kind's glyph.
///
/// 40 px by default, 52 px on the item detail header, 36 px small.
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
        let p = cx.palette();
        let (_, icon) = Hue::of_kind(self.kind);
        let radius = if self.size >= px(44.) {
            tokens::RADIUS_WELL
        } else {
            10.
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(self.size)
            .rounded(px(radius))
            .bg(color(p.dark_key))
            .text_color(color(p.on_dark_key))
            .shadow(vec![lip(p.dark_key_edge, 3.)])
            .map(|tile| match self.letter {
                Some(letter) => {
                    let size = (self.size * 0.42).round();
                    tile.font_family(tokens::FONT_SANS)
                        .font_weight(FontWeight::EXTRA_BOLD)
                        .text_size(size)
                        .line_height(size)
                        .child(letter)
                }
                None => tile.child(Icon::new(icon).size((self.size * 0.5).round())),
            })
    }
}

/// One item in a list (`hw-item`): title, subtitle and the kind's engraved
/// tag, with an optional badge and a Copy key. Put rows in a well.
///
/// The subtitle is the most useful secondary fact: the username for a login,
/// the last four digits for a card, the first line for a note. The open item
/// rises to a key face with a 2 px green ring.
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

    /// Marks the item open in the detail pane: a key face with a green ring.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Adds the green star.
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

    /// Shows a Copy key on hover and selection.
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
        let kind = Hue::kind_label(self.kind);

        div()
            .id(self.id.clone())
            .group(group.clone())
            .relative()
            .flex()
            .items_center()
            .gap(px(10.))
            .w_full()
            .map(|row| {
                if self.dense {
                    row.min_h(px(44.)).py(px(6.)).px(px(tokens::SPACE_2))
                } else {
                    row.min_h(px(52.)).py(px(9.)).px(px(10.))
                }
            })
            .rounded(px(10.))
            .cursor_pointer()
            .tab_index(0)
            .focus_visible(|row| row.shadow(focus_ring(p, p.well)))
            .when(selected, |row| {
                row.bg(color(p.raised))
                    .shadow(vec![lip(p.edge_key, 2.), ring(p.green)])
            })
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
                            .text_token(tokens::BODY)
                            .font_weight(if selected {
                                FontWeight::BOLD
                            } else {
                                FontWeight::SEMIBOLD
                            })
                            .child(div().truncate().child(self.title))
                            .when(self.favorite, |title| {
                                title.child(
                                    Icon::new(IconName::Star)
                                        .size(px(14.))
                                        .color(color(p.green)),
                                )
                            }),
                    )
                    .when_some(self.subtitle, |text, subtitle| {
                        text.child(
                            div()
                                .truncate()
                                .text_token(tokens::SMALL)
                                .text_color(color(p.ink_2))
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
                            Button::new((ElementId::from("copy"), self.id.to_string()), "Copy")
                                .size(ButtonSize::Sm)
                                .on_click(move |event, window, cx| {
                                    // The row itself must not also react to this click.
                                    cx.stop_propagation();
                                    handler(event, window, cx);
                                }),
                        ),
                )
            })
            .child(Tag::new(kind))
            .when_some(self.on_click, |row, handler| row.on_click(handler))
    }
}
