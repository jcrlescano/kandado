//! D-47 Import: pick a source, drop or choose the file, see what it holds,
//! choose where it goes, import. The same pages are reached from the first-run
//! offer (D-05) with the source already chosen.
//!
//! States: sources, waiting for a file, reading, needs the export's password,
//! could not read, preview, importing (with progress and Cancel), done (with
//! Review of what was left out).

use std::{path::PathBuf, sync::Arc, time::Duration};

use gpui_kit::AppContext as _;
use gpui_kit::{
    Context, Entity, EventEmitter, ExternalPaths, InteractiveElement as _, IntoElement,
    ParentElement, PathPromptOptions, Render, SharedString, Styled, Subscription, Window,
    component::{
        button::Button as KitButton,
        h_flex,
        input::{InputEvent, InputState},
        menu::{DropdownMenu as _, PopupMenuItem},
        v_flex,
    },
    div,
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::{
    ImportError, ImportReport, Source,
    transfer::{ImportChoice, ImportProgress, Preview, Reading, TransferError},
};
use uuid::Uuid;

use super::{
    super::{Nav, Services, error_line, explain, paragraph, small},
    rows::{SettingRow, group_label},
};
use crate::{
    assets::IconName,
    components::{
        Banner, Button, ButtonVariant, FieldGroup, Link, ProgressBar, TextField, Toggle, Tone,
        drop_zone, section_label,
    },
    theme::{KandadoTheme, TextStyles as _, color, tokens},
};

/// Where the imported items go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Destination {
    Vault(Uuid),
    /// A vault per vault of the source.
    Source,
}

enum State {
    /// The list of sources.
    Sources,
    /// A source is chosen; waiting for a file.
    File {
        source: Source,
    },
    Reading {
        source: Source,
        name: String,
    },
    /// An encrypted export: ask for its password.
    Passphrase {
        source: Source,
        path: PathBuf,
        wrong: bool,
    },
    Failed {
        source: Source,
        message: SharedString,
    },
    Preview(Box<Previewing>),
    Importing {
        preview: Box<Preview>,
        progress: Arc<ImportProgress>,
    },
    Done {
        report: Box<ImportReport>,
        review: bool,
    },
}

struct Previewing {
    reading: Reading,
    destination: Destination,
    include_trashed: bool,
    review: bool,
    failure: Option<SharedString>,
}

pub struct ImportPage {
    services: Services,
    state: State,
    passphrase: Entity<InputState>,
    _subscription: Subscription,
}

impl EventEmitter<Nav> for ImportPage {}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn size_text(bytes: u64) -> String {
    match bytes {
        0..=1_023 => format!("{bytes} B"),
        1_024..=1_048_575 => format!("{:.0} KB", bytes as f64 / 1_024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

/// "210 imported, 4 skipped": the line of the last import.
pub fn report_line(report: &ImportReport) -> String {
    let mut line = format!("{} imported", report.imported);
    if !report.skipped.is_empty() {
        line.push_str(&format!(", {} skipped", report.skipped.len()));
    }
    line
}

impl ImportPage {
    pub fn new(services: Services, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let passphrase = cx.new(|cx| InputState::new(window, cx).masked(true));
        let subscription =
            cx.subscribe_in(&passphrase, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.open_with_passphrase(cx);
                }
            });
        Self {
            services,
            state: State::Sources,
            passphrase,
            _subscription: subscription,
        }
    }

    /// Opens the page on a source, as the first-run offer does.
    pub fn start(&mut self, source: Option<Source>, cx: &mut Context<Self>) {
        // Leave an import that is running alone.
        if matches!(self.state, State::Importing { .. } | State::Reading { .. }) {
            return;
        }
        self.state = match source {
            Some(source) => State::File { source },
            None => State::Sources,
        };
        cx.notify();
    }

    pub fn choose_source(&mut self, source: Source, cx: &mut Context<Self>) {
        self.state = State::File { source };
        cx.notify();
    }

    /// Back to the list of sources.
    pub fn back(&mut self, cx: &mut Context<Self>) {
        if matches!(self.state, State::Importing { .. } | State::Reading { .. }) {
            return;
        }
        self.state = State::Sources;
        cx.notify();
    }

    fn source(&self) -> Option<Source> {
        match &self.state {
            State::File { source }
            | State::Reading { source, .. }
            | State::Passphrase { source, .. }
            | State::Failed { source, .. } => Some(*source),
            State::Preview(previewing) => Some(previewing.reading.preview.source),
            _ => None,
        }
    }

    /// Opens the system file picker.
    fn pick_file(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.source() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let options = PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some(format!("Choose your {} export", source.name()).into()),
            };
            let receiver = cx.update(|cx| cx.prompt_for_paths(options));
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update(cx, |this, cx| this.load(source, path, None, cx));
            }
        })
        .detach();
    }

    /// Reads a file. Called with what was chosen or dropped.
    pub fn load(
        &mut self,
        source: Source,
        path: PathBuf,
        passphrase: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.state = State::Reading { source, name };
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let read_path = path.clone();
            let result = services
                .tasks
                .run(async move { app.read_import(source, &read_path, passphrase).await })
                .await;
            let _ = this.update(cx, |this, cx| this.loaded(source, path, result, cx));
        })
        .detach();
    }

    fn loaded(
        &mut self,
        source: Source,
        path: PathBuf,
        result: Result<Reading, TransferError>,
        cx: &mut Context<Self>,
    ) {
        self.state = match result {
            Ok(reading) => {
                let vaults = self.services.app.vaults().unwrap_or_default();
                let destination = if reading.preview.vaults.len() > 1 {
                    Destination::Source
                } else {
                    vaults
                        .first()
                        .map_or(Destination::Source, |vault| Destination::Vault(vault.id))
                };
                State::Preview(Box::new(Previewing {
                    reading,
                    destination,
                    include_trashed: true,
                    review: false,
                    failure: None,
                }))
            }
            Err(TransferError::Import(ImportError::NeedsPassphrase)) => State::Passphrase {
                source,
                path,
                wrong: false,
            },
            Err(TransferError::Import(ImportError::WrongPassphrase)) => State::Passphrase {
                source,
                path,
                wrong: true,
            },
            Err(error) => State::Failed {
                source,
                message: read_error(&error),
            },
        };
        cx.notify();
    }

    fn open_with_passphrase(&mut self, cx: &mut Context<Self>) {
        let State::Passphrase { source, path, .. } = &self.state else {
            return;
        };
        let (source, path) = (*source, path.clone());
        let passphrase = self.passphrase.read(cx).value().to_string();
        if passphrase.is_empty() {
            return;
        }
        self.load(source, path, Some(passphrase), cx);
    }

    pub fn start_import(&mut self, cx: &mut Context<Self>) {
        let State::Preview(previewing) = &self.state else {
            return;
        };
        let (plan, preview) = (
            previewing.reading.plan.clone(),
            Box::new(previewing.reading.preview.clone()),
        );
        let choice = match previewing.destination {
            Destination::Vault(id) => ImportChoice::Into(id),
            Destination::Source => ImportChoice::MatchSource,
        };
        let include_trashed = previewing.include_trashed;
        let progress = ImportProgress::new();
        self.state = State::Importing {
            preview: preview.clone(),
            progress: progress.clone(),
        };
        cx.notify();
        // Redraw the bar while it runs.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let running = this
                    .update(cx, |this, cx| {
                        if matches!(this.state, State::Importing { .. }) {
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        })
        .detach();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let run_progress = progress.clone();
            let result = services
                .tasks
                .run(async move {
                    app.run_import(plan, choice, include_trashed, run_progress)
                        .await
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.state = match result {
                    Ok(report) => State::Done {
                        report: Box::new(report),
                        review: false,
                    },
                    Err(TransferError::Cancelled) => State::Sources,
                    Err(error) => State::Failed {
                        source: preview.source,
                        message: read_error(&error),
                    },
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub fn cancel_import(&mut self) {
        if let State::Importing { progress, .. } = &self.state {
            progress.cancel();
        }
    }

    /// For tests: what the page is showing.
    pub fn showing(&self) -> &'static str {
        match &self.state {
            State::Sources => "sources",
            State::File { .. } => "file",
            State::Reading { .. } => "reading",
            State::Passphrase { .. } => "passphrase",
            State::Failed { .. } => "failed",
            State::Preview(_) => "preview",
            State::Importing { .. } => "importing",
            State::Done { .. } => "done",
        }
    }

    // ---- drawing -----------------------------------------------------------

    fn source_rows(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut group = FieldGroup::new();
        for (index, source) in Source::ALL.into_iter().enumerate() {
            let icon = match source {
                Source::Browser => IconName::Globe,
                Source::Csv => IconName::Note,
                Source::Kandado => IconName::Lock,
                _ => IconName::Shield,
            };
            let mut row = SettingRow::new(("import-source", index), icon, source.name());
            if !source.files().is_empty() {
                row = row.description(source.files());
            }
            group = group.child(
                row.opens(cx.listener(move |this, _, _, cx| this.choose_source(source, cx))),
            );
        }
        v_flex()
            .gap(px(8.))
            .child(group_label("Import from", cx))
            .child(group)
    }

    fn back_link(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let label = self
            .source()
            .map_or("Choose another source".to_owned(), |source| {
                format!(
                    "Importing from {} \u{b7} choose another source",
                    source.name()
                )
            });
        h_flex().child(
            Link::new("import-back", label).on_click(cx.listener(|this, _, _, cx| this.back(cx))),
        )
    }

    fn preview_view(&self, previewing: &Previewing, cx: &mut Context<Self>) -> impl IntoElement {
        let preview = &previewing.reading.preview;
        let vaults = self.services.app.vaults().unwrap_or_default();
        let destination = previewing.destination;
        let current = match destination {
            Destination::Vault(id) => vaults
                .iter()
                .find(|vault| vault.id == id)
                .map_or("Choose a vault".to_owned(), |vault| vault.meta.name.clone()),
            Destination::Source => format!("Keep {}\u{2019}s vaults", preview.source.name()),
        };
        let view = cx.entity();
        let names: Vec<(Uuid, String)> = vaults
            .iter()
            .map(|vault| (vault.id, vault.meta.name.clone()))
            .collect();
        let has_source_vaults = !preview.vaults.is_empty();
        let source_label = format!(
            "Keep {}\u{2019}s vaults ({})",
            preview.source.name(),
            preview.vaults.join(", ")
        );
        let target = KitButton::new("import-target")
            .label(current)
            .dropdown_menu(move |menu, _, _| {
                let mut menu = names.iter().fold(menu, |menu, (id, name)| {
                    let (view, id) = (view.clone(), *id);
                    menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(destination == Destination::Vault(id))
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.set_destination(Destination::Vault(id), cx)
                                });
                            }),
                    )
                });
                if has_source_vaults {
                    let view = view.clone();
                    menu = menu.separator().item(
                        PopupMenuItem::new(source_label.clone())
                            .checked(destination == Destination::Source)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.set_destination(Destination::Source, cx)
                                });
                            }),
                    );
                }
                menu
            });

        let count = if previewing.include_trashed {
            preview.importable
        } else {
            preview.importable - preview.trashed
        };
        let mut body = v_flex()
            .gap(px(tokens::SPACE_3))
            .child(self.back_link(cx))
            .child(drop_zone(
                "import-file",
                IconName::Note,
                preview.file_name.clone(),
                format!(
                    "{} \u{b7} {} found",
                    size_text(preview.size),
                    plural(preview.found, "item", "items")
                ),
                cx,
            ))
            .child(
                v_flex()
                    .gap(px(6.))
                    .child(section_label("Into vault", cx))
                    .child(h_flex().child(target)),
            );
        if preview.trashed > 0 {
            body = body.child(
                FieldGroup::new().child(
                    SettingRow::new(
                        "import-trash",
                        IconName::Trash,
                        format!(
                            "Also import {} from the trash",
                            plural(preview.trashed, "item", "items")
                        ),
                    )
                    .description("They arrive in Kandado\u{2019}s trash, with 30 days left.")
                    .trailing(
                        Toggle::new("toggle-import-trash")
                            .checked(previewing.include_trashed)
                            .on_change(cx.listener(|this, on: &bool, _, cx| {
                                if let State::Preview(previewing) = &mut this.state {
                                    previewing.include_trashed = *on;
                                }
                                cx.notify();
                            })),
                    ),
                ),
            );
        }
        if preview.aliases > 0 {
            body = body.child(small(
                format!(
                    "{} can\u{2019}t move, because the address belongs to {}. Each is kept as a note with its address.",
                    plural(preview.aliases, "email alias", "email aliases"),
                    preview.source.name()
                ),
                cx,
            ));
        }
        for line in &preview.ignored {
            body = body.child(small(format!("Not kept: {line}."), cx));
        }
        if !preview.skipped.is_empty() {
            let n = preview.skipped.len();
            let mut banner = Banner::new(
                Tone::Warning,
                format!("{} can\u{2019}t be imported.", plural(n, "item", "items")),
            )
            .action(
                Button::new(
                    "import-review",
                    if previewing.review { "Hide" } else { "Review" },
                )
                .size(crate::components::ButtonSize::Sm)
                .on_click(cx.listener(|this, _, _, cx| {
                    if let State::Preview(previewing) = &mut this.state {
                        previewing.review = !previewing.review;
                    }
                    cx.notify();
                })),
            );
            banner = banner.icon(IconName::Alert);
            body = body.child(banner);
            if previewing.review {
                body = body.child(skipped_list(&preview.skipped, cx));
            }
        }
        if let Some(failure) = previewing.failure.clone() {
            body = body.child(error_line(failure, cx));
        }
        body.child(
            Button::new(
                "import-run",
                format!("Import {}", plural(count, "item", "items")),
            )
            .primary()
            .icon(IconName::Plus)
            .block()
            .disabled(count == 0)
            .on_click(cx.listener(|this, _, _, cx| this.start_import(cx))),
        )
    }

    fn set_destination(&mut self, destination: Destination, cx: &mut Context<Self>) {
        if let State::Preview(previewing) = &mut self.state {
            previewing.destination = destination;
        }
        cx.notify();
    }
}

fn read_error(error: &TransferError) -> SharedString {
    match error {
        TransferError::Import(ImportError::NotThisSource(name)) => format!(
            "That file is not an export from {name}. Choose another file, or another source."
        )
        .into(),
        TransferError::Import(ImportError::Unsupported(text)) => (*text).into(),
        TransferError::Import(ImportError::WrongPassphrase) => {
            "That is not the password for this export.".into()
        }
        TransferError::Import(ImportError::NeedsPassphrase) => {
            "This export needs its password.".into()
        }
        TransferError::TooLarge => "That file is too large to import.".into(),
        TransferError::Io(_) => {
            "Kandado could not read that file. Check that it still exists.".into()
        }
        TransferError::Cancelled => "The import was cancelled.".into(),
        TransferError::WrongPassword => "That is not your account password.".into(),
        TransferError::App(error) => explain(error),
    }
}

/// What was left out, one line each: what it was, and why.
fn skipped_list(skipped: &[kandado_app::Skipped], cx: &gpui_kit::App) -> gpui_kit::Div {
    let p = cx.palette();
    v_flex().gap(px(4.)).children(skipped.iter().map(|skipped| {
        div()
            .text_token(tokens::SMALL)
            .text_color(color(p.ink))
            .child(format!("{}: {}", skipped.label, skipped.reason))
    }))
}
impl Render for ImportPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        match &self.state {
            State::Sources => {
                let mut page = v_flex().gap(px(tokens::SPACE_3));
                if let Some(report) = self.services.app.last_import() {
                    page = page.child(last_import_banner(&report, cx));
                }
                page.child(self.source_rows(cx)).into_any_element()
            }
            State::File { source } => v_flex()
                .gap(px(tokens::SPACE_3))
                .child(self.back_link(cx))
                .child(
                    drop_zone(
                        "import-drop",
                        IconName::Note,
                        "Drop the file here",
                        format!("or choose it. {}", file_hint(*source)),
                        cx,
                    )
                    .on_drop(cx.listener({
                        let source = *source;
                        move |this, paths: &ExternalPaths, _, cx| {
                            if let Some(path) = paths.paths().first() {
                                this.load(source, path.clone(), None, cx);
                            }
                        }
                    })),
                )
                .child(
                    h_flex().child(
                        Button::new("import-choose", "Choose file\u{2026}")
                            .primary()
                            .icon(IconName::Plus)
                            .on_click(cx.listener(|this, _, _, cx| this.pick_file(cx))),
                    ),
                )
                .into_any_element(),
            State::Reading { name, .. } => v_flex()
                .gap(px(tokens::SPACE_3))
                .child(drop_zone(
                    "import-reading",
                    IconName::Note,
                    name.clone(),
                    "Reading\u{2026}",
                    cx,
                ))
                .into_any_element(),
            State::Passphrase { wrong, .. } => v_flex()
                .gap(px(tokens::SPACE_3))
                .child(self.back_link(cx))
                .child(paragraph(
                    "This export is encrypted. Enter the password it was made with.",
                    cx,
                ))
                .child(
                    TextField::new(&self.passphrase)
                        .label("Export password")
                        .icon(IconName::Lock)
                        .when(*wrong, |field| {
                            field.error("That is not the password for this export.")
                        }),
                )
                .child(
                    Button::new("import-open", "Open export")
                        .primary()
                        .block()
                        .on_click(cx.listener(|this, _, _, cx| this.open_with_passphrase(cx))),
                )
                .into_any_element(),
            State::Failed { message, .. } => v_flex()
                .gap(px(tokens::SPACE_3))
                .child(self.back_link(cx))
                .child(error_line(message.clone(), cx))
                .child(
                    h_flex().child(
                        Button::new("import-try-again", "Choose another file")
                            .on_click(cx.listener(|this, _, _, cx| this.pick_file(cx))),
                    ),
                )
                .into_any_element(),
            State::Preview(previewing) => self.preview_view(previewing, cx).into_any_element(),
            State::Importing { preview, progress } => {
                let (done, total) = progress.get();
                let total = if total == 0 {
                    preview.importable
                } else {
                    total
                };
                v_flex()
                    .gap(px(tokens::SPACE_3))
                    .child(drop_zone(
                        "import-running",
                        IconName::Note,
                        preview.file_name.clone(),
                        format!(
                            "{} \u{b7} {} found",
                            size_text(preview.size),
                            plural(preview.found, "item", "items")
                        ),
                        cx,
                    ))
                    .child(ProgressBar::new(if total == 0 {
                        0.
                    } else {
                        done as f32 / total as f32
                    }))
                    .child(
                        div()
                            .text_token(tokens::SMALL)
                            .text_color(color(p.ink_muted))
                            .child(format!("Importing {done} of {total}\u{2026}")),
                    )
                    .child(
                        h_flex().child(
                            Button::new("import-cancel", "Cancel")
                                .on_click(cx.listener(|this, _, _, _| this.cancel_import())),
                        ),
                    )
                    .into_any_element()
            }
            State::Done { report, review } => {
                let mut page = v_flex()
                    .gap(px(tokens::SPACE_3))
                    .child(last_import_banner(report, cx));
                if *review {
                    page = page.child(report_details(report, cx));
                }
                page.child(
                    h_flex()
                        .gap(px(tokens::SPACE_2))
                        .child(
                            Button::new("import-to-vault", "Go to vault")
                                .primary()
                                .on_click(cx.listener(|_, _, _, cx| cx.emit(Nav::Home))),
                        )
                        .child(
                            Button::new("import-another", "Import another file")
                                .variant(ButtonVariant::Secondary)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.state = State::Sources;
                                    cx.notify();
                                })),
                        )
                        .when(
                            !report.skipped.is_empty() || !report.ignored.is_empty(),
                            |row| {
                                row.child(
                                    Button::new(
                                        "import-done-review",
                                        if *review { "Hide details" } else { "Review" },
                                    )
                                    .variant(ButtonVariant::Ghost)
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            if let State::Done { review, .. } = &mut this.state {
                                                *review = !*review;
                                            }
                                            cx.notify();
                                        },
                                    )),
                                )
                            },
                        ),
                )
                .into_any_element()
            }
        }
    }
}

fn file_hint(source: Source) -> String {
    if source.files().is_empty() {
        "A CSV file with a header row.".to_owned()
    } else {
        format!("{}.", source.files())
    }
}

/// "Last import: 210 imported, 4 skipped" (D-47).
fn last_import_banner(report: &ImportReport, cx: &gpui_kit::App) -> gpui_kit::AnyElement {
    let _ = cx;
    let detail = match report.skipped.len() {
        0 => String::new(),
        n => format!(" {} could not be imported.", plural(n, "item", "items")),
    };
    Banner::new(
        Tone::Teal,
        format!("Last import: {}.{detail}", report_line(report)),
    )
    .icon(IconName::Check)
    .into_any_element()
}

/// What an import left out or did not keep, one line each.
fn report_details(report: &ImportReport, cx: &gpui_kit::App) -> gpui_kit::AnyElement {
    let p = cx.palette();
    let mut list = v_flex().gap(px(4.));
    if report.vaults_created > 0 {
        list = list.child(small(
            format!("{} made.", plural(report.vaults_created, "vault", "vaults")),
            cx,
        ));
    }
    if report.in_trash > 0 {
        list = list.child(small(
            format!(
                "{} went to the trash.",
                plural(report.in_trash, "item", "items")
            ),
            cx,
        ));
    }
    if report.trash_left_out > 0 {
        list = list.child(small(
            format!(
                "{} from the source\u{2019}s trash were left out.",
                plural(report.trash_left_out, "item", "items")
            ),
            cx,
        ));
    }
    if report.aliases_as_notes > 0 {
        list = list.child(small(
            format!(
                "{} kept as notes.",
                plural(report.aliases_as_notes, "email alias", "email aliases")
            ),
            cx,
        ));
    }
    for line in &report.ignored {
        list = list.child(small(format!("Not kept: {line}."), cx));
    }
    for skipped in &report.skipped {
        list = list.child(
            div()
                .text_token(tokens::SMALL)
                .text_color(color(p.ink))
                .child(format!("{}: {}", skipped.label, skipped.reason)),
        );
    }
    list.into_any_element()
}
