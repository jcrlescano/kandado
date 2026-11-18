//! Forgot password. The 12-word recovery phrase unwraps the vault key, and a
//! new master password wraps it again. Changing the password on the server
//! needs a fresh Google sign-in, so this screen may ask for one before it
//! finishes. There is no server-side reset: without the phrase there is no way
//! back in, and the screen says so.
//!
//! This screen is not in the design system (its "Forgot password?" link is
//! D-04's); it is built from the same parts as D-02. See `decisions.md`.

use gpui_kit::AppContext as _;
use gpui_kit::{
    Context, Entity, EventEmitter, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
    component::{
        h_flex,
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

use super::super::{Nav, Services, auth_shell, error_line, explain, heading, paragraph, small};
use crate::{
    assets::IconName,
    components::{Button, ButtonSize, ButtonVariant, IconButton, Link, StrengthMeter, TextField},
};

enum Step {
    /// Typing the phrase and the new password.
    Form,
    /// The server wants a fresh Google sign-in before it accepts the change.
    ConfirmWithGoogle { waiting: bool },
}

pub struct RecoverView {
    services: Services,
    email: String,
    phrase: Entity<InputState>,
    password: Entity<InputState>,
    confirm: Entity<InputState>,
    revealed: bool,
    busy: bool,
    submitted: bool,
    step: Step,
    failure: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<Nav> for RecoverView {}

impl RecoverView {
    pub fn new(
        services: Services,
        email: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let phrase = cx
            .new(|cx| InputState::new(window, cx).placeholder("twelve words separated by spaces"));
        let password = cx.new(|cx| InputState::new(window, cx).masked(true));
        let confirm = cx.new(|cx| InputState::new(window, cx).masked(true));
        let subscriptions = [&phrase, &password, &confirm]
            .into_iter()
            .map(|input| cx.subscribe(input, |_, _, _: &InputEvent, cx| cx.notify()))
            .collect();
        Self {
            services,
            email,
            phrase,
            password,
            confirm,
            revealed: false,
            busy: false,
            submitted: false,
            step: Step::Form,
            failure: None,
            _subscriptions: subscriptions,
        }
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
        for input in [&self.password, &self.confirm] {
            input.update(cx, |input, cx| input.set_masked(masked, window, cx));
        }
        cx.notify();
    }

    fn clear_inputs(&self, window: &mut Window, cx: &mut Context<Self>) {
        for input in [&self.phrase, &self.password, &self.confirm] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.submitted = true;
        self.failure = None;
        let phrase = self.phrase.read(cx).value().to_string();
        let password = self.password.read(cx).value().to_string();
        let confirm = self.confirm.read(cx).value().to_string();
        if phrase.split_whitespace().count() != 12
            || self.password_problem(&password).is_some()
            || password != confirm
        {
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
                .run(async move { app.recover(phrase, password).await })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                this.after(result, window, cx);
            });
        })
        .detach();
    }

    /// Handles the answer to recover or resume.
    fn after(
        &mut self,
        result: Result<Outcome, kandado_app::AppError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(Outcome::Done) => {
                self.clear_inputs(window, cx);
                cx.emit(Nav::Phase(Phase::Unlocked {
                    email: self.email.clone(),
                }));
            }
            Ok(Outcome::NeedsGoogle) => {
                self.step = Step::ConfirmWithGoogle { waiting: false };
                cx.notify();
            }
            Err(error) => {
                self.failure = Some(explain(&error));
                cx.notify();
            }
        }
    }

    fn confirm_with_google(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.failure = None;
        self.step = Step::ConfirmWithGoogle { waiting: true };
        cx.notify();
        let services = self.services.clone();
        cx.spawn_in(window, async move |this, cx| {
            let app = services.app.clone();
            let result = services
                .tasks
                .run(async move {
                    let pending = app.begin_google().await?;
                    platform::open_url(pending.url.as_str()).map_err(|_| {
                        kandado_app::AppError::Auth(kandado_app::auth::AuthError::Failed)
                    })?;
                    app.finish_google(pending).await?;
                    app.resume().await
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                if result.is_err() {
                    this.step = Step::ConfirmWithGoogle { waiting: false };
                }
                this.after(result, window, cx);
            });
        })
        .detach();
    }
}

impl Render for RecoverView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let back = Link::new("back-to-unlock", "Back to unlock")
            .on_click(cx.listener(|_, _, _, cx| cx.emit(Nav::Unlock)));

        if let Step::ConfirmWithGoogle { waiting } = self.step {
            let form = v_flex()
                .gap(px(14.))
                .child(heading("Confirm it\u{2019}s you", cx))
                .child(paragraph(
                    "To set a new master password, sign in with Google once more.",
                    cx,
                ))
                .when(waiting, |form| {
                    form.child(small(
                        "Finish signing in in your browser, then come back here.",
                        cx,
                    ))
                })
                .when_some(self.failure.clone(), |form, message| {
                    form.child(error_line(message, cx))
                })
                .child(
                    Button::new(
                        "confirm-with-google",
                        if waiting {
                            "Waiting for Google…"
                        } else {
                            "Continue with Google"
                        },
                    )
                    .primary()
                    .size(ButtonSize::Lg)
                    .icon(IconName::User)
                    .block()
                    .disabled(self.busy)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.confirm_with_google(window, cx)),
                    ),
                )
                .child(h_flex().child(back));
            return auth_shell(form, cx);
        }

        let phrase_text = self.phrase.read(cx).value().to_string();
        let password = self.password.read(cx).value().to_string();
        let confirm = self.confirm.read(cx).value().to_string();
        let estimate = strength::estimate(&password, &[self.email.as_str()]);
        let phrase_error = (self.submitted && phrase_text.split_whitespace().count() != 12)
            .then_some("Enter all 12 words, separated by spaces.");
        let password_error = self
            .submitted
            .then(|| self.password_problem(&password))
            .flatten();
        let mismatch = self.submitted && password != confirm;

        let reveal = IconButton::new(
            "reveal-new-password",
            if self.revealed {
                IconName::EyeOff
            } else {
                IconName::Eye
            },
            if self.revealed {
                "Hide password"
            } else {
                "Show password"
            },
        )
        .size(ButtonSize::Sm)
        .on_click(cx.listener(|this, _, window, cx| this.toggle_reveal(window, cx)));

        let form = v_flex()
            .gap(px(14.))
            .child(heading("Forgot your password?", cx))
            .child(paragraph(
                "Enter your 12-word recovery phrase and choose a new master password. Without the phrase, nobody can get you back in, not even us.",
                cx,
            ))
            .child(
                TextField::new(&self.phrase)
                    .label("Recovery phrase")
                    .icon(IconName::Note)
                    .when_some(phrase_error, |field, message| field.error(message)),
            )
            .child(
                TextField::new(&self.password)
                    .label("New master password")
                    .icon(IconName::Lock)
                    .trailing(reveal)
                    .when_some(password_error, |field, message| field.error(message)),
            )
            .when(!password.is_empty(), |form| {
                form.child(StrengthMeter::new(estimate.score))
            })
            .child(
                TextField::new(&self.confirm)
                    .label("Confirm new master password")
                    .icon(IconName::Lock)
                    .when(mismatch, |field| field.error("The two passwords don\u{2019}t match.")),
            )
            .when_some(self.failure.clone(), |form, message| {
                form.child(error_line(message, cx))
            })
            .child(
                Button::new(
                    "recover",
                    if self.busy { "Checking…" } else { "Set new password" },
                )
                .primary()
                .size(ButtonSize::Lg)
                .block()
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            )
            .child(h_flex().child(back));
        let _ = ButtonVariant::Ghost;
        auth_shell(form, cx)
    }
}
