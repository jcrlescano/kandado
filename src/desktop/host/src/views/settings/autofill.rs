//! D-45 Autofill and browser: the browser extension link. "Unlock the browser
//! extension with this app" registers Kandado with Chrome, Edge and Firefox and
//! listens for paired extensions (`kandado_app::browser`). A pairing request
//! shows here with its six-digit code, to accept when the extension shows the
//! same digits.
//!
//! Not here yet: auto-type into other apps and its two-factor option, passkeys
//! on this computer (phase 8), and "Get the extension" (store listings, phase 10).

use std::time::Duration;

use gpui_kit::{
    Context, IntoElement, ParentElement, Render, SharedString, Styled, Window,
    component::{h_flex, v_flex},
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::browser::{BrowserLink, LinkError};
use time::OffsetDateTime;

use super::{
    super::{Services, error_line, small},
    rows::{SettingRow, group_label},
};
use crate::{
    assets::IconName,
    components::{Banner, Button, ButtonSize, ButtonVariant, FieldGroup, Toggle, Tone},
};

pub struct AutofillPage {
    services: Services,
    seen: u64,
    failure: Option<SharedString>,
}

fn explain_link(error: &LinkError) -> SharedString {
    match error {
        LinkError::HostMissing => {
            "Kandado's browser link program is missing. Reinstall Kandado to add it.".into()
        }
        LinkError::Install(_) => "Windows did not let Kandado register with the browsers.".into(),
        LinkError::Pipe(_) => {
            "The browser link could not start. Is another copy of Kandado running?".into()
        }
        LinkError::Save(_) => "The paired browsers could not be saved. Try again.".into(),
        LinkError::App(error) => super::super::explain(error),
    }
}

/// "482 913"
fn spaced(code: &str) -> String {
    if code.len() == 6 {
        format!("{} {}", &code[..3], &code[3..])
    } else {
        code.to_owned()
    }
}

impl AutofillPage {
    pub fn new(services: Services, cx: &mut Context<Self>) -> Self {
        // A pairing request arrives from the pipe server: look for it twice a second.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let alive = this.update(cx, |this, cx| {
                    let version = this.services.browser.version();
                    if version != this.seen {
                        this.seen = version;
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
            seen: services.browser.version(),
            services,
            failure: None,
        }
    }

    fn link(&self) -> &std::sync::Arc<BrowserLink> {
        &self.services.browser
    }

    fn set_enabled(&mut self, on: bool, cx: &mut Context<Self>) {
        self.failure = self
            .link()
            .set_enabled(on)
            .err()
            .map(|error| explain_link(&error));
        cx.notify();
    }

    fn answer(&mut self, id: uuid::Uuid, accept: bool, cx: &mut Context<Self>) {
        self.failure = self
            .link()
            .answer(id, accept)
            .err()
            .map(|error| explain_link(&error));
        cx.notify();
    }

    fn remove(&mut self, id: uuid::Uuid, cx: &mut Context<Self>) {
        self.failure = self
            .link()
            .remove(id)
            .err()
            .map(|error| explain_link(&error));
        cx.notify();
    }
}

impl Render for AutofillPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let link = self.link().clone();
        let enabled = link.enabled();
        let pairings = link.pairings();
        let pending = link.pending();

        let mut page = v_flex().gap(px(8.));
        if let Some(request) = &pending {
            let id = request.id;
            page = page.child(
                Banner::new(
                    Tone::Brass,
                    format!(
                        "{} wants to unlock with this app. Accept only if it shows {}.",
                        request.browser,
                        spaced(&request.code)
                    ),
                )
                .icon(IconName::Link)
                .action(
                    h_flex()
                        .gap(px(8.))
                        .child(
                            Button::new("decline-pairing", "Decline")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.answer(id, false, cx)),
                                ),
                        )
                        .child(
                            Button::new("accept-pairing", "Accept")
                                .variant(ButtonVariant::Primary)
                                .size(ButtonSize::Sm)
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.answer(id, true, cx)),
                                ),
                        ),
                ),
            );
        } else if enabled && !pairings.is_empty() {
            let names: Vec<&str> = pairings
                .iter()
                .map(|pairing| pairing.browser.as_str())
                .collect();
            page = page.child(
                Banner::new(
                    Tone::Teal,
                    format!(
                        "Browser extension connected. {} unlock{} when the desktop app unlocks.",
                        names.join(" and "),
                        if names.len() == 1 { "s" } else { "" }
                    ),
                )
                .icon(IconName::Check),
            );
        } else if enabled {
            page = page.child(
                Banner::new(
                    Tone::Neutral,
                    "No browser paired yet. In the extension, open Settings \u{203a} Security and turn on Lock and unlock with the desktop app.",
                )
                .icon(IconName::Globe),
            );
        }

        page = page.child(
            FieldGroup::new().child(
                SettingRow::new(
                    "row-unlock-extension",
                    IconName::Globe,
                    "Unlock the browser extension with this app",
                )
                .description("Paired browsers unlock and lock with Kandado on this computer.")
                .trailing(
                    Toggle::new("toggle-unlock-extension")
                        .checked(enabled)
                        .on_change(cx.listener(|this, on: &bool, _, cx| this.set_enabled(*on, cx))),
                ),
            ),
        );

        if !pairings.is_empty() {
            let now = OffsetDateTime::now_utc().unix_timestamp();
            page =
                page.child(group_label("Paired browsers", cx))
                    .child(
                        FieldGroup::new().children(pairings.into_iter().map(|pairing| {
                            let id = pairing.id;
                            let days = (now - pairing.paired_at).max(0) / 86_400;
                            let when = match days {
                                0 => "Paired today".to_owned(),
                                1 => "Paired yesterday".to_owned(),
                                n => format!("Paired {n} days ago"),
                            };
                            SettingRow::new(
                                ("pairing", id.as_u128() as u64),
                                IconName::Globe,
                                pairing.browser.clone(),
                            )
                            .description(when)
                            .trailing(
                                Button::new(("unpair", id.as_u128() as u64), "Remove")
                                    .variant(ButtonVariant::Ghost)
                                    .size(ButtonSize::Sm)
                                    .on_click(
                                        cx.listener(move |this, _, _, cx| this.remove(id, cx)),
                                    ),
                            )
                        })),
                    );
        }

        page.when_some(self.failure.clone(), |page, failure| {
            page.child(error_line(failure, cx))
        })
        .when(!enabled && pending.is_none(), |page| {
            page.child(small(
                "The extension works on its own too: it signs in and unlocks with your master password.",
                cx,
            ))
        })
    }
}
