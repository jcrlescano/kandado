use gpui_kit::{
    App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder as _, px,
};

use super::{Hue, Icon};
use crate::{
    assets::IconName,
    theme::{KandadoTheme, color, focus_ring, tokens},
};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// A vault as a selectable chip: name, colour and optional count.
///
/// Each vault picks one hue and an icon; the chip shows them on the hue's
/// soft ground. Vaults are named by purpose: "Personal", "Family", "Work".
#[derive(IntoElement)]
pub struct VaultChip {
    id: ElementId,
    name: SharedString,
    count: Option<usize>,
    hue: Hue,
    icon: IconName,
    active: bool,
    dropdown: bool,
    on_click: Option<ClickHandler>,
}

impl VaultChip {
    pub fn new(id: impl Into<ElementId>, name: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            count: None,
            hue: Hue::default(),
            icon: IconName::Vault,
            active: false,
            dropdown: false,
            on_click: None,
        }
    }

    pub fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }

    pub fn hue(mut self, hue: Hue) -> Self {
        self.hue = hue;
        self
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = icon;
        self
    }

    /// Marks the vault being viewed.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Adds a chevron: the chip opens the vault switcher.
    pub fn dropdown(mut self, dropdown: bool) -> Self {
        self.dropdown = dropdown;
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

impl RenderOnce for VaultChip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let (hue, soft) = self.hue.colors(p);
        div()
            .id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .gap(px(tokens::SPACE_2))
            .h(px(36.))
            .pl(px(6.))
            .pr(px(tokens::SPACE_3))
            .rounded(px(tokens::RADIUS_PILL))
            .border_1()
            .text_color(color(p.ink))
            .font_family(tokens::FONT_SANS)
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(14.))
            .cursor_pointer()
            .tab_index(0)
            .focus_visible(|chip| chip.shadow(focus_ring(p, p.surface_100)))
            .map(|chip| {
                if self.active {
                    chip.bg(color(p.brass_soft)).border_color(color(p.brass))
                } else {
                    chip.bg(color(p.surface_100))
                        .border_color(color(p.line))
                        .hover(|chip| chip.bg(color(p.surface_200)))
                }
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(24.))
                    .rounded(px(tokens::RADIUS_PILL))
                    .bg(soft)
                    .text_color(hue)
                    .child(Icon::new(self.icon).size(px(14.))),
            )
            .child(self.name)
            .when_some(self.count, |chip, count| {
                chip.child(
                    div()
                        .text_size(px(12.))
                        .text_color(color(p.ink_muted))
                        .child(count.to_string()),
                )
            })
            .when(self.dropdown, |chip| {
                chip.child(Icon::new(IconName::ChevronDown).size(px(16.)))
            })
            .when_some(self.on_click, |chip, handler| chip.on_click(handler))
    }
}
