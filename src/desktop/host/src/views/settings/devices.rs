//! D-49 Devices: every signed-in app and extension. Sign one out, or all the
//! others (which asks for a fresh Google sign-in).
//! States: loading, loaded, could not load (with Retry), signing one out,
//! asking to confirm with Google.

use gpui_kit::{
    Context, EventEmitter, IntoElement, ParentElement, Render, SharedString, Styled, Window,
    component::v_flex, div, prelude::FluentBuilder as _, px,
};
use kandado_app::Outcome;
use kandado_protocol::Device;
use time::OffsetDateTime;
use uuid::Uuid;

use super::{
    super::{Nav, Services, error_line, explain, small, with_overlay},
    confirm,
    rows::SettingRow,
};
use crate::{
    assets::IconName,
    components::{Badge, Button, ButtonSize, ButtonVariant, Dialog, FieldGroup, Tone},
    theme::{KandadoTheme, color},
};

enum Load {
    Loading,
    Loaded(Vec<Device>),
    Failed(SharedString),
}

pub struct DevicesPage {
    services: Services,
    load: Load,
    /// The device being signed out, or "all" (the nil id).
    working: Option<Uuid>,
    failure: Option<SharedString>,
    confirming: Option<Confirm>,
}

/// The "confirm with Google" dialog for signing out the other devices.
struct Confirm {
    waiting: bool,
    failure: Option<SharedString>,
}

impl EventEmitter<Nav> for DevicesPage {}

const ALL: Uuid = Uuid::nil();

impl DevicesPage {
    pub fn new(services: Services, cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            services,
            load: Load::Loading,
            working: None,
            failure: None,
            confirming: None,
        };
        page.reload(cx);
        page
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.load = Load::Loading;
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let result = services.tasks.run(async move { app.devices().await }).await;
            let _ = this.update(cx, |this, cx| {
                this.load = match result {
                    Ok(list) => Load::Loaded(list.devices),
                    Err(error) => Load::Failed(explain(&error)),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn sign_out_one(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.working.is_some() {
            return;
        }
        self.working = Some(id);
        self.failure = None;
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let result = services
                .tasks
                .run(async move { app.revoke_device(id).await })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.working = None;
                match result {
                    Ok(()) => this.reload(cx),
                    Err(error) => {
                        this.failure = Some(explain(&error));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn sign_out_others(&mut self, cx: &mut Context<Self>) {
        if self.working.is_some() {
            return;
        }
        self.working = Some(ALL);
        self.failure = None;
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let result = services
                .tasks
                .run(async move { app.revoke_other_devices().await })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.working = None;
                match result {
                    Ok(Outcome::Done) => this.reload(cx),
                    Ok(Outcome::NeedsGoogle) => {
                        this.confirming = Some(Confirm {
                            waiting: false,
                            failure: None,
                        });
                        cx.notify();
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

    /// The person agreed to sign in with Google again; then try once more.
    fn confirm_with_google(&mut self, cx: &mut Context<Self>) {
        let Some(confirming) = self.confirming.as_mut() else {
            return;
        };
        if confirming.waiting {
            return;
        }
        confirming.waiting = true;
        confirming.failure = None;
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let signed_in = confirm::google(&services).await;
            let result = match signed_in {
                Ok(()) => {
                    let app = services.app.clone();
                    services
                        .tasks
                        .run(async move { app.revoke_other_devices().await })
                        .await
                }
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |this, cx| match result {
                Ok(_) => {
                    this.confirming = None;
                    this.reload(cx);
                }
                Err(error) => {
                    if let Some(confirming) = this.confirming.as_mut() {
                        confirming.waiting = false;
                        confirming.failure = Some(explain(&error));
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

/// "now", "5 minutes ago", "2 hours ago", "3 days ago".
pub fn ago(then: OffsetDateTime, now: OffsetDateTime) -> String {
    let seconds = (now - then).whole_seconds().max(0);
    let (count, unit) = match seconds {
        0..=89 => return "now".to_owned(),
        90..=3_599 => ((seconds + 30) / 60, "minute"),
        3_600..=86_399 => ((seconds + 1_800) / 3_600, "hour"),
        _ => ((seconds + 43_200) / 86_400, "day"),
    };
    format!("{count} {unit}{} ago", if count == 1 { "" } else { "s" })
}

fn icon_for(platform: &str) -> IconName {
    let lower = platform.to_lowercase();
    if ["chrome", "edge", "firefox", "safari"]
        .iter()
        .any(|browser| lower.contains(browser))
    {
        IconName::Globe
    } else {
        IconName::Grid
    }
}

impl Render for DevicesPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = OffsetDateTime::now_utc();
        let body = match &self.load {
            Load::Loading => v_flex().child(small("Loading your devices…", cx)),
            Load::Failed(message) => v_flex()
                .gap(px(12.))
                .child(error_line(message.clone(), cx))
                .child(
                    div().child(
                        Button::new("retry-devices", "Try again")
                            .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
                    ),
                ),
            Load::Loaded(devices) => {
                let others = devices.iter().any(|device| !device.current);
                v_flex()
                    .gap(px(12.))
                    .child(FieldGroup::new().children(devices.iter().map(|device| {
                        let id = device.id;
                        let signing_out = self.working == Some(id);
                        let row = SettingRow::new(
                            ("device", id.as_u128() as u64),
                            icon_for(&device.platform),
                            device.name.clone(),
                        )
                        .description(format!(
                            "{} · {}",
                            device.platform,
                            ago(device.last_seen_at, now)
                        ));
                        if device.current {
                            row.trailing(Badge::new("This device").tone(Tone::Teal).no_icon())
                        } else {
                            row.trailing(
                                Button::new(
                                    ("sign-out-device", id.as_u128() as u64),
                                    if signing_out {
                                        "Signing out…"
                                    } else {
                                        "Sign out"
                                    },
                                )
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .disabled(self.working.is_some())
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.sign_out_one(id, cx)),
                                ),
                            )
                        }
                    })))
                    .when(others, |body| {
                        body.child(
                            Button::new(
                                "sign-out-others",
                                if self.working == Some(ALL) {
                                    "Signing out…"
                                } else {
                                    "Sign out all other devices"
                                },
                            )
                            .variant(ButtonVariant::DangerQuiet)
                            .icon(IconName::Logout)
                            .block()
                            .disabled(self.working.is_some())
                            .on_click(cx.listener(|this, _, _, cx| this.sign_out_others(cx))),
                        )
                    })
            }
        };

        let dialog = self.confirming.as_ref().map(|confirming| {
            Dialog::new("Confirm it\u{2019}s you")
                .icon(IconName::Shield, Tone::Brass)
                .body("To sign out your other devices, sign in with Google once more.")
                .when_some(confirming.failure.clone(), |dialog, message| {
                    dialog.child(error_line(message, cx))
                })
                .action(
                    Button::new("confirm-cancel", "Cancel")
                        .disabled(confirming.waiting)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirming = None;
                            cx.notify();
                        })),
                )
                .action(
                    Button::new(
                        "confirm-google",
                        if confirming.waiting {
                            "Waiting for Google…"
                        } else {
                            "Continue with Google"
                        },
                    )
                    .primary()
                    .disabled(confirming.waiting)
                    .on_click(cx.listener(|this, _, _, cx| this.confirm_with_google(cx))),
                )
                .into_any_element()
        });

        with_overlay(
            v_flex()
                .gap(px(12.))
                .child(body)
                .when_some(self.failure.clone(), |page, message| {
                    page.child(error_line(message, cx))
                })
                .into_any_element(),
            dialog,
        )
        .text_color(color(cx.palette().ink))
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;

    #[test]
    fn times_read_like_the_design() {
        let now = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        let before = |d: Duration| ago(now - d, now);
        assert_eq!(before(Duration::seconds(5)), "now");
        assert_eq!(before(Duration::seconds(89)), "now");
        assert_eq!(before(Duration::minutes(2)), "2 minutes ago");
        assert_eq!(before(Duration::minutes(59)), "59 minutes ago");
        assert_eq!(before(Duration::hours(1)), "1 hour ago");
        assert_eq!(before(Duration::hours(2)), "2 hours ago");
        assert_eq!(before(Duration::days(1)), "1 day ago");
        assert_eq!(before(Duration::days(12)), "12 days ago");
        // A clock a little ahead of the server does not print a negative time.
        assert_eq!(ago(now + Duration::minutes(5), now), "now");
    }

    #[test]
    fn browsers_get_the_globe_and_apps_get_the_grid() {
        assert_eq!(icon_for("Chrome"), IconName::Globe);
        assert_eq!(icon_for("Windows"), IconName::Grid);
        assert_eq!(icon_for("Firefox on Windows 11"), IconName::Globe);
    }
}
