//! D-20 Add two-factor code: paste the setup key, or let Kandado read a QR
//! code that is visible on screen. A live code preview confirms the key works
//! before it is added. States: empty, key not valid, preview, scanning, no QR
//! code found.

use std::time::Duration;

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
use kandado_app::totp_import::{self, SetupKey, TotpImportError};
use kandado_model::{Secret, totp::Totp};

use super::super::{Services, paragraph};
use crate::{
    assets::IconName,
    components::{
        Button, Dialog, FieldGroup, Segmented, TextField, TotpCode, section_label, window_layer,
    },
};

#[derive(Debug, Clone)]
pub enum AddTotpEvent {
    /// The `otpauth://` URI to store on the login.
    Added(Secret),
    Cancelled,
}

pub struct AddTotp {
    services: Services,
    /// False: enter the key. True: scan the screen.
    scan: bool,
    key: Entity<InputState>,
    found: Option<SetupKey>,
    scanning: bool,
    error: Option<SharedString>,
    _subscription: Subscription,
}

impl EventEmitter<AddTotpEvent> for AddTotp {}

impl AddTotp {
    pub fn new(services: Services, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let key = cx.new(|cx| InputState::new(window, cx).placeholder("JBSW Y3DP EHPK 3PXP"));
        let subscription = cx.subscribe(&key, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Change => this.key_changed(cx),
            InputEvent::PressEnter { .. } => this.add(cx),
            _ => {}
        });
        key.update(cx, |input, cx| input.focus(window, cx));
        // The preview counts down once a second.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
            services,
            scan: false,
            key,
            found: None,
            scanning: false,
            error: None,
            _subscription: subscription,
        }
    }

    fn key_changed(&mut self, cx: &mut Context<Self>) {
        let typed = self.key.read(cx).value();
        if typed.trim().is_empty() {
            self.found = None;
            self.error = None;
        } else {
            match totp_import::parse(&typed) {
                Ok(key) => {
                    self.found = Some(key);
                    self.error = None;
                }
                Err(_) => {
                    self.found = None;
                    self.error = Some(
                        "That is not a setup key. Copy it again from the site's two-factor page."
                            .into(),
                    );
                }
            }
        }
        cx.notify();
    }

    pub fn set_mode(&mut self, scan: bool, cx: &mut Context<Self>) {
        if self.scan == scan {
            return;
        }
        self.scan = scan;
        self.found = None;
        self.error = None;
        if scan {
            self.scan_screen(cx);
        } else {
            self.key_changed(cx);
        }
        cx.notify();
    }

    pub fn scan_screen(&mut self, cx: &mut Context<Self>) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        self.error = None;
        cx.notify();
        let tasks = self.services.tasks.clone();
        cx.spawn(async move |this, cx| {
            let result = tasks.run(totp_import::scan_screen_in_background()).await;
            let _ = this.update(cx, |this, cx| {
                this.scanning = false;
                match result {
                    Ok(key) => this.found = Some(key),
                    Err(TotpImportError::Capture) => {
                        this.error = Some(
                            "Kandado could not read the screen. Paste the setup key instead."
                                .into(),
                        );
                    }
                    Err(_) => {
                        this.error = Some(
                            "No QR code found. Show the code on screen, then scan again.".into(),
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn add(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = self.found.take() {
            cx.emit(AddTotpEvent::Added(key.uri));
        }
    }
}

impl Render for AddTotp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let preview = self.found.as_ref().and_then(|key| {
            let totp = Totp::parse(key.uri.expose()).ok()?;
            Some((
                totp.code_at(kandado_app::vault::now() as u64),
                key.issuer.clone(),
                key.account.clone(),
            ))
        });

        let mut dialog = Dialog::new("Add two-factor code").child(
            Segmented::new(
                "totp-mode",
                ["Enter setup key", "Scan QR on screen"],
                usize::from(self.scan),
            )
            .on_select(move |index, _, cx| {
                view.update(cx, |this, cx| this.set_mode(index == 1, cx));
            }),
        );
        if self.scan {
            dialog = dialog.child(paragraph(
                if self.scanning {
                    "Looking for a QR code on your screen…"
                } else if self.found.is_some() {
                    "Found a setup code on screen."
                } else {
                    "Show the site's QR code anywhere on screen. Kandado reads it here and keeps no picture."
                },
                cx,
            ));
            if let Some(error) = self.error.clone() {
                dialog = dialog.child(super::super::error_line(error, cx));
            }
            if !self.scanning && self.found.is_none() {
                dialog = dialog.child(
                    v_flex().items_start().child(
                        Button::new("scan-again", "Scan again")
                            .icon(IconName::Refresh)
                            .on_click(cx.listener(|this, _, _, cx| this.scan_screen(cx))),
                    ),
                );
            }
        } else {
            dialog = dialog.child(
                TextField::new(&self.key)
                    .label("Setup key")
                    .icon(IconName::Key)
                    .mono(true)
                    .when_some(self.error.clone(), |field, error| field.error(error)),
            );
        }
        if let Some((code, issuer, account)) = preview {
            let who = [issuer, account]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            dialog = dialog
                .child(section_label(
                    if who.is_empty() {
                        "Preview".to_owned()
                    } else {
                        format!("Preview · {who}")
                    },
                    cx,
                ))
                .child(FieldGroup::new().child(v_flex().pr(px(40.)).child(
                    TotpCode::new("totp-preview", code.digits, code.remaining).period(code.period),
                )));
        }
        let ready = self.found.is_some();
        window_layer(
            dialog
                .action(
                    Button::new("totp-cancel", "Cancel")
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(AddTotpEvent::Cancelled))),
                )
                .action(
                    Button::new("totp-add", "Add code")
                        .primary()
                        .disabled(!ready)
                        .on_click(cx.listener(|this, _, _, cx| this.add(cx))),
                ),
            window,
        )
    }
}
