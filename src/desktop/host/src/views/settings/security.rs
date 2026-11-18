//! D-44 Security: Windows Hello, when the app locks, and a note about the
//! recovery phrase. Changes save as they are made.
//!
//! Not here yet: capture protection, and the
//! PIN rows of the design, which belong to the browser extension.

use gpui_kit::{
    Context, IntoElement, ParentElement, Render, SharedString, Styled, Window,
    component::{
        button::Button as KitButton,
        menu::{DropdownMenu as _, PopupMenuItem},
        v_flex,
    },
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::settings::{ClipboardClear, LockAfter};

use super::{
    super::{Services, error_line, explain},
    rows::{SettingRow, group_label},
};
use crate::{
    assets::IconName,
    components::{FieldGroup, Toggle},
};

pub struct SecurityPage {
    services: Services,
    busy: bool,
    failure: Option<SharedString>,
}

impl SecurityPage {
    pub fn new(services: Services) -> Self {
        Self {
            services,
            busy: false,
            failure: None,
        }
    }

    fn set_hello(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.failure = None;
        if !on {
            if let Err(error) = self.services.app.disable_hello() {
                self.failure = Some(explain(&error));
            }
            cx.notify();
            return;
        }
        self.busy = true;
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let result = services
                .tasks
                .run(async move { app.enable_hello().await })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                if let Err(error) = result {
                    this.failure = Some(explain(&error));
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn set_lock_after(&mut self, choice: LockAfter, cx: &mut Context<Self>) {
        if let Err(error) = self
            .services
            .app
            .update_settings(|settings| settings.lock_after = choice)
        {
            self.failure = Some(explain(&error));
        }
        cx.notify();
    }

    fn set_clipboard_clear(&mut self, choice: ClipboardClear, cx: &mut Context<Self>) {
        if let Err(error) = self
            .services
            .app
            .update_settings(|settings| settings.clipboard_clear = choice)
        {
            self.failure = Some(explain(&error));
        }
        cx.notify();
    }

    fn set_lock_on_sleep(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Err(error) = self
            .services
            .app
            .update_settings(|settings| settings.lock_on_sleep = on)
        {
            self.failure = Some(explain(&error));
        }
        cx.notify();
    }
}

impl Render for SecurityPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self.services.app.settings();
        let hello_available = self.services.app.hello_available();
        let view = cx.entity();

        let lock_after = KitButton::new("lock-after")
            .label(settings.lock_after.label())
            .dropdown_menu(move |menu, _, _| {
                let current = settings.lock_after;
                LockAfter::ALL.into_iter().fold(menu, |menu, choice| {
                    let view = view.clone();
                    menu.item(
                        PopupMenuItem::new(choice.label())
                            .checked(choice == current)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| this.set_lock_after(choice, cx));
                            }),
                    )
                })
            });

        let view = cx.entity();
        let clipboard = KitButton::new("clipboard-clear")
            .label(settings.clipboard_clear.label())
            .dropdown_menu(move |menu, _, _| {
                let current = settings.clipboard_clear;
                ClipboardClear::ALL.into_iter().fold(menu, |menu, choice| {
                    let view = view.clone();
                    menu.item(
                        PopupMenuItem::new(choice.label())
                            .checked(choice == current)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| this.set_clipboard_clear(choice, cx));
                            }),
                    )
                })
            });

        v_flex()
            .gap(px(8.))
            .child(group_label("Unlock", cx))
            .child(
                FieldGroup::new().child(
                    SettingRow::new("row-hello", IconName::Fingerprint, "Unlock with Windows Hello")
                        .description(if hello_available {
                            "Use your face, fingerprint or PIN instead of typing the master password."
                        } else {
                            "Windows Hello is not available on this computer."
                        })
                        .trailing(
                            Toggle::new("toggle-hello")
                                .checked(settings.windows_hello)
                                .disabled(!hello_available || self.busy)
                                .on_change(cx.listener(|this, on: &bool, _, cx| {
                                    this.set_hello(*on, cx)
                                })),
                        ),
                ),
            )
            .child(group_label("Locking", cx))
            .child(
                FieldGroup::new()
                    .child(
                        SettingRow::new("row-lock-after", IconName::Clock, "Lock after")
                            .trailing(lock_after),
                    )
                    .child(
                        SettingRow::new(
                            "row-lock-sleep",
                            IconName::Lock,
                            "Lock when the computer sleeps or locks",
                        )
                        .trailing(
                            Toggle::new("toggle-lock-sleep")
                                .checked(settings.lock_on_sleep)
                                .on_change(cx.listener(|this, on: &bool, _, cx| {
                                    this.set_lock_on_sleep(*on, cx)
                                })),
                        ),
                    ),
            )
            .child(group_label("Clipboard", cx))
            .child(
                FieldGroup::new().child(
                    SettingRow::new("row-clipboard", IconName::Copy, "Clear clipboard after")
                        .description(
                            "Copied passwords and codes are kept out of clipboard history.",
                        )
                        .trailing(clipboard),
                ),
            )
            .child(group_label("Recovery", cx))
            .child(
                FieldGroup::new().child(
                    SettingRow::new("row-phrase", IconName::Note, "Recovery phrase").description(
                        "Shown once, when you created your account. Kandado never stores it, so it can\u{2019}t show it again.",
                    ),
                ),
            )
            .when_some(self.failure.clone(), |page, message| {
                page.child(error_line(message, cx))
            })
    }
}
