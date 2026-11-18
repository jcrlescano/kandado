//! D-04 Unlock. Windows Hello first when it is on, the master password as the
//! fallback, "Forgot password?" for the recovery phrase and a Sign out link.
//! States: ready, checking, wrong password (with tries left), waiting after
//! five wrong passwords, Hello unavailable, signing out.

use gpui_kit::AppContext as _;
use gpui_kit::{
    Context, Entity, EventEmitter, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
    component::{
        h_flex,
        input::{InputEvent, InputState},
        v_flex,
    },
    div,
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::{Phase, UnlockError};

use super::super::{
    Nav, Services, auth_shell, explain, explain_unlock, heading, paragraph,
    system::sign_out_dialog, with_overlay,
};
use crate::{
    assets::IconName,
    components::{Button, ButtonSize, IconButton, Link, Logo, TextField},
    theme::{KandadoTheme, TextStyles as _, color, tokens},
};

pub struct UnlockView {
    services: Services,
    email: String,
    hello: bool,
    password: Entity<InputState>,
    revealed: bool,
    busy: bool,
    failure: Option<SharedString>,
    signing_out: Option<SignOut>,
    _subscriptions: Vec<Subscription>,
}

struct SignOut {
    busy: bool,
    failure: Option<SharedString>,
}

impl EventEmitter<Nav> for UnlockView {}

impl UnlockView {
    /// `hello` is true when Windows Hello is set up on this computer; the
    /// view then asks for it straight away.
    pub fn new(
        services: Services,
        email: String,
        hello: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let password = cx.new(|cx| InputState::new(window, cx).masked(true));
        let subscriptions = vec![cx.subscribe_in(
            &password,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.unlock_with_password(window, cx);
                }
            },
        )];
        let view = Self {
            services,
            email,
            hello,
            password,
            revealed: false,
            busy: false,
            failure: None,
            signing_out: None,
            _subscriptions: subscriptions,
        };
        view.password
            .clone()
            .update(cx, |input, cx| input.focus(window, cx));
        view
    }

    /// Asks for Windows Hello. Called on open when it is on.
    pub fn ask_hello(&mut self, cx: &mut Context<Self>) {
        if !self.hello || self.busy {
            return;
        }
        self.busy = true;
        self.failure = None;
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let result = services
                .tasks
                .run(async move { app.unlock_with_hello().await })
                .await;
            let _ = this.update(cx, |this, cx| this.finish(result, cx));
        })
        .detach();
    }

    fn unlock_with_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let password = self.password.read(cx).value().to_string();
        if password.is_empty() || self.busy {
            return;
        }
        self.busy = true;
        self.failure = None;
        cx.notify();
        let services = self.services.clone();
        cx.spawn_in(window, async move |this, cx| {
            let app = services.app.clone();
            let result = services
                .tasks
                .run(async move { app.unlock_with_password(password).await })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                // The typed password is gone whether or not it was right.
                this.password
                    .update(cx, |input, cx| input.set_value("", window, cx));
                this.finish(result, cx);
            });
        })
        .detach();
    }

    fn finish(&mut self, result: Result<Phase, UnlockError>, cx: &mut Context<Self>) {
        self.busy = false;
        match result {
            Ok(phase) => cx.emit(Nav::Phase(phase)),
            Err(UnlockError::HelloUnavailable) => {
                // Not a failed attempt: the password is still the way in.
                self.failure = None;
                cx.notify();
            }
            Err(error) => {
                self.failure = Some(explain_unlock(&error));
                cx.notify();
            }
        }
    }

    fn toggle_reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.revealed = !self.revealed;
        let masked = !self.revealed;
        self.password
            .update(cx, |input, cx| input.set_masked(masked, window, cx));
        cx.notify();
    }

    fn confirm_sign_out(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.signing_out.as_mut() else {
            return;
        };
        if state.busy {
            return;
        }
        state.busy = true;
        state.failure = None;
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let result = services
                .tasks
                .run(async move { app.sign_out().await })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(()) => cx.emit(Nav::Phase(Phase::SignedOut)),
                Err(error) => {
                    if let Some(state) = this.signing_out.as_mut() {
                        state.busy = false;
                        state.failure = Some(explain(&error));
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl Render for UnlockView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let reveal = IconButton::new(
            "reveal-password",
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
            .child(Logo::new().size(px(48.)))
            .child(heading("Unlock Kandado", cx))
            .child(paragraph(self.email.clone(), cx))
            .when(self.hello, |form| {
                form.child(
                    Button::new("unlock-hello", "Unlock with Windows Hello")
                        .primary()
                        .size(ButtonSize::Lg)
                        .icon(IconName::Fingerprint)
                        .block()
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.ask_hello(cx))),
                )
                .child(
                    div()
                        .text_token(tokens::LABEL)
                        .text_color(color(p.ink_muted))
                        .flex()
                        .justify_center()
                        .child("or"),
                )
            })
            .child(
                TextField::new(&self.password)
                    .label("Master password")
                    .icon(IconName::Lock)
                    .trailing(reveal)
                    .when_some(self.failure.clone(), |field, message| field.error(message)),
            )
            .child(
                Button::new("unlock", if self.busy { "Unlocking…" } else { "Unlock" })
                    .size(ButtonSize::Lg)
                    .when(!self.hello, |button| button.primary())
                    .block()
                    .disabled(self.busy)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.unlock_with_password(window, cx)),
                    ),
            )
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        Link::new("forgot-password", "Forgot password?")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(Nav::Recover))),
                    )
                    .child(Link::new("sign-out", "Sign out").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.signing_out = Some(SignOut {
                                busy: false,
                                failure: None,
                            });
                            cx.notify();
                        },
                    ))),
            );

        let dialog = self.signing_out.as_ref().map(|state| {
            sign_out_dialog(
                state.busy,
                state.failure.clone(),
                cx.listener(|this, _, _, cx| {
                    this.signing_out = None;
                    cx.notify();
                }),
                cx.listener(|this, _, _, cx| this.confirm_sign_out(cx)),
                cx,
            )
            .into_any_element()
        });
        with_overlay(auth_shell(form, cx), dialog)
    }
}
