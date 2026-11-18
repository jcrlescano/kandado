//! D-16 Item history: the versions of an item on the left, a preview of the
//! chosen one on the right, and "Restore this version", which saves it as the
//! newest version. States: the current version chosen (nothing to restore),
//! an earlier version chosen, a secret revealed in the preview.

use std::collections::HashMap;

use gpui_kit::{
    Context, EventEmitter, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Window,
    component::{h_flex, scroll::ScrollableElement as _, v_flex},
    div, px,
};
use kandado_app::vault::{ItemDetail, Version};
use uuid::Uuid;

use super::{
    super::{Services, date, explain},
    detail::{icon_for, note_box},
};
use crate::{
    assets::IconName,
    components::{
        Button, FieldGroup, Icon, IconButton, IconButtonVariant, SecretField, radio, section_label,
    },
    theme::{KandadoTheme, TextStyles as _, color, focus_ring, tokens},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryEvent {
    Back,
    /// A version was restored; the item changed.
    Restored,
    Failed(SharedString),
}

pub struct HistoryView {
    services: Services,
    id: Uuid,
    title: SharedString,
    versions: Vec<Version>,
    /// The revision being previewed.
    chosen: i64,
    preview: Option<ItemDetail>,
    revealed: HashMap<usize, SharedString>,
}

impl EventEmitter<HistoryEvent> for HistoryView {}

impl HistoryView {
    pub fn new(services: Services, id: Uuid, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let versions = services.app.history(id).unwrap_or_default();
        let chosen = versions.first().map_or(0, |version| version.revision);
        let mut view = Self {
            title: services
                .app
                .entry(id)
                .map(|entry| entry.title)
                .unwrap_or_default()
                .into(),
            services,
            id,
            versions,
            chosen,
            preview: None,
            revealed: HashMap::new(),
        };
        view.choose(chosen, cx);
        view.fetch_server_versions(cx);
        cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() && !this.revealed.is_empty() {
                this.revealed.clear();
                cx.notify();
            }
        })
        .detach();
        view
    }

    pub fn versions(&self) -> &[Version] {
        &self.versions
    }

    pub fn choose(&mut self, revision: i64, cx: &mut Context<Self>) {
        self.chosen = revision;
        self.revealed.clear();
        self.preview = self.services.app.version_detail(self.id, revision).ok();
        cx.notify();
    }

    fn toggle_reveal(&mut self, field: usize, cx: &mut Context<Self>) {
        if self.revealed.remove(&field).is_none() {
            match self.services.app.reveal(self.id, Some(self.chosen), field) {
                Ok(secret) => {
                    self.revealed
                        .insert(field, secret.as_str().to_owned().into());
                }
                Err(error) => cx.emit(HistoryEvent::Failed(explain(&error))),
            }
        }
        cx.notify();
    }

    pub fn restore(&mut self, cx: &mut Context<Self>) {
        match self.services.app.restore_version(self.id, self.chosen) {
            Ok(()) => cx.emit(HistoryEvent::Restored),
            Err(error) => cx.emit(HistoryEvent::Failed(explain(&error))),
        }
    }

    fn chosen_is_current(&self) -> bool {
        self.versions
            .iter()
            .any(|version| version.current && version.revision == self.chosen)
    }
}

impl Render for HistoryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let mut list = FieldGroup::new();
        for version in &self.versions {
            let revision = version.revision;
            let on = revision == self.chosen;
            list = list.child(
                h_flex()
                    .id(("version", revision as u64))
                    .items_center()
                    .gap(px(tokens::SPACE_3))
                    .min_h(px(56.))
                    .px(px(tokens::SPACE_4))
                    .py(px(tokens::SPACE_2))
                    .relative()
                    .group(crate::components::GROUP_ROW)
                    .child(crate::components::group_row_hover(cx))
                    .cursor_pointer()
                    .tab_index(0)
                    .focus_visible(|row| row.shadow(focus_ring(p, p.raised)))
                    .on_click(cx.listener(move |this, _, _, cx| this.choose(revision, cx)))
                    .child(
                        Icon::new(IconName::Clock)
                            .size(px(20.))
                            .color(color(p.ink_2)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_token(tokens::BODY)
                                    .text_color(color(p.ink))
                                    .child(date(version.saved_at)),
                            )
                            .child(
                                div()
                                    .text_token(tokens::SMALL)
                                    .text_color(color(p.ink_2))
                                    .child(if version.current {
                                        format!("Current version · {}", version.summary)
                                    } else {
                                        version.summary.clone()
                                    }),
                            ),
                    )
                    .child(radio(on, cx)),
            );
        }

        let mut preview = v_flex().flex_1().min_w_0().gap(px(tokens::SPACE_3));
        if let Some(detail) = self.preview.clone() {
            let saved_at = self
                .versions
                .iter()
                .find(|version| version.revision == self.chosen)
                .map_or(detail.updated_at, |version| version.saved_at);
            preview = preview.child(section_label(format!("{} version", date(saved_at)), cx));
            for section in &detail.sections {
                let mut group = FieldGroup::new();
                for field in &section.fields {
                    let index = field.index;
                    let mut row = SecretField::new(("version-field", index), field.label.clone())
                        .mono(field.mono)
                        .link(field.link)
                        .copyable(false);
                    if let Some(icon) = icon_for(field.icon) {
                        row = row.icon(icon);
                    }
                    row = match (&field.value, field.totp) {
                        (Some(value), _) => row.plain(value.clone()),
                        // The setup key itself is not shown; say that there is one.
                        (None, true) => row.plain("Set up"),
                        (None, false) => {
                            row.revealed(self.revealed.get(&index).cloned()).on_reveal(
                                cx.listener(move |this, _, _, cx| this.toggle_reveal(index, cx)),
                            )
                        }
                    };
                    group = group.child(row);
                }
                preview = preview.child(group);
            }
            if !detail.note.trim().is_empty() {
                preview = preview.child(note_box(&detail.note, cx));
            }
            preview = preview.child(
                h_flex().child(
                    Button::new("restore-version", "Restore this version")
                        .primary()
                        .disabled(self.chosen_is_current())
                        .on_click(cx.listener(|this, _, _, cx| this.restore(cx))),
                ),
            );
        }

        div()
            .id("history-pane")
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .child(
                v_flex()
                    .px(px(tokens::SPACE_6))
                    .py(px(tokens::SPACE_5))
                    .gap(px(tokens::SPACE_5))
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(tokens::SPACE_4))
                            .child(
                                IconButton::new("history-back", IconName::Chevron, "Back to item")
                                    .variant(IconButtonVariant::Secondary)
                                    .flipped()
                                    .on_click(
                                        cx.listener(|_, _, _, cx| cx.emit(HistoryEvent::Back)),
                                    ),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .font_family(tokens::FONT_SANS)
                                    .text_size(px(24.))
                                    .line_height(px(30.))
                                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                    .text_color(color(p.ink))
                                    .child(format!("{} history", self.title)),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_start()
                            .gap(px(tokens::SPACE_5))
                            .child(v_flex().flex_1().min_w_0().child(list))
                            .child(preview),
                    ),
            )
    }
}

impl HistoryView {
    /// Brings in the versions the server has that this computer lacks (made
    /// on another computer), then redraws the list. Offline, the list is
    /// what this computer has.
    fn fetch_server_versions(&mut self, cx: &mut Context<Self>) {
        let (services, id) = (self.services.clone(), self.id);
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let added = services
                .tasks
                .run(async move { app.fetch_history(id).await })
                .await;
            if matches!(added, Ok(count) if count > 0) {
                let _ = this.update(cx, |this, cx| {
                    this.versions = this.services.app.history(this.id).unwrap_or_default();
                    cx.notify();
                });
            }
        })
        .detach();
    }
}
