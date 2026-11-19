//! The vault window once it is unlocked: sidebar (or rail), item list and
//! detail pane. The shell owns where the person is (which vault, which item,
//! what is selected), the keyboard shortcuts, the dialogs and the toast; the
//! panes and dialogs themselves are drawn by the views.
//!
//! | Drawn in | What |
//! | --- | --- |
//! | [`rail`] | The sidebar and its 64 px rail (D-17) |
//! | `views::items` | List, empty states, search, multi-select, create menu, move dialog, detail, forms, history |
//! | `views::trash` | The trash list header and its empty state |
//! | `views::vaults` | The create and edit vault dialog |
//! | `views::system` | Confirmation dialogs and the shortcuts list |

pub mod rail;

use std::{rc::Rc, time::Duration};

use gpui_kit::AppContext as _;
use gpui_kit::{
    AnyElement, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, KeyDownEvent, ListAlignment, ListState, Modifiers, ParentElement, Pixels, Render,
    SharedString, Styled, Subscription, Window,
    component::{
        h_flex,
        input::{InputEvent, InputState},
    },
    div,
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::{Entry, Filter, VaultInfo};
use kandado_model::{ItemContent, ItemData, ItemKind, item::Login};
use uuid::Uuid;

use crate::{
    assets::IconName,
    components::{Hue, Toast, panel},
    theme::tokens,
    views::{
        Nav, Services, explain,
        items::{
            detail::{DetailEvent, DetailView, copied_message},
            form::{FormEvent, FormStart, FormView},
            history::{HistoryEvent, HistoryView},
        },
        vaults::{VaultDialog, VaultDialogEvent},
    },
};

/// Below this window width the sidebar becomes a rail (D-17).
pub const RAIL_BREAKPOINT: Pixels = px(1120.);
pub const SIDEBAR_WIDTH: Pixels = px(232.);
pub const RAIL_WIDTH: Pixels = px(64.);
pub const LIST_WIDTH: Pixels = px(360.);
/// How long a toast stays.
const TOAST_TIME: Duration = Duration::from_secs(4);

/// What the list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    AllVaults,
    Vault(Uuid),
    Trash,
    /// Monitor arrives in phase 7; the entry explains that.
    Monitor,
}

/// What the detail pane shows.
pub(crate) enum Pane {
    /// Nothing chosen: an empty state, or the multi-select summary.
    Empty,
    Detail(Entity<DetailView>),
    Form(Entity<FormView>),
    History(Entity<HistoryView>),
}

/// Something the person asked for that would close a form with edits. It
/// waits behind the discard dialog (D-26).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Pending {
    Open(Uuid),
    Go(Place),
    Create(ItemKind),
    CloseForm,
    Lock,
    Settings,
}

/// What is drawn over the panes.
pub(crate) enum Overlay {
    None,
    CreateMenu,
    Vault(Entity<VaultDialog>),
    Move { ids: Vec<Uuid>, target: Uuid },
    DeleteForever(Vec<Uuid>),
    EmptyTrash,
    Discard(Pending),
    Shortcuts,
}

/// A row of the list pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Row {
    Label(&'static str),
    Item(Entry),
}

pub(crate) struct ToastState {
    pub message: SharedString,
    pub danger: bool,
    /// Items an Undo would take back out of the trash.
    pub undo: Option<Vec<Uuid>>,
    serial: u64,
}

/// Things the window does for the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellEvent {
    Lock,
}

pub struct VaultShell {
    pub(crate) services: Services,
    pub(crate) email: String,
    focus: FocusHandle,
    pub(crate) place: Place,
    pub(crate) kind: Option<ItemKind>,
    pub(crate) vaults: Vec<VaultInfo>,
    pub(crate) rows: Rc<Vec<Row>>,
    pub(crate) list: ListState,
    /// `(items in all vaults, items in the trash)`.
    pub(crate) counts: (usize, usize),
    /// The item open in the detail pane.
    pub(crate) selected: Option<Uuid>,
    /// Items ticked for a bulk action (D-15), in the order they were ticked.
    pub(crate) checked: Vec<Uuid>,
    pub(crate) pane: Pane,
    pub(crate) overlay: Overlay,
    pub(crate) search: Entity<InputState>,
    pub(crate) search_open: bool,
    pub(crate) results: Vec<Entry>,
    pub(crate) result_index: usize,
    pub(crate) toast: Option<ToastState>,
    toast_serial: u64,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<Nav> for VaultShell {}
impl EventEmitter<ShellEvent> for VaultShell {}

impl Focusable for VaultShell {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus.clone()
    }
}

/// The hue a vault's colour name means.
pub fn hue_of(colour: &str) -> Hue {
    match colour {
        // Stored names from the previous design system, and Hardware's own.
        "teal" | "green" => Hue::Teal,
        "coral" | "orange" => Hue::Coral,
        "sky" => Hue::Sky,
        "plum" => Hue::Plum,
        "moss" | "graphite" => Hue::Moss,
        _ => Hue::Brass,
    }
}

/// The icon a vault's icon name means.
pub fn vault_icon(icon: &str) -> IconName {
    match icon {
        "user" => IconName::User,
        "bolt" => IconName::Bolt,
        "card" => IconName::Card,
        "star" => IconName::Star,
        "shield" => IconName::Shield,
        "globe" => IconName::Globe,
        "key" => IconName::Key,
        _ => IconName::Vault,
    }
}

impl VaultShell {
    pub fn new(
        services: Services,
        email: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search in all vaults"));
        let subscriptions = vec![
            cx.subscribe(&search, |this, _, event: &InputEvent, cx| match event {
                InputEvent::Change | InputEvent::Focus => this.search_changed(cx),
                _ => {}
            }),
            // Whatever is revealed masks again when the window loses focus.
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active()
                    && let Pane::Form(form) = &this.pane
                {
                    let form = form.clone();
                    form.update(cx, |form, cx| form.mask_all(window, cx));
                }
            }),
        ];
        let mut shell = Self {
            services,
            email,
            focus: cx.focus_handle(),
            place: Place::AllVaults,
            kind: None,
            vaults: Vec::new(),
            rows: Rc::new(Vec::new()),
            list: ListState::new(0, ListAlignment::Top, px(600.)),
            counts: (0, 0),
            selected: None,
            checked: Vec::new(),
            pane: Pane::Empty,
            overlay: Overlay::None,
            search,
            search_open: false,
            results: Vec::new(),
            result_index: 0,
            toast: None,
            toast_serial: 0,
            _subscriptions: subscriptions,
        };
        shell.refresh(cx);
        shell.focus.focus(window, cx);
        shell
    }

    /// The search field, which the window draws in its title bar.
    pub fn search_input(&self) -> &Entity<InputState> {
        &self.search
    }

    pub fn place(&self) -> Place {
        self.place
    }

    pub fn selected(&self) -> Option<Uuid> {
        self.selected
    }

    pub fn checked(&self) -> &[Uuid] {
        &self.checked
    }

    pub fn form(&self) -> Option<&Entity<FormView>> {
        match &self.pane {
            Pane::Form(form) => Some(form),
            _ => None,
        }
    }

    pub fn detail(&self) -> Option<&Entity<DetailView>> {
        match &self.pane {
            Pane::Detail(detail) => Some(detail),
            _ => None,
        }
    }

    pub fn has_dialog(&self) -> bool {
        !matches!(self.overlay, Overlay::None)
    }

    pub fn toast_message(&self) -> Option<SharedString> {
        self.toast.as_ref().map(|toast| toast.message.clone())
    }

    pub fn search_results(&self) -> &[Entry] {
        &self.results
    }

    // ---- data --------------------------------------------------------------

    fn filter(&self) -> Filter {
        Filter {
            vault: match self.place {
                Place::Vault(id) => Some(id),
                _ => None,
            },
            kind: self.kind,
            trash: self.place == Place::Trash,
        }
    }

    /// The ids of the items in the list, in the order shown.
    pub(crate) fn listed(&self) -> Vec<Uuid> {
        self.rows
            .iter()
            .filter_map(|row| match row {
                Row::Item(entry) => Some(entry.id),
                Row::Label(_) => None,
            })
            .collect()
    }

    /// Reads the vaults and the list again after anything changed.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let app = &self.services.app;
        self.vaults = app.vaults().unwrap_or_default();
        self.counts = app.counts();
        if let Place::Vault(id) = self.place
            && !self.vaults.iter().any(|vault| vault.id == id)
        {
            self.place = Place::AllVaults;
        }
        let entries = match self.place {
            Place::Monitor => Vec::new(),
            _ => app.entries(self.filter()),
        };
        let mut rows = Vec::with_capacity(entries.len() + 2);
        if self.place == Place::Trash {
            rows.extend(entries.into_iter().map(Row::Item));
        } else {
            let (favourites, others): (Vec<Entry>, Vec<Entry>) =
                entries.into_iter().partition(|entry| entry.favourite);
            if !favourites.is_empty() {
                rows.push(Row::Label("Favourites"));
                rows.extend(favourites.into_iter().map(Row::Item));
                if !others.is_empty() {
                    rows.push(Row::Label("Items"));
                }
            }
            rows.extend(others.into_iter().map(Row::Item));
        }
        if *self.rows != rows {
            self.list.reset(rows.len());
            self.rows = Rc::new(rows);
        }
        let listed = self.listed();
        self.checked.retain(|id| listed.contains(id));
        if self.search_open {
            self.results = app.search(&self.search.read(cx).value());
            self.result_index = self.result_index.min(self.results.len().saturating_sub(1));
        }
        cx.notify();
    }

    pub(crate) fn vault_name(&self, id: Uuid) -> (SharedString, Hue) {
        self.vaults
            .iter()
            .find(|vault| vault.id == id)
            .map(|vault| (vault.meta.name.clone().into(), hue_of(&vault.meta.colour)))
            .unwrap_or_default()
    }

    /// The vault a new item goes into: the one being looked at, or the first.
    fn target_vault(&self) -> Option<Uuid> {
        match self.place {
            Place::Vault(id) => Some(id),
            _ => self.vaults.first().map(|vault| vault.id),
        }
    }

    pub(crate) fn place_title(&self) -> SharedString {
        match self.place {
            Place::AllVaults => "All vaults".into(),
            Place::Vault(id) => self.vault_name(id).0,
            Place::Trash => "Trash".into(),
            Place::Monitor => "Monitor".into(),
        }
    }

    // ---- toasts ------------------------------------------------------------

    fn show_toast(
        &mut self,
        message: impl Into<SharedString>,
        danger: bool,
        undo: Option<Vec<Uuid>>,
        cx: &mut Context<Self>,
    ) {
        self.toast_serial += 1;
        let serial = self.toast_serial;
        self.toast = Some(ToastState {
            message: message.into(),
            danger,
            undo,
            serial,
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TOAST_TIME).await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .toast
                    .as_ref()
                    .is_some_and(|toast| toast.serial == serial)
                {
                    this.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn toast(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.show_toast(message, false, None, cx);
    }

    pub(crate) fn fail(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.show_toast(message, true, None, cx);
    }

    /// Runs a vault change, refreshes the lists and reports a failure as a toast.
    fn attempt(
        &mut self,
        result: Result<(), kandado_app::AppError>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.refresh(cx);
        match result {
            Ok(()) => true,
            Err(error) => {
                self.fail(explain(&error), cx);
                false
            }
        }
    }

    // ---- moving around -------------------------------------------------------

    fn dirty_form(&self, cx: &gpui_kit::App) -> bool {
        matches!(&self.pane, Pane::Form(form) if form.read(cx).is_dirty(cx))
    }

    /// Does `pending` now, or asks first if a form has edits (D-26).
    pub(crate) fn request(
        &mut self,
        pending: Pending,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dirty_form(cx) {
            self.overlay = Overlay::Discard(pending);
            cx.notify();
        } else {
            self.perform(pending, window, cx);
        }
    }

    pub(crate) fn perform(
        &mut self,
        pending: Pending,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.overlay = Overlay::None;
        match pending {
            Pending::Open(id) => self.open(id, window, cx),
            Pending::Go(place) => {
                self.place = place;
                self.kind = None;
                self.selected = None;
                self.checked.clear();
                self.pane = Pane::Empty;
                self.refresh(cx);
                self.focus.focus(window, cx);
            }
            Pending::Create(kind) => self.start_form(kind, None, window, cx),
            Pending::CloseForm => self.close_form(window, cx),
            Pending::Lock => cx.emit(ShellEvent::Lock),
            Pending::Settings => cx.emit(Nav::Settings),
        }
        cx.notify();
    }

    pub fn go(&mut self, place: Place, window: &mut Window, cx: &mut Context<Self>) {
        self.request(Pending::Go(place), window, cx);
    }

    pub fn set_kind(&mut self, kind: Option<ItemKind>, cx: &mut Context<Self>) {
        self.kind = kind;
        self.refresh(cx);
    }

    /// Opens an item in the detail pane.
    fn open(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.services.app.entry(id) else {
            self.selected = None;
            self.pane = Pane::Empty;
            return;
        };
        // Coming from search, show the list the item lives in.
        if !self.listed().contains(&id) {
            self.place = if entry.is_trashed() {
                Place::Trash
            } else {
                Place::AllVaults
            };
            self.kind = None;
            self.refresh(cx);
        }
        let (vault_name, hue) = self.vault_name(entry.vault_id);
        let services = self.services.clone();
        let detail = cx.new(|cx| DetailView::new(services, id, vault_name, hue, window, cx));
        self._subscriptions.truncate(2);
        self._subscriptions.push(cx.subscribe_in(
            &detail,
            window,
            |this, _, event: &DetailEvent, window, cx| this.on_detail(event.clone(), window, cx),
        ));
        self.selected = Some(id);
        self.checked.clear();
        self.pane = Pane::Detail(detail);
        if let Some(position) = self
            .rows
            .iter()
            .position(|row| matches!(row, Row::Item(entry) if entry.id == id))
        {
            self.list.scroll_to_reveal_item(position);
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// A click on a row. Ctrl and Shift tick rows instead of opening them.
    pub fn click_row(
        &mut self,
        id: Uuid,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if modifiers.shift {
            self.check_range_to(id, cx);
        } else if modifiers.control || modifiers.platform || !self.checked.is_empty() {
            self.toggle_checked(id, cx);
        } else {
            self.request(Pending::Open(id), window, cx);
        }
    }

    pub fn select(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        self.request(Pending::Open(id), window, cx);
    }

    /// ↓ and ↑: the next or previous item in the list.
    fn step(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let listed = self.listed();
        if listed.is_empty() {
            return;
        }
        let current = self
            .selected
            .and_then(|id| listed.iter().position(|listed| *listed == id));
        let next = match (current, forward) {
            (None, _) => 0,
            (Some(position), true) => (position + 1).min(listed.len() - 1),
            (Some(position), false) => position.saturating_sub(1),
        };
        self.request(Pending::Open(listed[next]), window, cx);
    }

    // ---- multi-select (D-15) ---------------------------------------------------

    pub fn toggle_checked(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if matches!(self.pane, Pane::Form(_)) {
            return;
        }
        // Starting a selection takes the open item with it.
        if self.checked.is_empty()
            && let Some(selected) = self.selected.take()
            && selected != id
        {
            self.checked.push(selected);
        }
        match self.checked.iter().position(|checked| *checked == id) {
            Some(position) => {
                self.checked.remove(position);
            }
            None => self.checked.push(id),
        }
        self.selected = None;
        self.pane = Pane::Empty;
        cx.notify();
    }

    fn check_range_to(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if matches!(self.pane, Pane::Form(_)) {
            return;
        }
        let listed = self.listed();
        let anchor = self.checked.last().copied().or(self.selected);
        let (Some(from), Some(to)) = (
            anchor.and_then(|anchor| listed.iter().position(|listed| *listed == anchor)),
            listed.iter().position(|listed| *listed == id),
        ) else {
            return self.toggle_checked(id, cx);
        };
        let range = if from <= to { from..=to } else { to..=from };
        for id in &listed[range] {
            if !self.checked.contains(id) {
                self.checked.push(*id);
            }
        }
        self.selected = None;
        self.pane = Pane::Empty;
        cx.notify();
    }

    pub fn check_all(&mut self, cx: &mut Context<Self>) {
        if matches!(self.pane, Pane::Form(_)) {
            return;
        }
        self.checked = self.listed();
        self.selected = None;
        self.pane = Pane::Empty;
        cx.notify();
    }

    pub fn clear_checked(&mut self, cx: &mut Context<Self>) {
        self.checked.clear();
        cx.notify();
    }

    /// The items an action applies to: the ticked ones, or the open one.
    fn targets(&self) -> Vec<Uuid> {
        if self.checked.is_empty() {
            self.selected.into_iter().collect()
        } else {
            self.checked.clone()
        }
    }

    // ---- item actions ----------------------------------------------------------

    /// Moves items to the trash. It never asks; the toast offers Undo.
    pub fn trash(&mut self, ids: Vec<Uuid>, window: &mut Window, cx: &mut Context<Self>) {
        if ids.is_empty() || self.place == Place::Trash {
            return;
        }
        let result = self.services.app.trash_items(&ids);
        if ids.iter().any(|id| self.selected == Some(*id)) {
            self.selected = None;
            self.pane = Pane::Empty;
        }
        self.checked.clear();
        if self.attempt(result, cx) {
            let message = if ids.len() == 1 {
                "Moved to trash.".to_owned()
            } else {
                format!("{} items moved to trash.", ids.len())
            };
            self.show_toast(message, false, Some(ids), cx);
        }
        self.focus.focus(window, cx);
    }

    /// Undo on the toast: the items come back out of the trash.
    pub fn undo_trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ids) = self.toast.take().and_then(|toast| toast.undo) else {
            return;
        };
        let result = self.services.app.restore_items(&ids);
        if self.attempt(result, cx)
            && let [id] = ids[..]
        {
            self.open(id, window, cx);
        }
    }

    pub fn restore(&mut self, ids: Vec<Uuid>, cx: &mut Context<Self>) {
        let result = self.services.app.restore_items(&ids);
        self.selected = None;
        self.checked.clear();
        self.pane = Pane::Empty;
        if self.attempt(result, cx) {
            self.toast(
                if ids.len() == 1 {
                    "Restored to its vault.".to_owned()
                } else {
                    format!("{} items restored.", ids.len())
                },
                cx,
            );
        }
    }

    pub fn favourite(&mut self, ids: Vec<Uuid>, cx: &mut Context<Self>) {
        let result = self.services.app.set_favourite(&ids, true);
        if self.attempt(result, cx) {
            self.toast("Added to favourites.", cx);
        }
    }

    pub fn ask_move(&mut self, ids: Vec<Uuid>, cx: &mut Context<Self>) {
        let Some(first) = ids.first().and_then(|id| self.services.app.entry(*id)) else {
            return;
        };
        self.overlay = Overlay::Move {
            ids,
            target: first.vault_id,
        };
        cx.notify();
    }

    /// Chooses the vault in the move dialog.
    pub fn set_move_target(&mut self, vault: Uuid, cx: &mut Context<Self>) {
        if let Overlay::Move { target, .. } = &mut self.overlay {
            *target = vault;
        }
        cx.notify();
    }

    pub fn confirm_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Overlay::Move { ids, target } = std::mem::replace(&mut self.overlay, Overlay::None)
        else {
            return;
        };
        let result = self.services.app.move_items(&ids, target);
        self.checked.clear();
        if self.attempt(result, cx) {
            let (vault, _) = self.vault_name(target);
            self.toast(
                if ids.len() == 1 {
                    format!("Moved to {vault}.")
                } else {
                    format!("{} items moved to {vault}.", ids.len())
                },
                cx,
            );
            if let Some(id) = self.selected {
                self.open(id, window, cx);
            }
        }
        self.focus.focus(window, cx);
    }

    pub fn ask_delete_forever(&mut self, ids: Vec<Uuid>, cx: &mut Context<Self>) {
        if !ids.is_empty() {
            self.overlay = Overlay::DeleteForever(ids);
            cx.notify();
        }
    }

    pub fn ask_empty_trash(&mut self, cx: &mut Context<Self>) {
        if self.counts.1 > 0 {
            self.overlay = Overlay::EmptyTrash;
            cx.notify();
        }
    }

    /// The confirm button of whichever dialog is open.
    pub fn confirm_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match std::mem::replace(&mut self.overlay, Overlay::None) {
            Overlay::DeleteForever(ids) => {
                let result = self.services.app.delete_forever(&ids);
                self.selected = None;
                self.checked.clear();
                self.pane = Pane::Empty;
                if self.attempt(result, cx) {
                    self.toast("Deleted forever.", cx);
                }
            }
            Overlay::EmptyTrash => {
                let result = self.services.app.empty_trash();
                self.selected = None;
                self.checked.clear();
                self.pane = Pane::Empty;
                let emptied = result.as_ref().map(|_| ()).map_err(|_| ());
                if self.attempt(result.map(|_| ()), cx) && emptied.is_ok() {
                    self.toast("Trash emptied.", cx);
                }
            }
            Overlay::Discard(pending) => {
                self.pane = Pane::Empty;
                self.perform(pending, window, cx);
                return;
            }
            Overlay::Move { ids, target } => {
                self.overlay = Overlay::Move { ids, target };
                return self.confirm_move(window, cx);
            }
            other => self.overlay = other,
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = Overlay::None;
        // Back to the form if one is open, otherwise to the shell's shortcuts.
        match &self.pane {
            Pane::Form(form) => {
                let title = form.read(cx).title_input().clone();
                title.update(cx, |input, cx| input.focus(window, cx));
            }
            _ => self.focus.focus(window, cx),
        }
        cx.notify();
    }

    // ---- create and edit ---------------------------------------------------------

    pub fn open_create_menu(&mut self, cx: &mut Context<Self>) {
        self.overlay = Overlay::CreateMenu;
        cx.notify();
    }

    pub fn create(&mut self, kind: ItemKind, window: &mut Window, cx: &mut Context<Self>) {
        self.request(Pending::Create(kind), window, cx);
    }

    fn start_form(
        &mut self,
        kind: ItemKind,
        prefill: Option<ItemContent>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(vault) = self.target_vault() else {
            return self.fail("Create a vault first.", cx);
        };
        if matches!(self.place, Place::Trash | Place::Monitor) {
            self.place = Place::AllVaults;
            self.refresh(cx);
        }
        self.show_form(
            FormStart::Create {
                kind,
                vault,
                prefill: prefill.map(Box::new),
            },
            window,
            cx,
        );
    }

    pub fn edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        match self.services.app.item(id) {
            Ok(item) if item.trashed_at.is_none() => {
                self.show_form(FormStart::Edit(Box::new(item)), window, cx);
            }
            Ok(_) => {}
            Err(error) => self.fail(explain(&error), cx),
        }
    }

    fn show_form(&mut self, start: FormStart, window: &mut Window, cx: &mut Context<Self>) {
        let services = self.services.clone();
        let form = cx.new(|cx| FormView::new(services, start, window, cx));
        self._subscriptions.truncate(2);
        self._subscriptions.push(cx.subscribe_in(
            &form,
            window,
            |this, _, event: &FormEvent, window, cx| match event {
                FormEvent::Saved(id) => {
                    let id = *id;
                    this.pane = Pane::Empty;
                    this.refresh(cx);
                    this.open(id, window, cx);
                    this.toast("Saved.", cx);
                }
                FormEvent::CancelRequested => this.request(Pending::CloseForm, window, cx),
                FormEvent::Toast(message) => this.toast(message.clone(), cx),
            },
        ));
        self.checked.clear();
        self.overlay = Overlay::None;
        self.pane = Pane::Form(form);
        cx.notify();
    }

    /// Leaves the form without saving, back to the item it was editing.
    fn close_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pane = Pane::Empty;
        match self.selected {
            Some(id) => self.open(id, window, cx),
            None => self.focus.focus(window, cx),
        }
        cx.notify();
    }

    fn open_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        let services = self.services.clone();
        let history = cx.new(|cx| HistoryView::new(services, id, window, cx));
        self._subscriptions.truncate(2);
        self._subscriptions.push(cx.subscribe_in(
            &history,
            window,
            move |this, _, event: &HistoryEvent, window, cx| match event {
                HistoryEvent::Back => this.open(id, window, cx),
                HistoryEvent::Restored => {
                    this.refresh(cx);
                    this.open(id, window, cx);
                    this.toast("Version restored.", cx);
                }
                HistoryEvent::Failed(message) => this.fail(message.clone(), cx),
            },
        ));
        self.pane = Pane::History(history);
        cx.notify();
    }

    fn on_detail(&mut self, event: DetailEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected else {
            return;
        };
        match event {
            DetailEvent::Edit => self.edit(window, cx),
            DetailEvent::Move => self.ask_move(vec![id], cx),
            DetailEvent::History => self.open_history(window, cx),
            DetailEvent::Duplicate => match self.services.app.duplicate_item(id) {
                Ok(copy) => {
                    self.refresh(cx);
                    self.open(copy, window, cx);
                    self.toast("Duplicated.", cx);
                }
                Err(error) => self.fail(explain(&error), cx),
            },
            DetailEvent::Trash => self.trash(vec![id], window, cx),
            DetailEvent::Restore => self.restore(vec![id], cx),
            DetailEvent::DeleteForever => self.ask_delete_forever(vec![id], cx),
            DetailEvent::SaveAsLogin => self.save_as_login(id, window, cx),
            DetailEvent::Changed => self.refresh(cx),
            DetailEvent::Toast(message) => self.toast(message, cx),
            DetailEvent::Failed(message) => self.fail(message, cx),
        }
    }

    /// D-12: a login form that starts with the saved password.
    fn save_as_login(&mut self, id: Uuid, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(item) = self.services.app.item(id) else {
            return;
        };
        let ItemData::Password(saved) = &item.content.data else {
            return;
        };
        let prefill = ItemContent {
            title: item.content.title.clone(),
            notes: item.content.notes.clone(),
            data: ItemData::Login(Login {
                password: saved.password.clone(),
                ..Login::default()
            }),
            ..ItemContent::default()
        };
        self.selected = None;
        self.start_form(ItemKind::Login, Some(prefill), window, cx);
    }

    // ---- vault dialog (D-27) ---------------------------------------------------------

    /// Opens the dialog for a new vault, or to edit `vault`.
    pub fn open_vault_dialog(
        &mut self,
        vault: Option<Uuid>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing =
            vault.and_then(|id| self.vaults.iter().find(|vault| vault.id == id).cloned());
        let services = self.services.clone();
        let dialog = cx.new(|cx| VaultDialog::new(services, existing, window, cx));
        cx.subscribe_in(
            &dialog,
            window,
            |this, _, event: &VaultDialogEvent, window, cx| {
                if let VaultDialogEvent::Saved { id, created } = event {
                    this.overlay = Overlay::None;
                    if *created {
                        // A new vault is selected in the sidebar.
                        this.perform(Pending::Go(Place::Vault(*id)), window, cx);
                        this.toast("Vault created.", cx);
                    } else {
                        this.refresh(cx);
                        this.toast("Vault saved.", cx);
                    }
                } else {
                    this.close_dialog(window, cx);
                }
            },
        )
        .detach();
        self.overlay = Overlay::Vault(dialog);
        cx.notify();
    }

    // ---- search (D-14) ------------------------------------------------------------------

    /// Runs the search for what is in the field now.
    pub fn search_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value();
        self.search_open = !query.trim().is_empty();
        self.results = if self.search_open {
            self.services.app.search(&query)
        } else {
            Vec::new()
        };
        self.result_index = 0;
        cx.notify();
    }

    pub fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = Overlay::None;
        self.search.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.search_open = false;
        self.results.clear();
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn open_result(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.results.get(index).map(|entry| entry.id) {
            self.close_search(window, cx);
            self.request(Pending::Open(id), window, cx);
        }
    }

    /// Ctrl C in the results: the password, without opening the item.
    fn copy_result_password(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.results.get(self.result_index) else {
            return;
        };
        let field = self
            .services
            .app
            .item_detail(entry.id)
            .ok()
            .and_then(|detail| detail.password_field);
        match field.map(|field| self.services.app.copy_field(entry.id, field)) {
            Some(Ok(copied)) => self.toast(copied_message(&copied), cx),
            Some(Err(error)) => self.fail(explain(&error), cx),
            None => {}
        }
    }

    /// Copies a row's password from the list, without opening it.
    pub fn copy_password_of(&mut self, id: Uuid, cx: &mut Context<Self>) {
        let field = self
            .services
            .app
            .item_detail(id)
            .ok()
            .and_then(|detail| detail.password_field);
        match field.map(|field| self.services.app.copy_field(id, field)) {
            Some(Ok(copied)) => self.toast(copied_message(&copied), cx),
            Some(Err(error)) => self.fail(explain(&error), cx),
            None => {}
        }
    }

    /// "Create login for …" when a search finds nothing.
    pub fn create_from_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.search.read(cx).value().trim().to_owned();
        self.close_search(window, cx);
        let prefill = ItemContent::new(title, ItemData::Login(Login::default()));
        if self.dirty_form(cx) {
            return self.request(Pending::Create(ItemKind::Login), window, cx);
        }
        self.start_form(ItemKind::Login, Some(prefill), window, cx);
    }

    // ---- keyboard (D-51) --------------------------------------------------------------------

    /// Every shortcut of the vault. Key listeners run before the text
    /// inputs' own bindings, so what a key does is decided here from where
    /// the person is: in search, in a dialog, in a form, or browsing.
    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.handle_keystroke(&event.keystroke, window, cx) {
            cx.stop_propagation();
        }
    }

    /// Whether the search field has the keyboard. It is drawn in the
    /// window's title bar, outside the shell, so the window passes its keys
    /// to [`VaultShell::handle_keystroke`].
    pub fn search_focused(&self, window: &Window, cx: &gpui_kit::App) -> bool {
        self.search.focus_handle(cx).is_focused(window)
    }

    /// Does what a key means here, and says whether it meant anything.
    pub fn handle_keystroke(
        &mut self,
        keystroke: &gpui_kit::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = keystroke.key.as_str();
        let modifiers = keystroke.modifiers;
        let ctrl = modifiers.control && !modifiers.alt && !modifiers.shift;
        let plain = !modifiers.control && !modifiers.alt && !modifiers.shift && !modifiers.platform;
        // While results show, the keys work on them even if the field lost focus.
        let in_search = self.search_open || self.search.focus_handle(cx).is_focused(window);
        let handled = if in_search {
            match key {
                "escape" if plain => {
                    self.close_search(window, cx);
                    true
                }
                "down" if plain => {
                    if !self.results.is_empty() {
                        self.result_index = (self.result_index + 1).min(self.results.len() - 1);
                    }
                    true
                }
                "up" if plain => {
                    self.result_index = self.result_index.saturating_sub(1);
                    true
                }
                "enter" if plain => {
                    if self.results.is_empty() && self.search_open {
                        self.create_from_search(window, cx);
                    } else {
                        self.open_result(self.result_index, window, cx);
                    }
                    true
                }
                "c" if ctrl && self.search_open && !self.results.is_empty() => {
                    self.copy_result_password(cx);
                    true
                }
                _ => false,
            }
        } else if self.has_dialog() {
            match key {
                "escape" if plain => {
                    self.close_dialog(window, cx);
                    true
                }
                _ => false,
            }
        } else if let Pane::Form(form) = &self.pane {
            let form = form.clone();
            match key {
                "escape" if plain => {
                    form.update(cx, |form, cx| form.escape(window, cx));
                    true
                }
                "s" if ctrl => {
                    form.update(cx, |form, cx| form.save(cx));
                    true
                }
                "f" if ctrl => {
                    self.focus_search(window, cx);
                    true
                }
                _ => false,
            }
        } else {
            let detail = self.detail().cloned();
            let with_detail =
                |cx: &mut Context<Self>,
                 act: fn(&mut DetailView, &mut Context<DetailView>) -> bool| {
                    detail.as_ref().is_some_and(|detail| detail.update(cx, act))
                };
            match key {
                "f" if ctrl => {
                    self.focus_search(window, cx);
                    true
                }
                "n" if ctrl => {
                    self.create(ItemKind::Login, window, cx);
                    true
                }
                "/" if ctrl => {
                    self.overlay = Overlay::Shortcuts;
                    true
                }
                "b" if ctrl => with_detail(cx, DetailView::copy_username),
                "c" if ctrl => with_detail(cx, DetailView::copy_password),
                "t" if ctrl => with_detail(cx, DetailView::copy_code),
                "e" if ctrl && self.detail().is_some() => {
                    self.edit(window, cx);
                    true
                }
                "delete" if plain => {
                    let targets = self.targets();
                    if self.place == Place::Trash {
                        self.ask_delete_forever(targets, cx);
                    } else {
                        self.trash(targets, window, cx);
                    }
                    true
                }
                "down" if plain => {
                    self.step(true, window, cx);
                    true
                }
                "up" if plain => {
                    self.step(false, window, cx);
                    true
                }
                "escape" if plain && !self.checked.is_empty() => {
                    self.clear_checked(cx);
                    true
                }
                "a" if ctrl && !matches!(self.place, Place::Monitor) => {
                    self.check_all(cx);
                    true
                }
                _ => false,
            }
        };
        if handled {
            cx.notify();
        }
        handled
    }

    /// The title bar's Lock and Settings, which must not drop a form's edits.
    pub fn ask_lock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request(Pending::Lock, window, cx);
    }

    pub fn ask_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request(Pending::Settings, window, cx);
    }

    pub fn show_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.overlay = Overlay::Shortcuts;
        cx.notify();
    }

    fn toast_layer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let toast = self.toast.as_ref()?;
        let mut element = Toast::new("toast-action", toast.message.clone());
        if toast.danger {
            element = element.danger();
        }
        if toast.undo.is_some() {
            element = element.action(
                "Undo",
                cx.listener(|this, _, window, cx| this.undo_trash(window, cx)),
            );
        }
        Some(
            h_flex()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(tokens::SPACE_5))
                .justify_center()
                .child(element)
                .into_any_element(),
        )
    }
}

impl Render for VaultShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = window.viewport_size().width;
        let rail = width < RAIL_BREAKPOINT;
        let pane: AnyElement = match &self.pane {
            Pane::Detail(view) => view.clone().into_any_element(),
            Pane::Form(view) => view.clone().into_any_element(),
            Pane::History(view) => view.clone().into_any_element(),
            Pane::Empty => self.empty_pane(cx),
        };
        let overlay = self.overlay_layer(window, cx);
        let search = self.search_layer(cx);
        // Floating panels on the body ground, 14 px apart (Main board): the
        // vault panel, the list panel and the detail panel.
        div()
            .id("vault-shell")
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .gap(px(tokens::SPACE_4))
            .track_focus(&self.focus)
            // Before the inputs, which would otherwise take keys like Ctrl S.
            .capture_key_down(cx.listener(Self::on_key))
            .child(self.sidebar(rail, cx))
            .child(self.list_pane(cx))
            .child(
                panel(cx)
                    .flex()
                    .flex_col()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_hidden()
                    .child(pane)
                    .when_some(self.toast_layer(cx), |pane, toast| pane.child(toast)),
            )
            .when_some(search, |shell, search| shell.child(search))
            .when_some(overlay, |shell, overlay| shell.child(overlay))
    }
}

impl VaultShell {
    /// Something from the server arrived: draw the lists again, and follow
    /// the open item if it changed or is gone.
    pub fn on_synced(&mut self, cx: &mut Context<Self>) {
        self.refresh(cx);
        let Some(id) = self.selected else {
            return;
        };
        match (&self.pane, self.services.app.entry(id)) {
            (Pane::Detail(_), None) => {
                self.selected = None;
                self.pane = Pane::Empty;
            }
            (Pane::Detail(detail), Some(entry)) => {
                let detail = detail.clone();
                let changed = detail
                    .read(cx)
                    .detail()
                    .is_none_or(|shown| shown.updated_at != entry.updated_at);
                if changed {
                    detail.update(cx, |detail, cx| detail.reload(cx));
                }
            }
            _ => {}
        }
        cx.notify();
    }
}
