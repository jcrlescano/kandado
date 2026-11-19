//! D-32 Create secure link dialog. States: options (with creating and a
//! failure line), ready.
//!
//! The link holds its key. It lives in this dialog only until the dialog
//! closes, and goes to the clipboard only as a secret (cleared on the timer).

use gpui_kit::{
    Context, EventEmitter, FontWeight, IntoElement, ParentElement, Render, SharedString, Styled,
    Window, div, prelude::FluentBuilder as _, px,
};
use kandado_app::links::{LinkOptions, NewLink};
use kandado_protocol::ProblemCode;
use uuid::Uuid;

use super::{super::Services, day_and_time};
use crate::{
    assets::IconName,
    components::{
        Button, Dialog, LightState, Segmented, StatusLight, Toggle, lcd, section_label, well,
        window_layer,
    },
    theme::{KandadoTheme, TextStyles as _, color, tokens},
    views::{error_line, explain, items::detail::copied_message},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkDialogEvent {
    Closed,
    /// "See all links": open D-33.
    SeeAll,
    Toast(SharedString),
}

pub struct LinkDialog {
    services: Services,
    item: Uuid,
    title: String,
    offers_totp: bool,
    options: LinkOptions,
    creating: bool,
    failure: Option<SharedString>,
    made: Option<NewLink>,
}

impl EventEmitter<LinkDialogEvent> for LinkDialog {}

const EXPIRY: [&str; 4] = ["1 HOUR", "1 DAY", "7 DAYS", "30 DAYS"];
const VIEWS: [&str; 4] = ["ONCE", "5 TIMES", "10 TIMES", "NO LIMIT"];

impl LinkDialog {
    pub fn new(services: Services, item: Uuid, _: &mut Window, _: &mut Context<Self>) -> Self {
        let title = services
            .app
            .entry(item)
            .map(|entry| entry.title)
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "this item".to_owned());
        let offers_totp = services.app.has_totp(item);
        Self {
            services,
            item,
            title,
            offers_totp,
            options: LinkOptions::default(),
            creating: false,
            failure: None,
            made: None,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.made.is_some()
    }

    /// The failure line, if the last try failed.
    pub fn failure(&self) -> Option<&SharedString> {
        self.failure.as_ref()
    }

    /// "See all links": forget the link and open D-33.
    pub fn see_all(&mut self, cx: &mut Context<Self>) {
        self.made = None;
        cx.emit(LinkDialogEvent::SeeAll);
    }

    pub fn choose_expiry(&mut self, index: usize, cx: &mut Context<Self>) {
        self.options.expiry = index.min(EXPIRY.len() - 1);
        cx.notify();
    }

    pub fn choose_views(&mut self, index: usize, cx: &mut Context<Self>) {
        self.options.views = index.min(VIEWS.len() - 1);
        cx.notify();
    }

    pub fn set_include_totp(&mut self, on: bool, cx: &mut Context<Self>) {
        self.options.include_totp = on && self.offers_totp;
        cx.notify();
    }

    pub fn create(&mut self, cx: &mut Context<Self>) {
        if self.creating || self.made.is_some() {
            return;
        }
        self.creating = true;
        self.failure = None;
        cx.notify();
        let services = self.services.clone();
        let (item, options) = (self.item, self.options);
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let result = services
                .tasks
                .run(async move { app.create_link(item, options).await })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.creating = false;
                match result {
                    Ok(made) => this.made = Some(made),
                    Err(error) => this.failure = Some(failure_text(&error)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn copy(&mut self, cx: &mut Context<Self>) {
        let Some(made) = &self.made else {
            return;
        };
        match self.services.app.copy_link(made) {
            Ok(copied) => cx.emit(LinkDialogEvent::Toast(copied_message(&copied))),
            Err(error) => {
                self.failure = Some(explain(&error));
                cx.notify();
            }
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.made = None;
        cx.emit(LinkDialogEvent::Closed);
    }

    fn options_body(&self, dialog: Dialog, cx: &mut Context<Self>) -> Dialog {
        let p = cx.palette();
        let view = cx.entity();
        let expiry = {
            let view = view.clone();
            Segmented::new("link-expiry", EXPIRY, self.options.expiry).on_select(
                move |index, _, cx| view.update(cx, |this, cx| this.choose_expiry(index, cx)),
            )
        };
        let views = Segmented::new("link-views", VIEWS, self.options.views).on_select(
            move |index, _, cx| view.update(cx, |this, cx| this.choose_views(index, cx)),
        );
        dialog
            .child(section_label("Expires after", cx))
            .child(expiry)
            .child(section_label("Can be opened", cx))
            .child(views)
            .when(self.offers_totp, |dialog| {
                dialog.child(
                    well(cx).px(px(14.)).py(px(4.)).child(
                        Toggle::new("link-totp")
                            .label("Include the two-factor code")
                            .checked(self.options.include_totp)
                            .on_change(
                                cx.listener(|this, on: &bool, _, cx| {
                                    this.set_include_totp(*on, cx)
                                }),
                            ),
                    ),
                )
            })
            .child(
                div()
                    .text_token(tokens::SMALL)
                    .text_color(color(p.ink_2))
                    .child(
                        "The key is in the link itself. Kandado stores only the locked copy \
                         and can\u{2019}t open it.",
                    ),
            )
            .when_some(self.failure.clone(), |dialog, failure| {
                dialog.child(error_line(failure, cx))
            })
            .action(
                Button::new("link-cancel", "Cancel")
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            )
            .action(
                Button::new(
                    "link-create",
                    if self.creating {
                        "Creating\u{2026}"
                    } else {
                        "Create link"
                    },
                )
                .primary()
                .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
            )
    }

    fn ready_body(&self, made: &NewLink, dialog: Dialog, cx: &mut Context<Self>) -> Dialog {
        let p = cx.palette();
        let views = match made.max_views {
            Some(1) => "1 VIEW".to_owned(),
            Some(count) => format!("{count} VIEWS"),
            None => "NO VIEW LIMIT".to_owned(),
        };
        let advice = match made.max_views {
            Some(1) => "Send it in a different app from the item\u{2019}s site, and tell the \
                        person it works once."
                .to_owned(),
            Some(count) => format!(
                "Send it in a different app from the item\u{2019}s site, and tell the person \
                 it works {count} times."
            ),
            None => "Send it in a different app from the item\u{2019}s site, and tell the \
                     person when it stops working."
                .to_owned(),
        };
        let label = |text: String| {
            div()
                .text_token(tokens::LABEL)
                .text_color(color(p.lcd_label))
                .child(SharedString::from(text))
        };
        dialog
            .child(
                lcd(cx)
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .py(px(tokens::SPACE_5))
                    .px(px(18.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(tokens::SPACE_2))
                            .child(StatusLight::new(LightState::Ok))
                            .child(label("LINK READY".to_owned()).flex_1())
                            .child(label(format!(
                                "EXPIRES {} \u{b7} {views}",
                                day_and_time(made.expires_at)
                            ))),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .font_family(tokens::FONT_MONO)
                            .font_weight(FontWeight::MEDIUM)
                            .text_size(px(14.))
                            .line_height(px(20.))
                            .text_color(color(p.lcd_ink))
                            .child(SharedString::from(made.url.as_str().to_owned())),
                    ),
            )
            .child(
                div()
                    .text_token(tokens::BODY)
                    .text_color(color(p.ink))
                    .child(SharedString::from(advice)),
            )
            .when_some(self.failure.clone(), |dialog, failure| {
                dialog.child(error_line(failure, cx))
            })
            .action(
                Button::new("link-done", "Done")
                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
            )
            .action(
                Button::new("link-see-all", "See all links")
                    .on_click(cx.listener(|this, _, _, cx| this.see_all(cx))),
            )
            .action(
                Button::new("link-copy", "Copy link")
                    .primary()
                    .icon(IconName::Copy)
                    .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
            )
    }
}

/// Plain words for why a link was not made.
fn failure_text(error: &kandado_app::AppError) -> SharedString {
    use kandado_app::{ApiError, AppError};
    match error {
        AppError::Model(kandado_model::Error::NotShareable) => {
            "A passkey can\u{2019}t be shared by link.".into()
        }
        AppError::Api(ApiError::Problem(problem)) if problem.code == ProblemCode::RateLimited => {
            "You have too many working links. Revoke some in Secure links first.".into()
        }
        AppError::Api(ApiError::Problem(problem))
            if problem.code == ProblemCode::PayloadTooLarge =>
        {
            "This item is too large to share by link.".into()
        }
        AppError::Api(ApiError::Problem(problem)) if problem.code == ProblemCode::NotFound => {
            "The server doesn\u{2019}t have this item yet. Wait for it to sync and try again."
                .into()
        }
        other => explain(other),
    }
}

impl Render for LinkDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dialog = Dialog::new(format!("Secure link for {}", self.title)).wide();
        let dialog = match self.made.take() {
            Some(made) => {
                let dialog = self.ready_body(&made, dialog, cx);
                self.made = Some(made);
                dialog
            }
            None => self.options_body(dialog, cx),
        };
        window_layer(dialog, window)
    }
}
