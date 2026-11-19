//! D-33 Secure links, under Tools › Secure links. States: loading, list,
//! empty, could not load (with Retry), revoking one.

use gpui_kit::{
    AnyElement, Context, EventEmitter, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, Styled, Window,
    component::{h_flex, scroll::ScrollableElement as _, v_flex},
    div,
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::links::LinkRow;
use kandado_protocol::LinkState;
use time::OffsetDateTime;
use uuid::Uuid;

use super::{super::Services, day, remaining, views};
use crate::{
    assets::IconName,
    components::{Button, ButtonSize, ButtonVariant, LightState, StatusLight, Table, Tag, lcd},
    theme::{KandadoTheme, TextStyles as _, color, tokens},
    views::{error_line, explain, heading},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinksEvent {
    /// "Back to vault" from the empty state.
    BackToVault,
    Toast(SharedString),
}

enum Load {
    Loading,
    Loaded(Vec<LinkRow>),
    Failed(SharedString),
}

pub struct LinksPage {
    services: Services,
    load: Load,
    revoking: Option<Uuid>,
    failure: Option<SharedString>,
}

impl EventEmitter<LinksEvent> for LinksPage {}

impl LinksPage {
    pub fn new(services: Services, cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            services,
            load: Load::Loading,
            revoking: None,
            failure: None,
        };
        page.reload(cx);
        page
    }

    pub fn rows(&self) -> Option<&[LinkRow]> {
        match &self.load {
            Load::Loaded(rows) => Some(rows),
            _ => None,
        }
    }

    /// True when the list could not be loaded.
    pub fn failed(&self) -> bool {
        matches!(self.load, Load::Failed(_))
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.load, Load::Loaded(_)) {
            self.load = Load::Loading;
        }
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let result = services.tasks.run(async move { app.links().await }).await;
            let _ = this.update(cx, |this, cx| {
                this.load = match result {
                    Ok(rows) => Load::Loaded(rows),
                    Err(error) => Load::Failed(explain(&error)),
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub fn revoke(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.revoking.is_some() {
            return;
        }
        self.revoking = Some(id);
        self.failure = None;
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let result = services
                .tasks
                .run(async move { app.revoke_link(id).await })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.revoking = None;
                match result {
                    Ok(()) => {
                        cx.emit(LinksEvent::Toast("Link revoked.".into()));
                        this.reload(cx);
                    }
                    Err(error) => {
                        this.failure = Some(explain(&error));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn table(&self, rows: &[LinkRow], cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let now = OffsetDateTime::now_utc();
        let mono = |text: String| {
            div()
                .font_family(tokens::FONT_MONO)
                .text_size(px(12.))
                .text_color(color(p.ink))
                .child(SharedString::from(text))
                .into_any_element()
        };
        let mut table = Table::new(["Item", "Created", "Expires", "Views", ""]).last_width(px(96.));
        for row in rows {
            let info = &row.info;
            let active = info.state == LinkState::Active;
            let title = row
                .title
                .clone()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| {
                    if row.title.is_some() {
                        "Untitled".to_owned()
                    } else {
                        "Deleted item".to_owned()
                    }
                });
            let item = h_flex()
                .items_center()
                .gap(px(10.))
                .min_w_0()
                .child(StatusLight::new(if active {
                    LightState::Ok
                } else {
                    LightState::Off
                }))
                .child(
                    div()
                        .truncate()
                        .font_weight(gpui_kit::FontWeight::BOLD)
                        .text_color(color(if row.title.is_some() { p.ink } else { p.ink_2 }))
                        .child(SharedString::from(title)),
                )
                .into_any_element();
            let expires = match info.state {
                LinkState::Active => remaining(info.expires_at, now),
                LinkState::Expired => format!("ENDED {}", day(info.expires_at)),
                LinkState::UsedUp => "USED UP".to_owned(),
                LinkState::Revoked => "REVOKED".to_owned(),
            };
            let action = if active {
                let id = info.id;
                Button::new(("revoke-link", info.id.as_u128() as usize), "Revoke")
                    .size(ButtonSize::Sm)
                    .variant(ButtonVariant::DangerQuiet)
                    .disabled(self.revoking.is_some())
                    .on_click(cx.listener(move |this, _, _, cx| this.revoke(id, cx)))
                    .into_any_element()
            } else {
                Tag::new(match info.state {
                    LinkState::Expired => "Expired",
                    LinkState::UsedUp => "Used up",
                    _ => "Revoked",
                })
                .into_any_element()
            };
            table = table.row(vec![
                item,
                mono(day(info.created_at)),
                mono(expires),
                mono(views(info.views, info.max_views)),
                action,
            ]);
        }
        table.into_any_element()
    }

    fn empty(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        v_flex()
            .gap(px(tokens::SPACE_5))
            .max_w(px(640.))
            .child(
                lcd(cx)
                    .flex()
                    .items_center()
                    .gap(px(tokens::SPACE_3))
                    .py(px(14.))
                    .px(px(18.))
                    .child(StatusLight::new(LightState::Off))
                    .child(
                        div()
                            .flex_1()
                            .font_family(tokens::FONT_MONO)
                            .font_weight(gpui_kit::FontWeight::MEDIUM)
                            .text_size(px(15.))
                            .text_color(color(p.lcd_ink))
                            .child("0 ACTIVE LINKS"),
                    )
                    .child(
                        div()
                            .text_token(tokens::LABEL)
                            .text_color(color(p.lcd_label))
                            .child("READY"),
                    ),
            )
            .child(
                div()
                    .text_token(tokens::BODY)
                    .text_color(color(p.ink))
                    .child(
                        "To share one item with someone who doesn\u{2019}t use Kandado, open it \
                         and choose Share.",
                    ),
            )
            .child(
                div().child(
                    Button::new("links-back", "Back to vault")
                        .icon(IconName::ArrowLeft)
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(LinksEvent::BackToVault))),
                ),
            )
            .into_any_element()
    }
}

impl Render for LinksPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let body: AnyElement = match &self.load {
            Load::Loading => div()
                .text_token(tokens::SMALL)
                .text_color(color(p.ink_2))
                .child("Loading your links\u{2026}")
                .into_any_element(),
            Load::Failed(message) => v_flex()
                .gap(px(tokens::SPACE_3))
                .items_start()
                .child(error_line(message.clone(), cx))
                .child(
                    Button::new("links-retry", "Try again")
                        .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
                )
                .into_any_element(),
            Load::Loaded(rows) if rows.is_empty() => self.empty(cx),
            Load::Loaded(rows) => {
                let rows = rows.clone();
                self.table(&rows, cx)
            }
        };
        v_flex()
            .id("secure-links")
            .size_full()
            .overflow_y_scrollbar()
            .gap(px(20.))
            .py(px(22.))
            .px(px(28.))
            .child(
                v_flex()
                    .gap(px(6.))
                    .child(heading("Secure links", cx))
                    .child(div().text_size(px(15.)).text_color(color(p.ink_2)).child(
                        "Links you made to share one item. Revoke a link to stop it \
                                 working at once.",
                    )),
            )
            .when_some(self.failure.clone(), |page, failure| {
                page.child(error_line(failure, cx))
            })
            .child(body)
    }
}
