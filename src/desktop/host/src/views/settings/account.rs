//! D-50 Account: the Google address (read only), the plan, changing the master
//! password, where two-factor is managed, and deleting the account.
//!
//! Changing the master password and deleting the account both ask for a fresh
//! Google sign-in, so each dialog has a "Confirm it's you" step.

use gpui_kit::AppContext as _;
use gpui_kit::{
    Context, Entity, EventEmitter, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
    component::{
        input::{InputEvent, InputState},
        v_flex,
    },
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::{
    Outcome, Phase, platform,
    service::{MIN_PASSWORD_LEN, MIN_PASSWORD_SCORE},
};
use kandado_model::strength;

use super::{
    super::{Nav, Services, error_line, explain, small, with_overlay},
    confirm,
    rows::{SettingRow, group_label},
};
use crate::{
    assets::IconName,
    components::{
        Button, ButtonVariant, Dialog, FieldGroup, IconButton, StrengthMeter, TextField, Tone,
    },
};

/// Where Google lets people manage their own two-step verification.
const GOOGLE_TWO_STEP: &str = "https://myaccount.google.com/signinoptions/two-step-verification";

/// What a "Confirm it's you" step is for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Waiting {
    ChangePassword,
    DeleteAccount,
}

enum Open {
    ChangePassword,
    DeleteAccount,
    Confirm { waiting_for: Waiting, busy: bool },
}

pub struct AccountPage {
    services: Services,
    email: String,
    plan: SharedString,
    current: Entity<InputState>,
    new: Entity<InputState>,
    confirm: Entity<InputState>,
    delete_password: Entity<InputState>,
    delete_word: Entity<InputState>,
    revealed: bool,
    open: Option<Open>,
    submitted: bool,
    busy: bool,
    failure: Option<SharedString>,
    notice: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<Nav> for AccountPage {}

impl AccountPage {
    pub fn new(services: Services, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let masked = |window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).masked(true))
        };
        let current = masked(window, cx);
        let new = masked(window, cx);
        let confirm = masked(window, cx);
        let delete_password = masked(window, cx);
        let delete_word = cx.new(|cx| InputState::new(window, cx).placeholder("DELETE"));
        let subscriptions = [&current, &new, &confirm, &delete_password, &delete_word]
            .into_iter()
            .map(|input| cx.subscribe(input, |_, _, _: &InputEvent, cx| cx.notify()))
            .collect();
        let email = services.app.email().unwrap_or_default();
        let mut page = Self {
            services,
            email,
            plan: "Free".into(),
            current,
            new,
            confirm,
            delete_password,
            delete_word,
            revealed: false,
            open: None,
            submitted: false,
            busy: false,
            failure: None,
            notice: None,
            _subscriptions: subscriptions,
        };
        page.load_plan(cx);
        page
    }

    fn load_plan(&mut self, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let result = services.tasks.run(async move { app.profile().await }).await;
            if let Ok(me) = result {
                let _ = this.update(cx, |this, cx| {
                    let mut plan = me.plan;
                    if let Some(first) = plan.get_mut(0..1) {
                        first.make_ascii_uppercase();
                    }
                    this.plan = plan.into();
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn inputs(&self) -> [&Entity<InputState>; 5] {
        [
            &self.current,
            &self.new,
            &self.confirm,
            &self.delete_password,
            &self.delete_word,
        ]
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = None;
        self.submitted = false;
        self.busy = false;
        self.failure = None;
        for input in self.inputs() {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        cx.notify();
    }

    fn password_problem(&self, password: &str) -> Option<&'static str> {
        if password.chars().count() < MIN_PASSWORD_LEN {
            return Some("Use at least 8 characters.");
        }
        if strength::estimate(password, &[self.email.as_str()]).score < MIN_PASSWORD_SCORE {
            return Some("Too easy to guess. Try a short sentence of unrelated words.");
        }
        None
    }

    fn toggle_reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.revealed = !self.revealed;
        let masked = !self.revealed;
        for input in [
            &self.current,
            &self.new,
            &self.confirm,
            &self.delete_password,
        ] {
            input.update(cx, |input, cx| input.set_masked(masked, window, cx));
        }
        cx.notify();
    }

    // ---- change master password ----------------------------------------

    fn submit_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.submitted = true;
        self.failure = None;
        let current = self.current.read(cx).value().to_string();
        let new = self.new.read(cx).value().to_string();
        let confirm = self.confirm.read(cx).value().to_string();
        if current.is_empty() || self.password_problem(&new).is_some() || new != confirm {
            cx.notify();
            return;
        }
        self.busy = true;
        cx.notify();
        let services = self.services.clone();
        cx.spawn_in(window, async move |this, cx| {
            let app = services.app.clone();
            let result = services
                .tasks
                .run(async move { app.change_master_password(current, new).await })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(Outcome::Done) => this.password_changed(window, cx),
                    Ok(Outcome::NeedsGoogle) => {
                        this.open = Some(Open::Confirm {
                            waiting_for: Waiting::ChangePassword,
                            busy: false,
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

    fn password_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
        self.notice = Some("Your master password was changed.".into());
        cx.notify();
    }

    // ---- delete account -------------------------------------------------

    fn submit_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.submitted = true;
        self.failure = None;
        let password = self.delete_password.read(cx).value().to_string();
        let word = self.delete_word.read(cx).value().to_string();
        if password.is_empty() || word.trim() != "DELETE" {
            cx.notify();
            return;
        }
        self.busy = true;
        cx.notify();
        let services = self.services.clone();
        cx.spawn_in(window, async move |this, cx| {
            let app = services.app.clone();
            let result = services
                .tasks
                .run(async move {
                    app.verify_password(password).await?;
                    app.delete_account().await
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.busy = false;
                match result {
                    Ok(Outcome::Done) => cx.emit(Nav::Phase(Phase::SignedOut)),
                    Ok(Outcome::NeedsGoogle) => {
                        this.open = Some(Open::Confirm {
                            waiting_for: Waiting::DeleteAccount,
                            busy: false,
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

    // ---- confirm with Google ------------------------------------------------

    fn confirm_with_google(
        &mut self,
        waiting_for: Waiting,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(Open::Confirm { busy, .. }) = self.open.as_mut() {
            if *busy {
                return;
            }
            *busy = true;
        }
        self.failure = None;
        cx.notify();
        let services = self.services.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = match confirm::google(&services).await {
                Ok(()) => {
                    let app = services.app.clone();
                    services
                        .tasks
                        .run(async move {
                            match waiting_for {
                                Waiting::ChangePassword => app.resume().await,
                                Waiting::DeleteAccount => app.delete_account().await,
                            }
                        })
                        .await
                }
                Err(error) => Err(error),
            };
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Outcome::Done) => match waiting_for {
                    Waiting::ChangePassword => this.password_changed(window, cx),
                    Waiting::DeleteAccount => cx.emit(Nav::Phase(Phase::SignedOut)),
                },
                Ok(Outcome::NeedsGoogle) => {
                    this.failure = Some("Google sign-in did not go through. Try again.".into());
                    if let Some(Open::Confirm { busy, .. }) = this.open.as_mut() {
                        *busy = false;
                    }
                    cx.notify();
                }
                Err(error) => {
                    this.failure = Some(explain(&error));
                    if let Some(Open::Confirm { busy, .. }) = this.open.as_mut() {
                        *busy = false;
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn reveal_button(&self, cx: &mut Context<Self>) -> IconButton {
        IconButton::new(
            "reveal-account-passwords",
            if self.revealed {
                IconName::EyeOff
            } else {
                IconName::Eye
            },
            if self.revealed {
                "Hide passwords"
            } else {
                "Show passwords"
            },
        )
        .size(crate::components::ButtonSize::Sm)
        .on_click(cx.listener(|this, _, window, cx| this.toggle_reveal(window, cx)))
    }

    fn change_dialog(&self, cx: &mut Context<Self>) -> Dialog {
        let new = self.new.read(cx).value().to_string();
        let confirm = self.confirm.read(cx).value().to_string();
        let current = self.current.read(cx).value().to_string();
        let estimate = strength::estimate(&new, &[self.email.as_str()]);
        let new_error = self
            .submitted
            .then(|| self.password_problem(&new))
            .flatten();
        let mismatch = self.submitted && new != confirm;
        Dialog::new("Change master password")
            .icon(IconName::Lock, Tone::Brass)
            .body("Your vault is not re-encrypted. Only the key that opens it is wrapped again. You will confirm with Google next.")
            .child(
                TextField::new(&self.current)
                    .label("Current master password")
                    .trailing(self.reveal_button(cx))
                    .when(self.submitted && current.is_empty(), |field| {
                        field.error("Enter your current master password.")
                    }),
            )
            .child(TextField::new(&self.new).label("New master password").when_some(
                new_error,
                |field, message| field.error(message),
            ))
            .when(!new.is_empty(), |dialog| dialog.child(StrengthMeter::new(estimate.score)))
            .child(
                TextField::new(&self.confirm)
                    .label("Confirm new master password")
                    .when(mismatch, |field| field.error("The two passwords don\u{2019}t match.")),
            )
            .when_some(self.failure.clone(), |dialog, message| {
                dialog.child(error_line(message, cx))
            })
            .action(
                Button::new("change-cancel", "Cancel")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            )
            .action(
                Button::new(
                    "change-submit",
                    if self.busy { "Changing…" } else { "Change password" },
                )
                .primary()
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| this.submit_change(window, cx))),
            )
    }

    fn delete_dialog(&self, cx: &mut Context<Self>) -> Dialog {
        let password = self.delete_password.read(cx).value().to_string();
        let word = self.delete_word.read(cx).value().to_string();
        Dialog::new("Delete your account?")
            .icon(IconName::Trash, Tone::Danger)
            .body("This deletes your account and every vault you own, on every device. It can\u{2019}t be undone.")
            .child(
                TextField::new(&self.delete_password)
                    .label("Master password")
                    .trailing(self.reveal_button(cx))
                    .when(self.submitted && password.is_empty(), |field| {
                        field.error("Enter your master password.")
                    }),
            )
            .child(
                TextField::new(&self.delete_word)
                    .label("Type DELETE to confirm")
                    .when(self.submitted && word.trim() != "DELETE", |field| {
                        field.error("Type DELETE in capital letters.")
                    }),
            )
            .when_some(self.failure.clone(), |dialog, message| {
                dialog.child(error_line(message, cx))
            })
            .action(
                Button::new("delete-cancel", "Cancel")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            )
            .action(
                Button::new(
                    "delete-submit",
                    if self.busy { "Deleting…" } else { "Delete account" },
                )
                .variant(ButtonVariant::Danger)
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| this.submit_delete(window, cx))),
            )
    }

    fn confirm_dialog(&self, waiting_for: Waiting, busy: bool, cx: &mut Context<Self>) -> Dialog {
        Dialog::new("Confirm it\u{2019}s you")
            .icon(IconName::Shield, Tone::Brass)
            .body(match waiting_for {
                Waiting::ChangePassword => {
                    "To change your master password, sign in with Google once more."
                }
                Waiting::DeleteAccount => "To delete your account, sign in with Google once more.",
            })
            .when(busy, |dialog| {
                dialog.child(small(
                    "Finish signing in in your browser, then come back here.",
                    cx,
                ))
            })
            .when_some(self.failure.clone(), |dialog, message| {
                dialog.child(error_line(message, cx))
            })
            .action(
                Button::new("confirm-account-cancel", "Cancel")
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            )
            .action(
                Button::new(
                    "confirm-account-google",
                    if busy {
                        "Waiting for Google…"
                    } else {
                        "Continue with Google"
                    },
                )
                .primary()
                .disabled(busy)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.confirm_with_google(waiting_for, window, cx)
                })),
            )
    }
}

impl Render for AccountPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dialog = match &self.open {
            None => None,
            Some(Open::ChangePassword) => Some(self.change_dialog(cx).into_any_element()),
            Some(Open::DeleteAccount) => Some(self.delete_dialog(cx).into_any_element()),
            Some(Open::Confirm { waiting_for, busy }) => Some(
                self.confirm_dialog(*waiting_for, *busy, cx)
                    .into_any_element(),
            ),
        };

        let page = v_flex()
            .gap(px(8.))
            .child(group_label("Account", cx))
            .child(
                FieldGroup::new()
                    .child(
                        SettingRow::new("row-email", IconName::Mail, "Email")
                            .description("From your Google account.")
                            .value(self.email.clone()),
                    )
                    .child(
                        SettingRow::new("row-plan", IconName::Star, "Plan")
                            .value(self.plan.clone()),
                    )
                    .child(
                        SettingRow::new(
                            "row-change-password",
                            IconName::Lock,
                            "Change master password",
                        )
                        .opens(cx.listener(|this, _, _, cx| {
                            this.open = Some(Open::ChangePassword);
                            cx.notify();
                        })),
                    )
                    .child(
                        SettingRow::new(
                            "row-two-factor",
                            IconName::Shield,
                            "Two-factor for your account",
                        )
                        .description("Managed by Google.")
                        .opens(|_, _, _| {
                            let _ = platform::open_url(GOOGLE_TWO_STEP);
                        }),
                    ),
            )
            .when_some(self.notice.clone(), |page, notice| {
                page.child(small(notice, cx))
            })
            .child(group_label("Danger zone", cx))
            .child(
                FieldGroup::new().child(
                    SettingRow::new("row-delete", IconName::Trash, "Delete account")
                        .description("Deletes every vault you own. This can\u{2019}t be undone.")
                        .danger(cx.listener(|this, _, _, cx| {
                            this.open = Some(Open::DeleteAccount);
                            cx.notify();
                        })),
                ),
            );
        with_overlay(page.into_any_element(), dialog)
    }
}
