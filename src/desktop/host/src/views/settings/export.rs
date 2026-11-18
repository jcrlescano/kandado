//! D-48 Export: a format (encrypted, JSON, CSV), the account password asked
//! for again, and the system save dialog. JSON and CSV are not encrypted, and
//! the page says so.
//!
//! States: ready, wrong password, exporting, saved (with what a format left
//! out), could not save.

use std::path::PathBuf;

use gpui_kit::AppContext as _;
use gpui_kit::{
    Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription,
    Window,
    component::{
        input::{InputEvent, InputState},
        v_flex,
    },
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::{ExportFormat, transfer::TransferError};

use super::{
    super::{Services, error_line, explain, paragraph, small},
    rows::group_label,
};
use crate::{
    assets::IconName,
    components::{Banner, Button, ButtonSize, IconButton, Segmented, TextField, Tone},
    theme::tokens,
};

const FORMATS: [(ExportFormat, &str); 3] = [
    (ExportFormat::Encrypted, "Encrypted"),
    (ExportFormat::Json, "JSON"),
    (ExportFormat::Csv, "CSV"),
];

enum State {
    Ready,
    Exporting,
    Saved {
        path: PathBuf,
        items: usize,
        omitted: usize,
    },
    Failed(SharedString),
}

pub struct ExportPage {
    services: Services,
    format: ExportFormat,
    password: Entity<InputState>,
    shown: bool,
    wrong: bool,
    state: State,
    _subscription: Subscription,
}

fn plural(count: usize) -> String {
    format!("{count} {}", if count == 1 { "item" } else { "items" })
}

/// The file name the save dialog starts with: `kandado-export-2026-11-17.json`.
fn file_name(format: ExportFormat, unix: i64) -> String {
    let date = time::OffsetDateTime::from_unix_timestamp(unix)
        .map(|moment| moment.date().to_string())
        .unwrap_or_default();
    format!("kandado-export-{date}.{}", format.extension())
}

impl ExportPage {
    pub fn new(services: Services, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let password = cx.new(|cx| InputState::new(window, cx).masked(true));
        let subscription = cx.subscribe_in(
            &password,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.choose_file(window, cx),
                InputEvent::Change => {
                    this.wrong = false;
                    cx.notify();
                }
                _ => {}
            },
        );
        Self {
            services,
            format: ExportFormat::Encrypted,
            password,
            shown: false,
            wrong: false,
            state: State::Ready,
            _subscription: subscription,
        }
    }

    pub fn set_format(&mut self, format: ExportFormat, cx: &mut Context<Self>) {
        if matches!(self.state, State::Exporting) {
            return;
        }
        self.format = format;
        self.state = State::Ready;
        cx.notify();
    }

    fn toggle_shown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shown = !self.shown;
        let masked = !self.shown;
        self.password
            .update(cx, |input, cx| input.set_masked(masked, window, cx));
        cx.notify();
    }

    /// Asks where to save, then exports there.
    fn choose_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.state, State::Exporting) || self.password.read(cx).value().is_empty() {
            return;
        }
        let format = self.format;
        let name = file_name(format, kandado_app::vault::now());
        let start = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("Documents");
        cx.spawn_in(window, async move |this, cx| {
            let Ok(receiver) = cx.update(|_, cx| cx.prompt_for_new_path(&start, Some(&name)))
            else {
                return;
            };
            if let Ok(Ok(Some(path))) = receiver.await {
                let _ = this.update_in(cx, |this, window, cx| this.export_to(path, window, cx));
            }
        })
        .detach();
    }

    /// Checks the password and writes the file.
    pub fn export_to(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let password = self.password.read(cx).value().to_string();
        let format = self.format;
        self.state = State::Exporting;
        self.wrong = false;
        cx.notify();
        let services = self.services.clone();
        cx.spawn_in(window, async move |this, cx| {
            let app = services.app.clone();
            let result = services
                .tasks
                .run(async move { app.export_to(&path, format, password).await })
                .await;
            let _ = this.update_in(cx, |this, window, cx| this.finished(result, window, cx));
        })
        .detach();
    }

    fn finished(
        &mut self,
        result: Result<kandado_app::transfer::ExportSummary, TransferError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(summary) => {
                // The password is gone once it has been used.
                self.password
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.state = State::Saved {
                    path: summary.path,
                    items: summary.items,
                    omitted: summary.omitted,
                };
            }
            Err(TransferError::WrongPassword) => {
                self.wrong = true;
                self.state = State::Ready;
            }
            Err(error) => {
                self.state = State::Failed(match error {
                    TransferError::Io(_) => {
                        "Kandado could not save the file there. Choose another place.".into()
                    }
                    TransferError::App(error) => explain(&error),
                    _ => "The export did not finish. Try again.".into(),
                });
            }
        }
        cx.notify();
    }

    /// For tests: what the page is showing.
    pub fn showing(&self) -> &'static str {
        match &self.state {
            State::Ready if self.wrong => "wrong password",
            State::Ready => "ready",
            State::Exporting => "exporting",
            State::Saved { .. } => "saved",
            State::Failed(_) => "failed",
        }
    }
}

impl Render for ExportPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (all, csv) = self.services.app.export_counts();
        let count = if self.format == ExportFormat::Csv {
            csv
        } else {
            all
        };
        let selected = FORMATS
            .iter()
            .position(|(format, _)| *format == self.format)
            .unwrap_or(0);
        let view = cx.entity();
        let exporting = matches!(self.state, State::Exporting);

        let reveal = IconButton::new(
            "reveal-export-password",
            if self.shown {
                IconName::EyeOff
            } else {
                IconName::Eye
            },
            if self.shown {
                "Hide password"
            } else {
                "Show password"
            },
        )
        .size(ButtonSize::Sm)
        .on_click(cx.listener(|this, _, window, cx| this.toggle_shown(window, cx)));

        let mut page = v_flex()
            .gap(px(tokens::SPACE_3))
            .child(group_label("Format", cx))
            .child(
                Segmented::new("export-format", FORMATS.map(|(_, label)| label), selected)
                    .on_select(move |index, _, cx| {
                        view.update(cx, |this, cx| this.set_format(FORMATS[index].0, cx));
                    }),
            )
            .child(paragraph(
                match self.format {
                    ExportFormat::Encrypted => {
                        "Encrypted exports need your account password to open. Only Kandado can import them."
                    }
                    ExportFormat::Json => {
                        "Every vault and item, in a file other tools can read."
                    }
                    ExportFormat::Csv => {
                        "Logins and notes, in the columns other password managers import."
                    }
                },
                cx,
            ));
        if self.format != ExportFormat::Encrypted {
            page = page.child(
                Banner::new(
                    Tone::Warning,
                    "JSON and CSV are not encrypted. Anyone with the file can read every password.",
                )
                .icon(IconName::Alert),
            );
        }
        if self.format == ExportFormat::Csv && all > csv {
            page = page.child(small(
                format!(
                    "{} (cards, identities and items in the trash) can\u{2019}t be in a CSV. Use JSON or an encrypted export to keep them.",
                    plural(all - csv)
                ),
                cx,
            ));
        }
        page = page.child(
            TextField::new(&self.password)
                .label("Confirm your account password")
                .icon(IconName::Lock)
                .trailing(reveal)
                .when(self.wrong, |field| {
                    field.error("That is not your account password.")
                }),
        );
        if let State::Failed(message) = &self.state {
            page = page.child(error_line(message.clone(), cx));
        }
        page = page.child(
            Button::new(
                "export-run",
                if exporting {
                    "Exporting\u{2026}".to_owned()
                } else {
                    format!("Export {}", plural(count))
                },
            )
            .primary()
            .icon(IconName::Share)
            .block()
            .disabled(exporting || count == 0)
            .on_click(cx.listener(|this, _, window, cx| this.choose_file(window, cx))),
        );
        if let State::Saved {
            path,
            items,
            omitted,
        } = &self.state
        {
            let mut message = format!("Saved {} to {}.", plural(*items), path.display());
            if *omitted > 0 {
                message.push_str(&format!(" {} left out.", plural(*omitted)));
            }
            page = page.child(Banner::new(Tone::Teal, message).icon(IconName::Check));
        }
        page
    }
}

impl ExportPage {
    /// The password field, for tests.
    pub fn password_input(&self) -> &Entity<InputState> {
        &self.password
    }
}
