//! What the shell draws itself: the sidebar and its 64 px rail (D-17), the
//! list pane with its filters, empty states (D-06, D-41 empty) and bulk bar
//! (D-15), the search results (D-14), and the layer of dialogs: create menu
//! (D-18), move (D-34), delete forever (D-25), empty trash (D-42), discard
//! (D-26), vault (D-27) and shortcuts (D-51).

use gpui_kit::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement as _, Styled, Window,
    component::{h_flex, scroll::ScrollableElement as _, v_flex},
    div, list,
    prelude::FluentBuilder as _,
    px,
};
use kandado_model::ItemKind;
use uuid::Uuid;

use super::{
    LIST_WIDTH, Overlay, Pane, Place, RAIL_WIDTH, Row, SIDEBAR_WIDTH, VaultShell, hue_of,
    vault_icon,
};
use crate::{
    assets::IconName,
    components::{
        Button, ButtonSize, ButtonVariant, Dialog, EmptyState, FilterChip, Hue, Icon, IconButton,
        ItemIcon, ItemRow, NavItem, Tone, check, kbd, radio, section_label, window_layer,
    },
    theme::{DISPLAY_FACE, KandadoTheme, TextStyles as _, color, focus_ring, shadow, tokens},
    views::{
        Nav, heading,
        items::detail::letter,
        paragraph,
        settings::rows::{SettingRow, group_label},
        system::shortcuts_dialog,
    },
};

/// The kinds the create menu offers in phase 2. Alias waits for phase 9.
pub const CREATE_KINDS: [(ItemKind, &str); 5] = [
    (ItemKind::Login, "Login"),
    (ItemKind::Card, "Card"),
    (ItemKind::Note, "Note"),
    (ItemKind::Identity, "Identity"),
    (ItemKind::Password, "Password"),
];

/// The list's filter chips.
const FILTERS: [(Option<ItemKind>, &str); 5] = [
    (None, "All"),
    (Some(ItemKind::Login), "Logins"),
    (Some(ItemKind::Card), "Cards"),
    (Some(ItemKind::Note), "Notes"),
    (Some(ItemKind::Identity), "Identities"),
];

fn count(n: usize, cx: &gpui_kit::App) -> impl IntoElement {
    div()
        .text_size(px(12.))
        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
        .text_color(color(cx.palette().ink_muted))
        .child(n.to_string())
}

/// The coloured dot that stands for a vault in the sidebar.
fn vault_dot(hue: Hue, icon: IconName, cx: &gpui_kit::App) -> impl IntoElement {
    let (ink, ground) = hue.colors(cx.palette());
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(24.))
        .rounded(px(tokens::RADIUS_PILL))
        .bg(ground)
        .child(Icon::new(icon).size(px(14.)).color(ink))
}

impl VaultShell {
    pub(super) fn sidebar(&self, rail: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let nav =
            |id: &'static str, icon: IconName, label: &str, place: Place, n: Option<usize>| {
                NavItem::new(id, icon, label.to_owned())
                    .rail(rail)
                    .selected(self.place == place)
                    .when_some(n, |item, n| item.trailing(count(n, cx)))
                    .on_click(cx.listener(move |this, _, window, cx| this.go(place, window, cx)))
            };
        let heading = |text: &'static str| {
            div()
                .mt(px(tokens::SPACE_4))
                .mb(px(6.))
                .px(px(tokens::SPACE_2))
                .when(rail, |heading| heading.invisible().h(px(1.)))
                .child(section_label(text, cx))
        };

        // Three parts: Create on top and the account at the bottom stay put;
        // the vaults and tools between them scroll when there are many vaults.
        let side = v_flex()
            .id("sidebar")
            .w(if rail { RAIL_WIDTH } else { SIDEBAR_WIDTH })
            .flex_none()
            .h_full()
            .gap(px(2.))
            .p(px(tokens::SPACE_3))
            .when(rail, |side| side.items_center())
            .bg(color(p.surface_100))
            .border_r_1()
            .border_color(color(p.line))
            .child(if rail {
                IconButton::new("create-item", IconName::Plus, "Create item")
                    .variant(crate::components::IconButtonVariant::Primary)
                    .on_click(cx.listener(|this, _, _, cx| this.open_create_menu(cx)))
                    .into_any_element()
            } else {
                Button::new("create-item", "Create item")
                    .primary()
                    .icon(IconName::Plus)
                    .block()
                    .on_click(cx.listener(|this, _, _, cx| this.open_create_menu(cx)))
                    .into_any_element()
            });
        let mut list = v_flex()
            .w_full()
            .gap(px(2.))
            .when(rail, |list| list.items_center())
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .child(div().flex_1().child(heading("Vaults")))
                    .when(!rail, |row| {
                        row.child(
                            div().mt(px(tokens::SPACE_3)).child(
                                IconButton::new("new-vault", IconName::Plus, "Create vault")
                                    .size(ButtonSize::Sm)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_vault_dialog(None, window, cx)
                                    })),
                            ),
                        )
                    }),
            )
            .child(nav(
                "nav-all",
                IconName::Grid,
                "All vaults",
                Place::AllVaults,
                Some(self.counts.0),
            ));
        for (index, vault) in self.vaults.iter().enumerate() {
            let place = Place::Vault(vault.id);
            let id = vault.id;
            let selected = self.place == place;
            let dot = vault_dot(hue_of(&vault.meta.colour), vault_icon(&vault.meta.icon), cx);
            list = list.child(
                h_flex()
                    .id(("nav-vault", index))
                    .group("vault-row")
                    .items_center()
                    .gap(px(10.))
                    .h(px(40.))
                    .when(rail, |row| row.w(px(40.)).justify_center())
                    .when(!rail, |row| row.px(px(10.)))
                    .rounded(px(tokens::RADIUS_MD))
                    .cursor_pointer()
                    .tab_index(0)
                    .focus_visible(|row| row.shadow(focus_ring(p, p.surface_100)))
                    .map(|row| {
                        if selected {
                            row.bg(color(p.brass_soft))
                        } else {
                            row.hover(|row| row.bg(color(p.surface_200)))
                        }
                    })
                    .when(rail, |row| {
                        let name: SharedString = vault.meta.name.clone().into();
                        row.tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(name.clone())
                                .build(window, cx)
                        })
                    })
                    .on_click(cx.listener(move |this, _, window, cx| this.go(place, window, cx)))
                    .child(dot)
                    .when(!rail, |row| {
                        row.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_token(tokens::BODY_STRONG)
                                .text_color(color(p.ink))
                                .child(vault.meta.name.clone()),
                        )
                        .child(
                            div()
                                .invisible()
                                .group_hover("vault-row", |edit| edit.visible())
                                .when(selected, |edit| edit.visible())
                                .child(
                                    IconButton::new(
                                        ("edit-vault", index),
                                        IconName::Settings,
                                        "Edit vault",
                                    )
                                    .size(ButtonSize::Sm)
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            cx.stop_propagation();
                                            this.open_vault_dialog(Some(id), window, cx)
                                        },
                                    )),
                                ),
                        )
                        .child(count(vault.item_count, cx))
                    }),
            );
        }
        let list = list
            .child(heading("Tools"))
            .child(nav(
                "nav-monitor",
                IconName::Shield,
                "Monitor",
                Place::Monitor,
                None,
            ))
            .child(nav(
                "nav-trash",
                IconName::Trash,
                "Trash",
                Place::Trash,
                (self.counts.1 > 0).then_some(self.counts.1),
            ));
        side.child(
            div()
                .id("sidebar-scroll")
                .flex_1()
                .min_h_0()
                .w_full()
                .overflow_y_scrollbar()
                .child(list),
        )
        .child(
            h_flex()
                .items_center()
                .gap(px(10.))
                .pt(px(10.))
                .border_t_1()
                .border_color(color(p.line))
                .when(rail, |foot| foot.justify_center())
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .size(px(32.))
                        .rounded(px(tokens::RADIUS_PILL))
                        .bg(color(p.teal_soft))
                        .text_color(color(p.teal))
                        .font_family(DISPLAY_FACE)
                        .font_weight(gpui_kit::FontWeight::BOLD)
                        .child(
                            self.email
                                .chars()
                                .next()
                                .map(|c| c.to_uppercase().collect::<String>())
                                .unwrap_or_default(),
                        ),
                )
                .when(!rail, |foot| {
                    foot.child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(14.))
                                    .text_color(color(p.ink))
                                    .child(self.email.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(color(p.ink_muted))
                                    .child(crate::views::sync_text(
                                        &self.services.app.sync_status(),
                                        kandado_app::vault::now(),
                                    )),
                            ),
                    )
                    .child(
                        IconButton::new("open-settings", IconName::Settings, "Settings")
                            .size(ButtonSize::Sm)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.ask_settings(window, cx)),
                            ),
                    )
                }),
        )
    }

    fn row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let Some(row) = self.rows.get(index) else {
            return div().into_any_element();
        };
        let entry = match row {
            Row::Label(label) => {
                return div()
                    .pt(px(10.))
                    .pb(px(4.))
                    .px(px(tokens::SPACE_2))
                    .child(section_label(*label, cx))
                    .into_any_element();
            }
            Row::Item(entry) => entry.clone(),
        };
        let id = entry.id;
        let selecting = !self.checked.is_empty();
        let ticked = self.checked.contains(&id);
        let subtitle = match (self.place, entry.trashed_at) {
            (Place::Trash, Some(trashed_at)) => {
                let days = kandado_vault_days_left(trashed_at);
                format!("{days} {} left", if days == 1 { "day" } else { "days" })
            }
            _ => entry.subtitle.clone(),
        };
        let mut item = ItemRow::new(("item", index), entry.kind, entry.title.clone())
            .selected(self.selected == Some(id) || ticked)
            .favorite(entry.favourite)
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.click_row(id, event.modifiers(), window, cx)
            }));
        if !subtitle.is_empty() {
            item = item.subtitle(subtitle);
        }
        if let Some(letter) = letter(entry.kind, &entry.title) {
            item = item.letter(letter);
        }
        if entry.has_password && !selecting && self.place != Place::Trash {
            item = item.on_copy(cx.listener(move |this, _, _, cx| this.copy_password_of(id, cx)));
        }
        h_flex()
            .items_center()
            .gap(px(tokens::SPACE_2))
            .w_full()
            .when(selecting, |row| {
                row.child(
                    div()
                        .id(("tick", index))
                        .cursor_pointer()
                        .p(px(2.))
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_checked(id, cx)))
                        .child(check(ticked, cx)),
                )
            })
            .child(div().flex_1().min_w_0().child(item))
            .text_color(color(p.ink))
            .into_any_element()
    }

    pub(super) fn list_pane(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let trash = self.place == Place::Trash;
        let entity = cx.entity();
        let head = h_flex()
            .items_center()
            .gap(px(tokens::SPACE_2))
            .pt(px(14.))
            .pb(px(10.))
            .pl(px(tokens::SPACE_4))
            .pr(px(tokens::SPACE_3))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(DISPLAY_FACE)
                    .text_size(px(18.))
                    .line_height(px(24.))
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .text_color(color(p.ink))
                    .child(self.place_title()),
            )
            .when(trash && self.counts.1 > 0, |head| {
                head.child(
                    Button::new("empty-trash", "Empty trash")
                        .variant(ButtonVariant::DangerQuiet)
                        .size(ButtonSize::Sm)
                        .on_click(cx.listener(|this, _, _, cx| this.ask_empty_trash(cx))),
                )
            })
            .when(
                !trash && self.place != Place::Monitor && !self.rows.is_empty(),
                |head| {
                    head.child(
                        Button::new("select-all", "Select all")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .on_click(cx.listener(|this, _, _, cx| this.check_all(cx))),
                    )
                },
            );
        let filters = h_flex()
            .flex_wrap()
            .gap(px(6.))
            .px(px(tokens::SPACE_4))
            .pb(px(10.))
            .children(
                FILTERS
                    .into_iter()
                    .enumerate()
                    .map(|(index, (kind, label))| {
                        FilterChip::new(("filter", index), label)
                            .on(self.kind == kind)
                            .on_click(cx.listener(move |this, _, _, cx| this.set_kind(kind, cx)))
                    }),
            );
        let bulk = (!self.checked.is_empty()).then(|| {
            h_flex()
                .items_center()
                .mx(px(tokens::SPACE_2))
                .mb(px(tokens::SPACE_2))
                .px(px(tokens::SPACE_3))
                .h(px(40.))
                .rounded(px(tokens::RADIUS_MD))
                .bg(color(p.brass_soft))
                .text_token(tokens::BODY_STRONG)
                .text_color(color(p.ink))
                .child(
                    div()
                        .flex_1()
                        .child(format!("{} selected", self.checked.len())),
                )
                .child(
                    IconButton::new("clear-selection", IconName::X, "Clear selection")
                        .size(ButtonSize::Sm)
                        .on_click(cx.listener(|this, _, _, cx| this.clear_checked(cx))),
                )
        });

        let body: AnyElement = if self.place == Place::Monitor {
            EmptyState::new(IconName::Shield, "Monitor is coming")
                .hue(Hue::Teal)
                .body("Weak, reused and breached passwords show here in a later release.")
                .into_any_element()
        } else if self.rows.is_empty() {
            let (icon, hue, title, body) = if trash {
                (
                    IconName::Trash,
                    Hue::Sky,
                    "Trash is empty".to_owned(),
                    "Items you move to the trash stay here for 30 days.".to_owned(),
                )
            } else if self.kind.is_some() {
                (
                    IconName::Search,
                    Hue::Sky,
                    "Nothing of this kind".to_owned(),
                    "Choose All to see every item.".to_owned(),
                )
            } else {
                (
                    IconName::Search,
                    Hue::Sky,
                    "No items".to_owned(),
                    format!("Items you add to {} show here.", self.place_title()),
                )
            };
            EmptyState::new(icon, title)
                .hue(hue)
                .body(body)
                .into_any_element()
        } else {
            list(self.list.clone(), move |index, _, cx| {
                entity.update(cx, |this, cx| this.row(index, cx))
            })
            .size_full()
            .into_any_element()
        };

        v_flex()
            .id("item-list")
            .w(LIST_WIDTH)
            .flex_none()
            .h_full()
            .bg(color(p.surface_100))
            .border_r_1()
            .border_color(color(p.line))
            .child(head)
            .when(!trash && self.place != Place::Monitor, |pane| {
                pane.child(filters)
            })
            .when_some(bulk, |pane, bulk| pane.child(bulk))
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .px(px(tokens::SPACE_2))
                    .pb(px(tokens::SPACE_2))
                    .child(body),
            )
    }

    pub(super) fn empty_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        // D-15: what the ticked items are, and what can be done with them.
        if !self.checked.is_empty() {
            let names: Vec<String> = self
                .checked
                .iter()
                .filter_map(|id| self.services.app.entry(*id))
                .map(|entry| entry.title)
                .collect();
            let summary = match names.as_slice() {
                [] => String::new(),
                [one] => format!("{one}."),
                [first, second] => format!("{first} and {second}."),
                [first, second, third] => format!("{first}, {second} and {third}."),
                [first, second, rest @ ..] => {
                    format!("{first}, {second} and {} more.", rest.len())
                }
            };
            let trash = self.place == Place::Trash;
            let actions = if trash {
                h_flex()
                    .gap(px(tokens::SPACE_2))
                    .mt(px(tokens::SPACE_3))
                    .child(
                        Button::new("bulk-restore", "Restore")
                            .icon(IconName::Refresh)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let ids = this.checked.clone();
                                this.restore(ids, cx)
                            })),
                    )
                    .child(
                        Button::new("bulk-delete", "Delete forever")
                            .variant(ButtonVariant::DangerQuiet)
                            .icon(IconName::Trash)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let ids = this.checked.clone();
                                this.ask_delete_forever(ids, cx)
                            })),
                    )
            } else {
                h_flex()
                    .gap(px(tokens::SPACE_2))
                    .mt(px(tokens::SPACE_3))
                    .child(
                        Button::new("bulk-move", "Move to vault")
                            .icon(IconName::Vault)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let ids = this.checked.clone();
                                this.ask_move(ids, cx)
                            })),
                    )
                    .child(
                        Button::new("bulk-favourite", "Favourite")
                            .icon(IconName::Star)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let ids = this.checked.clone();
                                this.favourite(ids, cx)
                            })),
                    )
                    .child(
                        Button::new("bulk-trash", "Move to trash")
                            .variant(ButtonVariant::DangerQuiet)
                            .icon(IconName::Trash)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let ids = this.checked.clone();
                                this.trash(ids, window, cx)
                            })),
                    )
            };
            let n = self.checked.len();
            return EmptyState::new(
                IconName::Check,
                format!("{n} {} selected", if n == 1 { "item" } else { "items" }),
            )
            .body(summary)
            .child(actions)
            .into_any_element();
        }
        if self.place == Place::Trash || self.place == Place::Monitor {
            return EmptyState::new(IconName::Trash, "Nothing chosen")
                .hue(Hue::Sky)
                .body("Choose an item to restore it or delete it forever.")
                .into_any_element();
        }
        // D-05: the first time, an empty vault offers to bring passwords in.
        if matches!(self.place, Place::AllVaults | Place::Vault(_))
            && self.services.app.should_offer_import()
        {
            return self.first_run_import(cx);
        }
        // D-06: a vault with nothing in it.
        if self.counts.0 == 0 || (matches!(self.place, Place::Vault(_)) && self.rows.is_empty()) {
            return EmptyState::new(
                IconName::Vault,
                format!("Nothing in {} yet", self.place_title()),
            )
            .body("Create your first item, or bring them from another password manager.")
            .child(
                h_flex()
                    .flex_wrap()
                    .justify_center()
                    .gap(px(tokens::SPACE_2))
                    .mt(px(tokens::SPACE_3))
                    .child(
                        Button::new("empty-login", "Create login")
                            .primary()
                            .icon(IconName::Key)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create(ItemKind::Login, window, cx)
                            })),
                    )
                    .child(
                        Button::new("empty-card", "Add card")
                            .icon(IconName::Card)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create(ItemKind::Card, window, cx)
                            })),
                    )
                    .child(
                        Button::new("empty-note", "Write note")
                            .icon(IconName::Note)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create(ItemKind::Note, window, cx)
                            })),
                    )
                    .child(
                        Button::new("empty-import", "Import")
                            .icon(IconName::Refresh)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(Nav::Import(None)))),
                    ),
            )
            .into_any_element();
        }
        EmptyState::new(IconName::Key, "Choose an item")
            .hue(Hue::Teal)
            .body("Pick something from the list, or press Ctrl N to create a login.")
            .into_any_element()
    }

    /// D-14: results grouped by kind under the search field.
    pub(super) fn search_layer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.search_open {
            return None;
        }
        let p = cx.palette();
        let mut panel = v_flex().gap(px(2.)).p(px(tokens::SPACE_2));
        if self.results.is_empty() {
            let query = self.search.read(cx).value().trim().to_owned();
            panel = panel.child(
                v_flex()
                    .items_center()
                    .gap(px(tokens::SPACE_2))
                    .p(px(tokens::SPACE_4))
                    .child(
                        div()
                            .text_token(tokens::BODY)
                            .text_color(color(p.ink_muted))
                            .child(format!("Nothing matches \u{201c}{query}\u{201d}.")),
                    )
                    .child(
                        Button::new("create-from-search", format!("Create login for {query}"))
                            .icon(IconName::Plus)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create_from_search(window, cx)
                            })),
                    ),
            );
        } else {
            let groups = [
                (ItemKind::Login, "Logins"),
                (ItemKind::Card, "Cards"),
                (ItemKind::Note, "Notes"),
                (ItemKind::Identity, "Identities"),
                (ItemKind::Password, "Passwords"),
                (ItemKind::Alias, "Aliases"),
                (ItemKind::Passkey, "Passkeys"),
            ];
            for (kind, label) in groups {
                let hits: Vec<(usize, &kandado_app::Entry)> = self
                    .results
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| entry.kind == kind)
                    .collect();
                if hits.is_empty() {
                    continue;
                }
                panel = panel.child(
                    div()
                        .pt(px(tokens::SPACE_2))
                        .pb(px(4.))
                        .px(px(tokens::SPACE_2))
                        .child(section_label(label, cx)),
                );
                for (index, entry) in hits {
                    let (vault, _) = self.vault_name(entry.vault_id);
                    let subtitle = if entry.subtitle.is_empty() {
                        vault.to_string()
                    } else {
                        format!("{} · {vault}", entry.subtitle)
                    };
                    let mut row = ItemRow::new(("result", index), entry.kind, entry.title.clone())
                        .subtitle(subtitle)
                        .dense(true)
                        .selected(index == self.result_index)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_result(index, window, cx)
                        }));
                    if let Some(letter) = letter(entry.kind, &entry.title) {
                        row = row.letter(letter);
                    }
                    panel = panel.child(row);
                }
            }
        }
        let hint = |key: &'static str, label: &'static str| {
            h_flex()
                .items_center()
                .gap(px(6.))
                .child(kbd(key, cx))
                .child(label)
        };
        let panel = v_flex()
            .id("search-results")
            .w(px(520.))
            .max_h(px(480.))
            .rounded(px(tokens::RADIUS_LG))
            .border_1()
            .border_color(color(p.line))
            .bg(color(p.surface_100))
            .shadow(shadow(tokens::SHADOW_POP, cx.theme_kind()))
            .occlude()
            .child(
                div()
                    .id("search-results-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(panel),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .gap(px(tokens::SPACE_4))
                    .px(px(tokens::SPACE_4))
                    .py(px(tokens::SPACE_2))
                    .border_t_1()
                    .border_color(color(p.line))
                    .text_token(tokens::SMALL)
                    .text_color(color(p.ink_muted))
                    .child(hint("↑↓", "Move"))
                    .child(hint("Enter", "Open"))
                    .child(hint("Ctrl C", "Copy password"))
                    .child(hint("Esc", "Close")),
            );
        Some(
            h_flex()
                .absolute()
                .top(px(6.))
                .left_0()
                .right_0()
                .justify_center()
                .child(panel)
                .into_any_element(),
        )
    }

    fn create_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let menu = v_flex()
            .id("create-menu")
            .absolute()
            .top(px(60.))
            .left(px(tokens::SPACE_3))
            .w(px(260.))
            .p(px(6.))
            .rounded(px(tokens::RADIUS_LG))
            .border_1()
            .border_color(color(p.line))
            .bg(color(p.surface_100))
            .shadow(shadow(tokens::SHADOW_POP, cx.theme_kind()))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, window, cx| this.close_dialog(window, cx)))
            .children(CREATE_KINDS.into_iter().map(|(kind, label)| {
                h_flex()
                    .id(SharedString::from(format!("create-{}", kind.as_str())))
                    .items_center()
                    .gap(px(tokens::SPACE_3))
                    .h(px(44.))
                    .px(px(tokens::SPACE_2))
                    .rounded(px(tokens::RADIUS_MD))
                    .cursor_pointer()
                    .tab_index(0)
                    .hover(|row| row.bg(color(p.surface_200)))
                    .focus_visible(|row| row.shadow(focus_ring(p, p.surface_100)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.overlay = Overlay::None;
                        this.create(kind, window, cx)
                    }))
                    .child(ItemIcon::new(kind).size(px(28.)))
                    .child(
                        div()
                            .flex_1()
                            .text_token(tokens::BODY)
                            .text_color(color(p.ink))
                            .child(label),
                    )
                    .when(kind == ItemKind::Login, |row| row.child(kbd("Ctrl N", cx)))
            }))
            .child(div().h(px(1.)).my(px(4.)).bg(color(p.line)))
            .child(
                h_flex()
                    .id("create-import")
                    .items_center()
                    .gap(px(tokens::SPACE_3))
                    .h(px(44.))
                    .px(px(tokens::SPACE_2))
                    .rounded(px(tokens::RADIUS_MD))
                    .cursor_pointer()
                    .tab_index(0)
                    .hover(|row| row.bg(color(p.surface_200)))
                    .focus_visible(|row| row.shadow(focus_ring(p, p.surface_100)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.overlay = Overlay::None;
                        cx.emit(Nav::Import(None));
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(28.))
                            .child(
                                Icon::new(IconName::Refresh)
                                    .size(px(18.))
                                    .color(color(p.ink_muted)),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_token(tokens::BODY)
                            .text_color(color(p.ink))
                            .child("Import{2026}"),
                    ),
            );
        menu.into_any_element()
    }

    fn move_dialog(&self, ids: &[Uuid], target: Uuid, cx: &mut Context<Self>) -> Dialog {
        let p = cx.palette();
        let n = ids.len();
        let from: Vec<Uuid> = ids
            .iter()
            .filter_map(|id| self.services.app.entry(*id))
            .map(|entry| entry.vault_id)
            .collect();
        let mut dialog = Dialog::new(if n == 1 {
            "Move item".to_owned()
        } else {
            format!("Move {n} items")
        });
        let mut rows = crate::components::FieldGroup::new();
        for (index, vault) in self.vaults.iter().enumerate() {
            let id = vault.id;
            let current = from.iter().all(|from| *from == id);
            rows = rows.child(
                h_flex()
                    .id(("move-to", index))
                    .items_center()
                    .gap(px(tokens::SPACE_3))
                    .min_h(px(52.))
                    .px(px(tokens::SPACE_4))
                    .relative()
                    .group(crate::components::GROUP_ROW)
                    .child(crate::components::group_row_hover(cx))
                    .cursor_pointer()
                    .tab_index(0)
                    .focus_visible(|row| row.shadow(focus_ring(p, p.surface_200)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Overlay::Move { target, .. } = &mut this.overlay {
                            *target = id;
                        }
                        cx.notify();
                    }))
                    .child(vault_dot(
                        hue_of(&vault.meta.colour),
                        vault_icon(&vault.meta.icon),
                        cx,
                    ))
                    .child(
                        v_flex()
                            .flex_1()
                            .child(
                                div()
                                    .text_token(tokens::BODY)
                                    .text_color(color(p.ink))
                                    .child(vault.meta.name.clone()),
                            )
                            .when(current, |text| {
                                text.child(
                                    div()
                                        .text_token(tokens::SMALL)
                                        .text_color(color(p.ink_muted))
                                        .child("Current vault"),
                                )
                            }),
                    )
                    .child(radio(id == target, cx)),
            );
        }
        dialog = dialog.child(rows);
        let unchanged = from.iter().all(|from| *from == target);
        dialog
            .action(
                Button::new("move-cancel", "Cancel")
                    .on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
            )
            .action(
                Button::new("move-confirm", "Move")
                    .primary()
                    .disabled(unchanged)
                    .on_click(cx.listener(|this, _, window, cx| this.confirm_move(window, cx))),
            )
    }

    fn confirm(
        &self,
        icon: Option<IconName>,
        title: String,
        body: String,
        cancel: &'static str,
        confirm: &'static str,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let mut dialog = Dialog::new(title).body(body);
        if let Some(icon) = icon {
            dialog = dialog.icon(icon, Tone::Danger);
        }
        dialog
            .action(
                Button::new("dialog-cancel", cancel)
                    .on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
            )
            .action(
                Button::new("dialog-confirm", confirm)
                    .variant(ButtonVariant::Danger)
                    .on_click(cx.listener(|this, _, window, cx| this.confirm_dialog(window, cx))),
            )
    }

    pub(super) fn overlay_layer(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = match &self.overlay {
            Overlay::None => return None,
            Overlay::CreateMenu => return Some(self.create_menu(cx)),
            Overlay::Vault(dialog) => return Some(dialog.clone().into_any_element()),
            Overlay::Shortcuts => shortcuts_dialog(
                cx.listener(|this, _, window, cx| this.close_dialog(window, cx)),
                cx,
            ),
            Overlay::Move { ids, target } => self.move_dialog(&ids.clone(), *target, cx),
            Overlay::DeleteForever(ids) => {
                let title = match ids.as_slice() {
                    [id] => self
                        .services
                        .app
                        .entry(*id)
                        .map(|entry| format!("Delete {} forever?", entry.title))
                        .unwrap_or_else(|| "Delete this item forever?".to_owned()),
                    ids => format!("Delete {} items forever?", ids.len()),
                };
                self.confirm(
                    Some(IconName::Trash),
                    title,
                    "It\u{2019}s removed from this computer and can\u{2019}t be undone.".to_owned(),
                    "Cancel",
                    "Delete forever",
                    cx,
                )
            }
            Overlay::EmptyTrash => {
                let n = self.counts.1;
                self.confirm(
                    Some(IconName::Trash),
                    format!(
                        "Delete {n} {} forever?",
                        if n == 1 { "item" } else { "items" }
                    ),
                    "They\u{2019}re removed from this computer. This can\u{2019}t be undone."
                        .to_owned(),
                    "Cancel",
                    "Empty trash",
                    cx,
                )
            }
            Overlay::Discard(_) => {
                let name = match &self.pane {
                    Pane::Form(form) => form.read(cx).display_name(cx),
                    _ => "this item".to_owned(),
                };
                self.confirm(
                    None,
                    "Discard changes?".to_owned(),
                    format!("Your edits to {name} will be lost."),
                    "Keep editing",
                    "Discard",
                    cx,
                )
            }
        };
        Some(window_layer(dialog, window).into_any_element())
    }
}

/// Whole days an item has left in the trash.
fn kandado_vault_days_left(trashed_at: i64) -> i64 {
    kandado_app::vault::days_left(trashed_at)
}

impl VaultShell {
    /// D-05 First-run import: the sources, and "Skip for now".
    fn first_run_import(&self, cx: &mut Context<Self>) -> AnyElement {
        use kandado_app::Source;
        let mut sources = crate::components::FieldGroup::new();
        for (index, source) in Source::ALL
            .into_iter()
            .filter(|source| *source != Source::Kandado)
            .enumerate()
        {
            let icon = match source {
                Source::Browser => IconName::Globe,
                Source::Csv => IconName::Note,
                _ => IconName::Shield,
            };
            let mut row = SettingRow::new(("first-run-source", index), icon, source.name());
            if !source.files().is_empty() {
                row = row.description(source.files());
            }
            sources = sources.child(row.opens(cx.listener(move |this, _, _, cx| {
                let _ = this.services.app.import_offer_done();
                cx.emit(Nav::Import(Some(source)));
            })));
        }
        v_flex()
            .flex_1()
            .min_h_0()
            .px(px(tokens::SPACE_6))
            .py(px(tokens::SPACE_5))
            .gap(px(tokens::SPACE_4))
            .child(
                h_flex()
                    .items_center()
                    .child(div().flex_1().child(heading("Bring your passwords", cx)))
                    .child(
                        Button::new("skip-import", "Skip for now")
                            .variant(ButtonVariant::Ghost)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let _ = this.services.app.import_offer_done();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(640.))
                    .gap(px(tokens::SPACE_3))
                    .child(paragraph(
                        "Moving from another manager? Import everything in one go. You can do this later in Settings \u{203a} Import.",
                        cx,
                    ))
                    .child(group_label("Import from", cx))
                    .child(sources),
            )
            .into_any_element()
    }
}
