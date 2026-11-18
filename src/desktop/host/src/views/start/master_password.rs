//! D-02 Create master password. The email is Google's and read-only. The
//! password is checked on this computer; nothing is sent until the recovery
//! phrase has been saved (D-03).

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
use kandado_app::service::{MIN_PASSWORD_LEN, MIN_PASSWORD_SCORE};
use kandado_model::strength;

use super::super::{
    Nav, Services, auth_shell, error_line, explain, heading, paragraph, read_only_field, small,
};
use crate::{
    assets::IconName,
    components::{Button, ButtonSize, IconButton, StrengthMeter, TextField},
};

pub struct MasterPasswordView {
    services: Services,
    email: String,
    password: Entity<InputState>,
    confirm: Entity<InputState>,
    revealed: bool,
    busy: bool,
    /// Errors appear after the first try to continue, not while typing.
    submitted: bool,
    failure: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<Nav> for MasterPasswordView {}

impl MasterPasswordView {
    pub fn new(
        services: Services,
        email: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let password = cx.new(|cx| InputState::new(window, cx).masked(true));
        let confirm = cx.new(|cx| InputState::new(window, cx).masked(true));
        let subscriptions = [&password, &confirm]
            .into_iter()
            .map(|input| cx.subscribe(input, |_, _, _: &InputEvent, cx| cx.notify()))
            .collect();
        Self {
            services,
            email,
            password,
            confirm,
            revealed: false,
            busy: false,
            submitted: false,
            failure: None,
            _subscriptions: subscriptions,
        }
    }

    fn typed(&self, cx: &Context<Self>) -> (String, String) {
        (
            self.password.read(cx).value().to_string(),
            self.confirm.read(cx).value().to_string(),
        )
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

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.submitted = true;
        self.failure = None;
        let (password, confirm) = self.typed(cx);
        if self.password_problem(&password).is_some() || password != confirm {
            cx.notify();
            return;
        }
        self.busy = true;
        cx.notify();
        let services = self.services.clone();
        let email = self.email.clone();
        cx.spawn_in(window, async move |this, cx| {
            let app = services.app.clone();
            let result = services
                .tasks
                .run(async move { app.create_master_password(password).await })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(_) => {
                        // The phrase is fetched by the next screen. Wipe what was typed.
                        for input in [&this.password, &this.confirm] {
                            input.update(cx, |input, cx| input.set_value("", window, cx));
                        }
                        cx.emit(Nav::Phrase { email });
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
}

impl Render for MasterPasswordView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (password, confirm) = self.typed(cx);
        let estimate = strength::estimate(&password, &[self.email.as_str()]);
        let password_error = self
            .submitted
            .then(|| self.password_problem(&password))
            .flatten();
        let mismatch = self.submitted && !confirm.is_empty() && password != confirm
            || self.submitted && confirm.is_empty();

        let reveal = IconButton::new(
            "reveal-passwords",
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
        .size(ButtonSize::Sm)
        .on_click(cx.listener(|this, _, window, cx| this.toggle_reveal(window, cx)));

        let form = v_flex()
            .gap(px(14.))
            .child(heading("Create your master password", cx))
            .child(paragraph(
                "It encrypts everything on this computer. Kandado can\u{2019}t reset it, so pick one you will remember.",
                cx,
            ))
            .child(read_only_field("Google account", self.email.clone(), IconName::Mail, cx))
            .child(
                TextField::new(&self.password)
                    .label("Master password")
                    .icon(IconName::Lock)
                    .trailing(reveal)
                    .when_some(password_error, |field, message| field.error(message)),
            )
            .when(!password.is_empty(), |form| {
                form.child(StrengthMeter::new(estimate.score))
            })
            .child(
                TextField::new(&self.confirm)
                    .label("Confirm master password")
                    .icon(IconName::Lock)
                    .when(mismatch, |field| field.error("The two passwords don\u{2019}t match.")),
            )
            .child(small(
                "Next you\u{2019}ll get a recovery phrase. It\u{2019}s the only way back in if you forget this password.",
                cx,
            ))
            .when_some(self.failure.clone(), |form, message| {
                form.child(error_line(message, cx))
            })
            .child(
                h_flex().child(
                    Button::new(
                        "create-master-password",
                        if self.busy {
                            "Creating your keys…"
                        } else {
                            "Continue"
                        },
                    )
                    .primary()
                    .size(ButtonSize::Lg)
                    .block()
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                ),
            );
        let _ = window;
        auth_shell(form, cx)
    }
}
