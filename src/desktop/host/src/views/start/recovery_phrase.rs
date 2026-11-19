//! D-03 Recovery phrase. 12 numbered words, Copy, Save as file, and a
//! checkbox that gates Continue. Nothing has been uploaded yet: Continue
//! uploads the keys, so closing the window here loses nothing.

use std::time::Duration;

use gpui_kit::{
    ClipboardItem, Context, EventEmitter, IntoElement, ParentElement, Render, SharedString, Styled,
    Window,
    component::{h_flex, v_flex},
    div,
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::KandadoApp;

use super::super::{Nav, Services, auth_shell, error_line, explain, heading, paragraph};
use crate::{
    assets::IconName,
    components::{Button, ButtonSize, FieldGroup, Toggle},
    theme::{KandadoTheme, color, family, tokens},
};

/// How long a copied phrase stays on the clipboard.
const CLIPBOARD_SECONDS: u64 = 60;

pub struct RecoveryPhraseView {
    services: Services,
    email: String,
    words: Vec<&'static str>,
    saved: bool,
    copied: bool,
    busy: bool,
    failure: Option<SharedString>,
    notice: Option<SharedString>,
}

impl EventEmitter<Nav> for RecoveryPhraseView {}

impl RecoveryPhraseView {
    pub fn new(services: Services, email: String) -> Self {
        let words = services.app.pending_phrase().unwrap_or_default();
        Self {
            services,
            email,
            words,
            saved: false,
            copied: false,
            busy: false,
            failure: None,
            notice: None,
        }
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        let text = self.words.join(" ");
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        self.copied = true;
        cx.notify();
        // Take the phrase off the clipboard again, unless something else has
        // been copied since.
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(CLIPBOARD_SECONDS))
                .await;
            let _ = this.update(cx, |this, cx| {
                let still_ours = cx
                    .read_from_clipboard()
                    .and_then(|item| item.text())
                    .is_some_and(|current| current == text);
                if still_ours {
                    cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
                }
                this.copied = false;
                cx.notify();
            });
        })
        .detach();
    }

    fn save_file(&mut self, cx: &mut Context<Self>) {
        let contents = KandadoApp::recovery_file_text(&self.words, &self.email);
        let folder = std::env::var_os("USERPROFILE")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        let chosen = cx.prompt_for_new_path(&folder, Some("kandado-recovery-phrase.txt"));
        cx.spawn(async move |this, cx| {
            let outcome = match chosen.await {
                Ok(Ok(Some(path))) => Some(
                    std::fs::write(&path, contents)
                        .map(|()| path)
                        .map_err(|error| error.to_string()),
                ),
                // Cancelled or the dialog failed: nothing to say.
                _ => None,
            };
            let _ = this.update(cx, |this, cx| {
                match outcome {
                    Some(Ok(path)) => {
                        this.notice = Some(format!("Saved to {}", path.display()).into());
                        this.failure = None;
                    }
                    Some(Err(_)) => {
                        this.failure = Some("Kandado could not save the file there.".into());
                    }
                    None => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn proceed(&mut self, cx: &mut Context<Self>) {
        if !self.saved || self.busy {
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
                .run(async move { app.confirm_recovery_saved().await })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(phase) => cx.emit(Nav::Phase(phase)),
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

impl Render for RecoveryPhraseView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let grid = div().grid().grid_cols(2).gap(px(tokens::SPACE_2)).children(
            self.words.iter().enumerate().map(|(index, word)| {
                h_flex()
                    .items_center()
                    .gap(px(10.))
                    .h(px(40.))
                    .px(px(tokens::SPACE_3))
                    .rounded(px(tokens::RADIUS_KEY))
                    .bg(color(p.raised))
                    .font_family(tokens::FONT_MONO)
                    .text_size(px(14.))
                    .text_color(color(p.ink))
                    .child(
                        div()
                            .min_w(px(16.))
                            .text_size(px(12.))
                            .text_color(color(p.ink_2))
                            .child(format!("{}", index + 1)),
                    )
                    .child(*word)
            }),
        );
        let _ = family;

        let form = v_flex()
            .gap(px(14.))
            .child(heading("Save your recovery phrase", cx))
            .child(paragraph(
                "If you forget your password, these 12 words are the only way back in.",
                cx,
            ))
            .child(grid)
            .child(
                h_flex()
                    .gap(px(tokens::SPACE_2))
                    .child(
                        Button::new("copy-phrase", if self.copied { "Copied" } else { "Copy" })
                            .icon(if self.copied {
                                IconName::Check
                            } else {
                                IconName::Copy
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                    )
                    .child(
                        Button::new("save-phrase", "Save as file")
                            .icon(IconName::Share)
                            .on_click(cx.listener(|this, _, _, cx| this.save_file(cx))),
                    ),
            )
            .when_some(self.notice.clone(), |form, notice| {
                form.child(paragraph(notice, cx))
            })
            .child(
                FieldGroup::new().child(
                    Toggle::new("phrase-saved")
                        .label("I saved my recovery phrase somewhere safe")
                        .checked(self.saved)
                        .on_change(cx.listener(|this, checked: &bool, _, cx| {
                            this.saved = *checked;
                            cx.notify();
                        })),
                ),
            )
            .when_some(self.failure.clone(), |form, message| {
                form.child(error_line(message, cx))
            })
            .child(
                Button::new(
                    "continue-after-phrase",
                    if self.busy { "Saving…" } else { "Continue" },
                )
                .primary()
                .size(ButtonSize::Lg)
                .block()
                .disabled(!self.saved || self.busy)
                .on_click(cx.listener(|this, _, _, cx| this.proceed(cx))),
            );
        auth_shell(form, cx)
    }
}
