//! D-27 Create vault dialog, also used to edit one: name, colour and icon.
//! States: new, editing, name missing, saving failed.

use gpui_kit::AppContext as _;
use gpui_kit::{
    Context, Entity, EventEmitter, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
    component::{
        h_flex,
        input::{InputEvent, InputState},
    },
    div,
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::VaultInfo;
use kandado_model::vault;
use uuid::Uuid;

use super::{Services, error_line, explain};
use crate::{
    components::{Button, Dialog, Icon, TextField, section_label, window_layer},
    shell::{hue_of, vault_icon},
    theme::{KandadoTheme, color, focus_ring, tokens},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultDialogEvent {
    Saved { id: Uuid, created: bool },
    Cancelled,
}

pub struct VaultDialog {
    services: Services,
    editing: Option<VaultInfo>,
    name: Entity<InputState>,
    colour: usize,
    icon: usize,
    name_error: Option<SharedString>,
    /// Set once saved, so a second Enter does not create a second vault.
    saved: bool,
    failure: Option<SharedString>,
    _subscription: Subscription,
}

impl EventEmitter<VaultDialogEvent> for VaultDialog {}

impl VaultDialog {
    pub fn new(
        services: Services,
        editing: Option<VaultInfo>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let meta = editing
            .as_ref()
            .map(|vault| vault.meta.clone())
            .unwrap_or_default();
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Family")
                .default_value(meta.name.clone())
        });
        name.update(cx, |input, cx| input.focus(window, cx));
        let subscription = cx.subscribe(&name, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Change => {
                this.name_error = None;
                cx.notify();
            }
            InputEvent::PressEnter { .. } => this.save(cx),
            _ => {}
        });
        Self {
            services,
            colour: vault::COLOURS
                .iter()
                .position(|c| *c == meta.colour)
                .unwrap_or(0),
            icon: vault::ICONS
                .iter()
                .position(|i| *i == meta.icon)
                .unwrap_or(0),
            editing,
            name,
            name_error: None,
            saved: false,
            failure: None,
            _subscription: subscription,
        }
    }

    pub fn name_input(&self) -> &Entity<InputState> {
        &self.name
    }

    pub fn choose_colour(&mut self, index: usize, cx: &mut Context<Self>) {
        self.colour = index.min(vault::COLOURS.len() - 1);
        cx.notify();
    }

    pub fn choose_icon(&mut self, index: usize, cx: &mut Context<Self>) {
        self.icon = index.min(vault::ICONS.len() - 1);
        cx.notify();
    }

    pub fn save(&mut self, cx: &mut Context<Self>) {
        if self.saved {
            return;
        }
        let name = self.name.read(cx).value().trim().to_owned();
        if name.is_empty() {
            self.name_error = Some("Give the vault a name.".into());
            cx.notify();
            return;
        }
        let mut meta = self
            .editing
            .as_ref()
            .map(|vault| vault.meta.clone())
            .unwrap_or_default();
        meta.name = name;
        meta.colour = vault::COLOURS[self.colour].to_owned();
        meta.icon = vault::ICONS[self.icon].to_owned();
        let result = match &self.editing {
            Some(vault) => self
                .services
                .app
                .update_vault(vault.id, &meta)
                .map(|()| (vault.id, false)),
            None => self.services.app.create_vault(&meta).map(|id| (id, true)),
        };
        match result {
            Ok((id, created)) => {
                self.saved = true;
                cx.emit(VaultDialogEvent::Saved { id, created })
            }
            Err(error) => {
                self.failure = Some(explain(&error));
                cx.notify();
            }
        }
    }
}

impl Render for VaultDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let swatches =
            h_flex()
                .gap(px(tokens::SPACE_2))
                .children(vault::COLOURS.iter().enumerate().map(|(index, name)| {
                    let (ink, _) = hue_of(name).colors(p);
                    let on = index == self.colour;
                    div()
                        .id(("swatch", index))
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(36.))
                        .rounded(px(tokens::RADIUS_ROUND))
                        .border_2()
                        .border_color(color(if on { p.ink } else { p.panel }))
                        .cursor_pointer()
                        .tab_index(0)
                        .focus_visible(|swatch| swatch.shadow(focus_ring(p, p.panel)))
                        .on_click(cx.listener(move |this, _, _, cx| this.choose_colour(index, cx)))
                        .child(
                            div()
                                .size(px(24.))
                                .rounded(px(tokens::RADIUS_ROUND))
                                .bg(ink),
                        )
                }));
        let (ink, ground) = hue_of(vault::COLOURS[self.colour]).colors(p);
        let icons = h_flex().flex_wrap().gap(px(tokens::SPACE_2)).children(
            vault::ICONS.iter().enumerate().map(|(index, name)| {
                let on = index == self.icon;
                div()
                    .id(("vault-icon", index))
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(40.))
                    .rounded(px(tokens::RADIUS_KEY))
                    .cursor_pointer()
                    .tab_index(0)
                    .focus_visible(|cell| cell.shadow(focus_ring(p, p.panel)))
                    .map(|cell| {
                        if on {
                            cell.bg(ground)
                        } else {
                            cell.bg(color(p.raised))
                                .hover(|cell| cell.bg(color(p.well)))
                        }
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.choose_icon(index, cx)))
                    .child(Icon::new(vault_icon(name)).size(px(20.)).color(if on {
                        ink
                    } else {
                        color(p.ink_2)
                    }))
            }),
        );
        let editing = self.editing.is_some();
        let mut dialog = Dialog::new(if editing { "Edit vault" } else { "New vault" })
            .child(
                TextField::new(&self.name)
                    .label("Name")
                    .when_some(self.name_error.clone(), |field, error| field.error(error)),
            )
            .child(section_label("Colour", cx))
            .child(swatches)
            .child(section_label("Icon", cx))
            .child(icons);
        if let Some(failure) = self.failure.clone() {
            dialog = dialog.child(error_line(failure, cx));
        }
        window_layer(
            dialog
                .action(
                    Button::new("vault-cancel", "Cancel")
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(VaultDialogEvent::Cancelled))),
                )
                .action(
                    Button::new("vault-save", if editing { "Save" } else { "Create vault" })
                        .primary()
                        .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                ),
            window,
        )
    }
}
