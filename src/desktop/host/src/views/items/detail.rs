//! The item detail pane: D-07 login, D-09 card, D-10 note, D-11 identity and
//! D-12 password, and the same pane for an item in the trash (D-41).
//!
//! `kandado-app` decides which fields a kind has and hands over a view model
//! with the secrets held back, so one view draws every kind. A secret comes
//! here only while the person has it revealed, and goes again when they hide
//! it, pick another item, or the window loses focus.

use std::{collections::HashMap, time::Duration};

use gpui_kit::{
    Context, EventEmitter, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, Styled, Window,
    component::{
        button::{Button as KitButton, ButtonVariants as _},
        h_flex,
        menu::{DropdownMenu as _, PopupMenuItem},
        scroll::ScrollableElement as _,
        v_flex,
    },
    div,
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::vault::{Field, FieldIcon, ItemDetail};
use kandado_model::{ItemKind, totp::Code};
use uuid::Uuid;

use super::super::{Services, date, explain};
use crate::{
    assets::IconName,
    components::{
        Badge, Banner, Button, ButtonVariant, Hue, Icon, IconButton, IconButtonVariant, ItemIcon,
        LightState, Link, SecretField, StatusLight, Tone, TotpCode, section_label, well,
    },
    theme::{KandadoTheme, TextStyles as _, color, tokens},
};

/// What the pane asks the shell to do. The pane itself only reveals, copies
/// and stars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailEvent {
    Edit,
    Move,
    History,
    Duplicate,
    Trash,
    Restore,
    DeleteForever,
    /// D-12: turn a saved password into a login.
    SaveAsLogin,
    /// The star changed; lists need a refresh.
    Changed,
    Toast(SharedString),
    Failed(SharedString),
}

pub struct DetailView {
    services: Services,
    id: Uuid,
    detail: Option<ItemDetail>,
    vault_name: SharedString,
    vault_hue: Hue,
    /// Secrets the person has revealed, by field. Emptied on blur.
    revealed: HashMap<usize, SharedString>,
    /// The field whose copy button shows the check mark.
    copied: Option<usize>,
    copy_serial: u64,
    code: Option<Code>,
}

impl EventEmitter<DetailEvent> for DetailView {}

pub fn icon_for(icon: FieldIcon) -> Option<IconName> {
    Some(match icon {
        FieldIcon::User => IconName::User,
        FieldIcon::Key => IconName::Key,
        FieldIcon::Globe => IconName::Globe,
        FieldIcon::Card => IconName::Card,
        FieldIcon::Clock => IconName::Clock,
        FieldIcon::Shield => IconName::Shield,
        FieldIcon::Mail => IconName::Mail,
        FieldIcon::Eye => IconName::Eye,
        FieldIcon::Id => IconName::Id,
        FieldIcon::Note => IconName::Note,
        FieldIcon::None => return None,
    })
}

/// The first letter of a title, for a login's icon tile.
pub fn letter(kind: ItemKind, title: &str) -> Option<SharedString> {
    (kind == ItemKind::Login)
        .then(|| title.chars().find(|c| c.is_alphanumeric()))
        .flatten()
        .map(|c| c.to_uppercase().collect::<String>().into())
}

/// The toast after a copy: "Password copied. Clears in 60 s."
pub fn copied_message(copied: &kandado_app::vault::Copied) -> SharedString {
    match copied.clears_in {
        Some(seconds) => format!("{} copied. Clears in {seconds} s.", copied.label).into(),
        None => format!("{} copied.", copied.label).into(),
    }
}

impl DetailView {
    pub fn new(
        services: Services,
        id: Uuid,
        vault_name: SharedString,
        vault_hue: Hue,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut view = Self {
            services,
            id,
            detail: None,
            vault_name,
            vault_hue,
            revealed: HashMap::new(),
            copied: None,
            copy_serial: 0,
            code: None,
        };
        view.reload(cx);

        // Revealed fields mask again when the window loses focus.
        cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() && !this.revealed.is_empty() {
                this.revealed.clear();
                cx.notify();
            }
        })
        .detach();

        // The two-factor code counts down once a second.
        if view.code.is_some() {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
        view
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn detail(&self) -> Option<&ItemDetail> {
        self.detail.as_ref()
    }

    /// Whether a secret is showing. Used by tests of re-masking.
    pub fn has_revealed(&self) -> bool {
        !self.revealed.is_empty()
    }

    /// Reads the item again, after an edit or a restore.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.detail = self.services.app.item_detail(self.id).ok();
        self.code = self.services.app.totp_code(self.id).ok().flatten();
        self.revealed.clear();
        cx.notify();
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let code = self.services.app.totp_code(self.id).ok().flatten();
        if code != self.code {
            self.code = code;
            cx.notify();
        }
    }

    pub fn toggle_reveal(&mut self, field: usize, cx: &mut Context<Self>) {
        if self.revealed.remove(&field).is_none() {
            match self.services.app.reveal(self.id, None, field) {
                Ok(secret) => {
                    self.revealed
                        .insert(field, secret.as_str().to_owned().into());
                }
                Err(error) => cx.emit(DetailEvent::Failed(explain(&error))),
            }
        }
        cx.notify();
    }

    /// Hides every revealed field.
    pub fn mask_all(&mut self, cx: &mut Context<Self>) {
        self.revealed.clear();
        cx.notify();
    }

    pub fn copy(&mut self, field: usize, cx: &mut Context<Self>) {
        match self.services.app.copy_field(self.id, field) {
            Ok(copied) => {
                self.copied = Some(field);
                self.copy_serial += 1;
                let serial = self.copy_serial;
                cx.emit(DetailEvent::Toast(copied_message(&copied)));
                cx.notify();
                // The check mark goes back to the copy icon after a moment.
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(1500))
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if this.copy_serial == serial {
                            this.copied = None;
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
            Err(error) => cx.emit(DetailEvent::Failed(explain(&error))),
        }
    }

    /// Ctrl B, Ctrl C and Ctrl T. Returns false if the item has no such field.
    pub fn copy_username(&mut self, cx: &mut Context<Self>) -> bool {
        self.copy_some(|detail| detail.username_field, cx)
    }

    pub fn copy_password(&mut self, cx: &mut Context<Self>) -> bool {
        self.copy_some(|detail| detail.password_field, cx)
    }

    pub fn copy_code(&mut self, cx: &mut Context<Self>) -> bool {
        self.copy_some(|detail| detail.totp_field, cx)
    }

    fn copy_some(
        &mut self,
        pick: impl Fn(&ItemDetail) -> Option<usize>,
        cx: &mut Context<Self>,
    ) -> bool {
        match self.detail.as_ref().and_then(pick) {
            Some(field) => {
                self.copy(field, cx);
                true
            }
            None => false,
        }
    }

    pub fn toggle_favourite(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = &self.detail else {
            return;
        };
        match self
            .services
            .app
            .set_favourite(&[self.id], !detail.favourite)
        {
            Ok(()) => {
                self.reload(cx);
                cx.emit(DetailEvent::Changed);
            }
            Err(error) => cx.emit(DetailEvent::Failed(explain(&error))),
        }
    }

    fn field(&self, field: &Field, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let index = field.index;
        let copied = self.copied == Some(index);
        if field.totp {
            let (digits, remaining, period) = match &self.code {
                Some(code) => (code.digits.clone(), code.remaining, code.period),
                None => ("------".to_owned(), 0, 30),
            };
            return TotpCode::new(("totp", index), digits, remaining)
                .period(period)
                .copied(copied)
                .on_copy(cx.listener(move |this, _, _, cx| this.copy(index, cx)))
                .into_any_element();
        }
        let mut row = SecretField::new(("field", index), field.label.clone())
            .stacked(true)
            .mono(field.mono)
            .link(field.link)
            .copied(copied)
            .on_copy(cx.listener(move |this, _, _, cx| this.copy(index, cx)));
        if let Some(icon) = icon_for(field.icon) {
            row = row.icon(icon);
        }
        row = match &field.value {
            Some(value) => row.plain(value.clone()),
            None => row
                .revealed(self.revealed.get(&index).cloned())
                .on_reveal(cx.listener(move |this, _, _, cx| this.toggle_reveal(index, cx))),
        };
        row.into_any_element()
    }

    fn header(&self, detail: &ItemDetail, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let trashed = detail.days_left.is_some();
        let mut icon = ItemIcon::new(detail.kind).size(px(52.));
        if let Some(letter) = letter(detail.kind, &detail.title) {
            icon = icon.letter(letter);
        }
        let title = if detail.title.is_empty() {
            SharedString::from("Untitled")
        } else {
            detail.title.clone().into()
        };

        let view = cx.entity();
        let is_password = detail.kind == ItemKind::Password;
        let more = KitButton::new("detail-more")
            .ghost()
            .tooltip("More actions")
            .child(Icon::new(IconName::More).size(px(20.)))
            .dropdown_menu(move |menu, _, _| {
                let item = |label: &'static str, event: DetailEvent| {
                    let view = view.clone();
                    PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        view.update(cx, |_, cx| cx.emit(event.clone()));
                    })
                };
                menu.when(is_password, |menu| {
                    menu.item(item("Save as login", DetailEvent::SaveAsLogin))
                })
                .item(item("Move to vault", DetailEvent::Move))
                .item(item("History", DetailEvent::History))
                .item(item("Duplicate", DetailEvent::Duplicate))
                .separator()
                .item(item("Move to trash", DetailEvent::Trash))
            });

        let actions = if trashed {
            h_flex()
                .gap(px(tokens::SPACE_2))
                .child(
                    Button::new("detail-restore", "Restore")
                        .icon(IconName::Refresh)
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(DetailEvent::Restore))),
                )
                .child(
                    Button::new("detail-delete-forever", "Delete forever")
                        .variant(ButtonVariant::DangerQuiet)
                        .icon(IconName::Trash)
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(DetailEvent::DeleteForever))),
                )
        } else {
            h_flex()
                .gap(px(tokens::SPACE_2))
                .child(
                    IconButton::new(
                        "detail-favourite",
                        IconName::Star,
                        if detail.favourite {
                            "Remove from favourites"
                        } else {
                            "Add to favourites"
                        },
                    )
                    .variant(IconButtonVariant::Secondary)
                    .checked(detail.favourite)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_favourite(cx))),
                )
                .child(
                    Button::new("detail-edit", "Edit")
                        .primary()
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(DetailEvent::Edit))),
                )
                .child(more)
        };

        let meta = format!("{} · {}", Hue::kind_label(detail.kind), self.vault_name);
        h_flex()
            .flex_wrap()
            .items_center()
            .gap(px(14.))
            .child(icon)
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(200.))
                    .child(
                        div()
                            .truncate()
                            .text_token(tokens::TITLE)
                            .text_color(color(p.ink))
                            .child(title),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(tokens::SPACE_2))
                            .mt(px(4.))
                            .text_token(tokens::LABEL)
                            .text_color(color(p.ink_3))
                            .child(
                                StatusLight::new(LightState::Ok)
                                    .size(8.)
                                    .color(self.vault_hue.light(p)),
                            )
                            .child(SharedString::from(meta.to_uppercase()))
                            .when_some(detail.strength, |line, strength| {
                                line.child(
                                    Badge::new(strength.label())
                                        .tone(match strength.score {
                                            0 | 1 => Tone::Danger,
                                            2 => Tone::Warning,
                                            _ => Tone::Teal,
                                        })
                                        .no_icon(),
                                )
                            }),
                    ),
            )
            .child(actions)
    }
}

/// The note, as a labelled well (hw-field). Notes are not concealed.
pub fn note_box(text: &str, cx: &gpui_kit::App) -> impl IntoElement {
    let p = cx.palette();
    v_flex().gap(px(6.)).child(section_label("Note", cx)).child(
        well(cx)
            .rounded(px(tokens::RADIUS_KEY))
            .py(px(tokens::SPACE_3))
            .px(px(14.))
            .children(text.lines().map(|line| {
                div()
                    .min_h(px(20.))
                    .text_size(px(14.))
                    .line_height(px(21.))
                    .text_color(color(p.ink))
                    .child(SharedString::from(line.to_owned()))
            })),
    )
}

impl Render for DetailView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let Some(detail) = self.detail.clone() else {
            return v_flex().flex_1().into_any_element();
        };
        let mut body = v_flex()
            .w_full()
            .max_w(px(820.))
            .gap(px(18.))
            .child(self.header(&detail, cx));

        if let Some(days_left) = detail.days_left {
            let days = |count: i64| if count == 1 { "day" } else { "days" };
            body = body.child(
                Banner::new(
                    Tone::Warning,
                    format!(
                        "In the trash. Kandado deletes it forever in {days_left} {}.",
                        days(days_left)
                    ),
                )
                .icon(IconName::Clock),
            );
        }
        // The two-factor code reads on the LCD, under the title.
        for field in detail.sections.iter().flat_map(|section| &section.fields) {
            if field.totp {
                body = body.child(self.field(field, cx));
            }
        }
        for section in &detail.sections {
            let fields: Vec<&Field> = section.fields.iter().filter(|field| !field.totp).collect();
            if fields.is_empty() {
                continue;
            }
            if let Some(label) = section.label {
                body = body.child(section_label(label, cx).mt(px(tokens::SPACE_1)));
            }
            body = body.child(h_flex().flex_wrap().gap(px(tokens::SPACE_3)).children(
                fields.into_iter().map(|field| {
                    div()
                        .flex_grow(1.)
                        .flex_basis(px(240.))
                        .min_w(px(240.))
                        .child(self.field(field, cx))
                }),
            ));
        }
        if !detail.note.trim().is_empty() {
            body = body.child(note_box(&detail.note, cx));
        }
        body = body.child(
            h_flex()
                .flex_wrap()
                .gap(px(tokens::SPACE_4))
                .text_token(tokens::SMALL)
                .text_color(color(p.ink_2))
                .child(format!("Created {}", date(detail.created_at)))
                .child(format!("Modified {}", date(detail.updated_at)))
                .when(detail.days_left.is_none(), |meta| {
                    meta.child(
                        Link::new("view-history", "View history")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(DetailEvent::History))),
                    )
                }),
        );

        div()
            .id("detail-pane")
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .child(v_flex().px(px(tokens::SPACE_6)).py(px(22.)).child(body))
            .into_any_element()
    }
}
