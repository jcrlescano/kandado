//! D-01 Sign in. Google is the only way in, so the form is one button.
//! States: ready, waiting for the browser, failed.

use gpui_kit::{
    Context, EventEmitter, IntoElement, ParentElement, Render, SharedString, Styled, Window,
    component::{h_flex, v_flex},
    div,
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::{platform, tasks::Canceller};

use super::super::{Nav, Services, auth_shell, error_line, explain, heading, paragraph, small};
use crate::{
    assets::IconName,
    components::{Button, ButtonSize, ButtonVariant, Link},
    theme::{KandadoTheme, color, tokens},
};

enum State {
    Ready,
    /// The browser is open; waiting for Google to send the person back.
    Waiting(Canceller),
    Failed(SharedString),
}

pub struct SignInView {
    services: Services,
    state: State,
}

impl EventEmitter<Nav> for SignInView {}

impl SignInView {
    pub fn new(services: Services) -> Self {
        Self {
            services,
            state: State::Ready,
        }
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        if matches!(self.state, State::Waiting(_)) {
            return;
        }
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let begun = services
                .tasks
                .run(async move { app.begin_google().await })
                .await;
            let pending = match begun {
                Ok(pending) => pending,
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.state = State::Failed(explain(&error));
                        cx.notify();
                    });
                    return;
                }
            };
            if let Err(error) = platform::open_url(pending.url.as_str()) {
                tracing::error!(%error, "could not open the browser");
                let _ = this.update(cx, |this, cx| {
                    this.state = State::Failed(
                        "Kandado could not open your browser. Open it yourself and try again."
                            .into(),
                    );
                    cx.notify();
                });
                return;
            }
            let app = services.app.clone();
            let (outcome, canceller) = services
                .tasks
                .run_cancellable(async move { app.finish_google(pending).await });
            let _ = this.update(cx, |this, cx| {
                this.state = State::Waiting(canceller);
                cx.notify();
            });
            let result = outcome.await;
            let _ = this.update(cx, |this, cx| match result {
                // Cancelled by the person.
                None => {
                    this.state = State::Ready;
                    cx.notify();
                }
                Some(Ok(Some(phase))) => cx.emit(Nav::Phase(phase)),
                Some(Ok(None)) => {
                    this.state = State::Ready;
                    cx.notify();
                }
                Some(Err(error)) => {
                    this.state = State::Failed(explain(&error));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        if let State::Waiting(canceller) = &self.state {
            canceller.cancel();
        }
        self.state = State::Ready;
        cx.notify();
    }
}

impl Render for SignInView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let waiting = matches!(self.state, State::Waiting(_));
        let form = v_flex()
            .gap(px(14.))
            .child(heading("Sign in", cx))
            .child(paragraph(
                "Use your Google account. Kandado never sees your Google password.",
                cx,
            ))
            .child(
                Button::new(
                    "continue-with-google",
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
                .disabled(waiting)
                .on_click(cx.listener(|this, _, _, cx| this.start(cx))),
            )
            .when(waiting, |form| {
                form.child(
                    v_flex()
                        .gap(px(tokens::SPACE_2))
                        .child(small(
                            "Finish signing in in your browser, then come back here.",
                            cx,
                        ))
                        .child(
                            h_flex().child(
                                Link::new("cancel-sign-in", "Cancel")
                                    .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
                            ),
                        ),
                )
            })
            .when_some(
                match &self.state {
                    State::Failed(message) => Some(message.clone()),
                    _ => None,
                },
                |form, message| form.child(error_line(message, cx)),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(color(p.ink_3))
                    .child("New here? The same button creates your account."),
            );
        let _ = ButtonVariant::Ghost;
        auth_shell(form, cx)
    }
}
