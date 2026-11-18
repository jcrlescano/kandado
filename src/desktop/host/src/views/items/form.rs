//! The create and edit forms, in the detail pane so the list stays visible:
//! D-19 login (with the generator popover), D-22 card, D-23 note, D-24
//! identity, and the password item that the create menu offers.
//!
//! One view draws every kind: the title, the kind's own fields, the note,
//! custom fields and the vault. The form holds what the person is typing,
//! secrets included, only while it is open.
//!
//! States: new, editing, a field in error (title missing, expiry not a date),
//! generator open, add two-factor dialog open, saving failed.

use std::collections::{HashMap, HashSet};

use gpui_kit::AppContext as _;
use gpui_kit::{
    Context, Entity, EventEmitter, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
    component::{
        button::Button as KitButton,
        h_flex,
        input::{InputEvent, InputState, TextareaState},
        menu::{DropdownMenu as _, PopupMenuItem},
        scroll::ScrollableElement as _,
        v_flex,
    },
    deferred, div,
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::{Item, VaultInfo};
use kandado_model::{
    ItemContent, ItemData, ItemKind, Secret,
    item::{Card, CustomField, Document, Expiry, Identity, Login, Password, Website},
    strength,
};
use uuid::Uuid;

use super::{
    super::{Services, error_line, explain},
    add_totp::{AddTotp, AddTotpEvent},
    detail::copied_message,
};
use crate::{
    assets::IconName,
    components::{
        Button, ButtonSize, ButtonVariant, GeneratorEvent, IconButton, ItemIcon, PasswordGenerator,
        StrengthMeter, TextArea, TextField, Toggle, section_label,
    },
    theme::{DISPLAY_FACE, KandadoTheme, TextStyles as _, color, shadow, tokens},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormEvent {
    Saved(Uuid),
    /// Cancel or Esc. The shell asks to discard if there are edits (D-26).
    CancelRequested,
    Toast(SharedString),
}

/// What the form opens with.
pub enum FormStart {
    Create {
        kind: ItemKind,
        vault: Uuid,
        /// Values to start from, such as a password being saved as a login.
        prefill: Option<Box<ItemContent>>,
    },
    Edit(Box<Item>),
}

/// A label and a value the person can add any number of: custom fields, and
/// an identity's documents.
struct Pair {
    label: Entity<InputState>,
    value: Entity<InputState>,
    hidden: bool,
}

pub struct FormView {
    services: Services,
    /// The item being edited, or `None` for a new one.
    id: Option<Uuid>,
    kind: ItemKind,
    /// The content the form started from. Fields this version does not know
    /// are carried through from here.
    original: ItemContent,
    /// What the form produced when it opened, to tell whether anything changed.
    baseline: ItemContent,
    vaults: Vec<VaultInfo>,
    vault: Uuid,
    original_vault: Uuid,
    title: Entity<InputState>,
    notes: Entity<TextareaState>,
    inputs: HashMap<&'static str, Entity<InputState>>,
    websites: Vec<Entity<InputState>>,
    custom: Vec<Pair>,
    documents: Vec<Pair>,
    totp: Option<Secret>,
    /// Masked inputs the person has chosen to see.
    shown: HashSet<&'static str>,
    generator: Option<Entity<PasswordGenerator>>,
    add_totp: Option<Entity<AddTotp>>,
    title_error: Option<SharedString>,
    expiry_error: Option<SharedString>,
    failure: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<FormEvent> for FormView {}

fn empty_content(kind: ItemKind) -> ItemContent {
    ItemContent::new(
        "",
        match kind {
            ItemKind::Login => ItemData::Login(Login::default()),
            ItemKind::Card => ItemData::Card(Card::default()),
            ItemKind::Identity => ItemData::Identity(Box::default()),
            ItemKind::Password => ItemData::Password(Password::default()),
            // Aliases and passkeys have no form yet; a note is the safe default.
            ItemKind::Note | ItemKind::Alias | ItemKind::Passkey => ItemData::default(),
        },
    )
}

/// "08/29", "8/2029" and "08 / 29" all mean August 2029.
/// An expiry that is not a month and a year.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadExpiry;

pub fn parse_expiry(text: &str) -> Result<Option<Expiry>, BadExpiry> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let (month, year) = text.split_once('/').ok_or(BadExpiry)?;
    let month: u8 = month.trim().parse().map_err(|_| BadExpiry)?;
    let year: u16 = year.trim().parse().map_err(|_| BadExpiry)?;
    let year = if year < 100 { 2000 + year } else { year };
    if !(1..=12).contains(&month) || !(2000..=2999).contains(&year) {
        return Err(BadExpiry);
    }
    Ok(Some(Expiry { month, year }))
}

fn kind_noun(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Login => "login",
        ItemKind::Alias => "alias",
        ItemKind::Card => "card",
        ItemKind::Note => "note",
        ItemKind::Identity => "identity",
        ItemKind::Password => "password",
        ItemKind::Passkey => "passkey",
    }
}

impl FormView {
    pub fn new(
        services: Services,
        start: FormStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let vaults = services.app.vaults().unwrap_or_default();
        let (id, kind, original, vault) = match start {
            FormStart::Create {
                kind,
                vault,
                prefill,
            } => (
                None,
                kind,
                prefill.map_or_else(|| empty_content(kind), |content| *content),
                vault,
            ),
            FormStart::Edit(item) => (
                Some(item.id),
                item.content.kind(),
                item.content,
                item.vault_id,
            ),
        };

        let mut subscriptions = Vec::new();
        let mut text = |value: &str, masked: bool, cx: &mut Context<Self>| {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(masked)
                    .default_value(value.to_owned())
            });
            // Typing clears errors and updates the strength meter.
            subscriptions.push(cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.title_error = None;
                    this.expiry_error = None;
                    this.failure = None;
                    cx.notify();
                }
            }));
            input
        };

        let title = text(&original.title, false, cx);
        let mut inputs = HashMap::new();
        let mut websites = Vec::new();
        let mut documents = Vec::new();
        let mut totp = None;
        match &original.data {
            ItemData::Login(login) => {
                inputs.insert("username", text(&login.username, false, cx));
                inputs.insert("password", text(login.password.expose(), true, cx));
                websites = login
                    .websites
                    .iter()
                    .map(|site| text(&site.url, false, cx))
                    .collect();
                if websites.is_empty() {
                    websites.push(text("", false, cx));
                }
                totp = login.totp.clone();
            }
            ItemData::Card(card) => {
                let expiry = card
                    .expiry
                    .map(|expiry| format!("{:02}/{:02}", expiry.month, expiry.year % 100))
                    .unwrap_or_default();
                inputs.insert("cardholder", text(&card.cardholder, false, cx));
                inputs.insert("number", text(card.number.expose(), false, cx));
                inputs.insert("expiry", text(&expiry, false, cx));
                inputs.insert("cvv", text(card.cvv.expose(), true, cx));
                inputs.insert("pin", text(card.pin.expose(), true, cx));
            }
            ItemData::Identity(identity) => {
                let address = &identity.address;
                for (key, value) in [
                    ("first_name", &identity.first_name),
                    ("last_name", &identity.last_name),
                    ("email", &identity.email),
                    ("phone", &identity.phone),
                    ("line1", &address.line1),
                    ("city", &address.city),
                    ("region", &address.region),
                    ("postal_code", &address.postal_code),
                    ("country", &address.country),
                    ("company", &identity.company),
                    ("job_title", &identity.job_title),
                ] {
                    inputs.insert(key, text(value, false, cx));
                }
                documents = identity
                    .documents
                    .iter()
                    .map(|document| Pair {
                        label: text(&document.label, false, cx),
                        value: text(document.number.expose(), false, cx),
                        hidden: true,
                    })
                    .collect();
            }
            ItemData::Password(password) => {
                inputs.insert("password", text(password.password.expose(), true, cx));
            }
            ItemData::Note(_) | ItemData::Alias(_) | ItemData::Passkey(_) => {}
        }
        let custom = original
            .custom_fields
            .iter()
            .map(|field| Pair {
                label: text(&field.label, false, cx),
                value: text(field.value.expose(), false, cx),
                hidden: field.hidden,
            })
            .collect();

        let rows = if kind == ItemKind::Note {
            (10, 24)
        } else {
            (3, 12)
        };
        let notes = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(rows.0, rows.1)
                .default_value(original.notes.clone())
        });
        title.update(cx, |input, cx| input.focus(window, cx));

        let mut form = Self {
            services,
            id,
            kind,
            baseline: original.clone(),
            original,
            vaults,
            vault,
            original_vault: vault,
            title,
            notes,
            inputs,
            websites,
            custom,
            documents,
            totp,
            shown: HashSet::new(),
            generator: None,
            add_totp: None,
            title_error: None,
            expiry_error: None,
            failure: None,
            _subscriptions: subscriptions,
        };
        form.baseline = form.build(cx).unwrap_or_else(|_| form.original.clone());
        form
    }

    pub fn kind(&self) -> ItemKind {
        self.kind
    }

    /// The item being edited, if this is not a new one.
    pub fn editing(&self) -> Option<Uuid> {
        self.id
    }

    pub fn title_input(&self) -> &Entity<InputState> {
        &self.title
    }

    pub fn input(&self, key: &str) -> Option<&Entity<InputState>> {
        self.inputs.get(key)
    }

    fn value(&self, key: &str, cx: &gpui_kit::App) -> String {
        self.inputs
            .get(key)
            .map(|input| input.read(cx).value().to_string())
            .unwrap_or_default()
    }

    fn pairs(pairs: &[Pair], cx: &gpui_kit::App) -> Vec<(String, String, bool)> {
        pairs
            .iter()
            .map(|pair| {
                (
                    pair.label.read(cx).value().trim().to_owned(),
                    pair.value.read(cx).value().to_string(),
                    pair.hidden,
                )
            })
            .filter(|(label, value, _)| !label.is_empty() || !value.is_empty())
            .collect()
    }

    /// The item as the form has it now. Fails only on a card expiry that is
    /// not a month and a year.
    fn build(&self, cx: &gpui_kit::App) -> Result<ItemContent, BadExpiry> {
        let mut content = self.original.clone();
        content.title = self.title.read(cx).value().trim().to_owned();
        content.notes = self.notes.read(cx).value().to_string();
        let old_custom = std::mem::take(&mut content.custom_fields);
        content.custom_fields = Self::pairs(&self.custom, cx)
            .into_iter()
            .enumerate()
            .map(|(index, (label, value, hidden))| CustomField {
                label,
                value: Secret::new(value),
                hidden,
                extra: old_custom
                    .get(index)
                    .map(|field| field.extra.clone())
                    .unwrap_or_default(),
            })
            .collect();
        match &mut content.data {
            ItemData::Login(login) => {
                login.username = self.value("username", cx).trim().to_owned();
                login.password = Secret::new(self.value("password", cx));
                let old_sites = std::mem::take(&mut login.websites);
                login.websites = self
                    .websites
                    .iter()
                    .map(|input| input.read(cx).value().trim().to_owned())
                    .filter(|url| !url.is_empty())
                    .enumerate()
                    .map(|(index, url)| Website {
                        url,
                        ..old_sites.get(index).cloned().unwrap_or_default()
                    })
                    .collect();
                login.totp = self.totp.clone();
            }
            ItemData::Card(card) => {
                card.cardholder = self.value("cardholder", cx).trim().to_owned();
                card.number = Secret::new(self.value("number", cx).trim());
                card.expiry = parse_expiry(&self.value("expiry", cx))?;
                card.cvv = Secret::new(self.value("cvv", cx).trim());
                card.pin = Secret::new(self.value("pin", cx).trim());
            }
            ItemData::Identity(identity) => {
                let get = |key: &str| self.value(key, cx).trim().to_owned();
                **identity = Identity {
                    first_name: get("first_name"),
                    last_name: get("last_name"),
                    email: get("email"),
                    phone: get("phone"),
                    company: get("company"),
                    job_title: get("job_title"),
                    address: kandado_model::item::Address {
                        line1: get("line1"),
                        city: get("city"),
                        region: get("region"),
                        postal_code: get("postal_code"),
                        country: get("country"),
                        ..identity.address.clone()
                    },
                    documents: Self::pairs(&self.documents, cx)
                        .into_iter()
                        .enumerate()
                        .map(|(index, (label, number, _))| Document {
                            label,
                            number: Secret::new(number),
                            extra: identity
                                .documents
                                .get(index)
                                .map(|document| document.extra.clone())
                                .unwrap_or_default(),
                        })
                        .collect(),
                    ..(**identity).clone()
                };
            }
            ItemData::Password(password) => {
                password.password = Secret::new(self.value("password", cx));
            }
            ItemData::Note(_) | ItemData::Alias(_) | ItemData::Passkey(_) => {}
        }
        Ok(content)
    }

    /// Whether closing the form would lose something.
    pub fn is_dirty(&self, cx: &gpui_kit::App) -> bool {
        self.vault != self.original_vault
            || self
                .build(cx)
                .map_or(true, |content| content != self.baseline)
    }

    /// The name the discard dialog uses: the title, or "this login".
    pub fn display_name(&self, cx: &gpui_kit::App) -> String {
        let title = self.title.read(cx).value().trim().to_owned();
        if title.is_empty() {
            format!("this {}", kind_noun(self.kind))
        } else {
            title
        }
    }

    pub fn save(&mut self, cx: &mut Context<Self>) {
        let content = match self.build(cx) {
            Ok(content) => content,
            Err(BadExpiry) => {
                self.expiry_error = Some("Enter the month and year, like 08/29.".into());
                cx.notify();
                return;
            }
        };
        if content.title.is_empty() {
            self.title_error = Some("Give this item a title so you can find it.".into());
            cx.notify();
            return;
        }
        let app = &self.services.app;
        let saved = match self.id {
            Some(id) => app.update_item(id, self.vault, &content).map(|()| id),
            None => app.create_item(self.vault, &content),
        };
        match saved {
            Ok(id) => cx.emit(FormEvent::Saved(id)),
            Err(error) => {
                self.failure = Some(explain(&error));
                cx.notify();
            }
        }
    }

    /// Esc: closes the generator or the two-factor dialog if one is open,
    /// otherwise asks to leave the form.
    pub fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.add_totp.take().is_some() || self.generator.take().is_some() {
            // The closed dialog had the focus; give it back to the form.
            self.title.update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
        } else {
            cx.emit(FormEvent::CancelRequested);
        }
    }

    fn toggle_shown(&mut self, key: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let show = !self.shown.remove(key);
        if show {
            self.shown.insert(key);
        }
        if let Some(input) = self.inputs.get(key) {
            input.update(cx, |input, cx| input.set_masked(!show, window, cx));
        }
        cx.notify();
    }

    /// Masks every revealed input again. Called when the window loses focus.
    pub fn mask_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for key in self.shown.drain() {
            if let Some(input) = self.inputs.get(key) {
                input.update(cx, |input, cx| input.set_masked(true, window, cx));
            }
        }
        cx.notify();
    }

    pub fn toggle_generator(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.generator.take().is_none() {
            let generator = cx.new(|cx| PasswordGenerator::new(window, cx).with_use_button());
            cx.subscribe_in(
                &generator,
                window,
                |this, _, event: &GeneratorEvent, window, cx| match event {
                    GeneratorEvent::Use(password) => {
                        this.use_password(password.expose().to_owned(), window, cx);
                    }
                    GeneratorEvent::Copy(password) => {
                        match this
                            .services
                            .app
                            .copy_text("Password", password.expose(), true)
                        {
                            Ok(copied) => cx.emit(FormEvent::Toast(copied_message(&copied))),
                            Err(error) => this.failure = Some(explain(&error)),
                        }
                        cx.notify();
                    }
                },
            )
            .detach();
            self.generator = Some(generator);
        }
        cx.notify();
    }

    pub fn use_password(&mut self, password: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.inputs.get("password") {
            input.update(cx, |input, cx| input.set_value(password, window, cx));
        }
        self.generator = None;
        cx.notify();
    }

    pub fn open_add_totp(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog = cx.new(|cx| AddTotp::new(self.services.clone(), window, cx));
        cx.subscribe_in(
            &dialog,
            window,
            |this, _, event: &AddTotpEvent, window, cx| {
                if let AddTotpEvent::Added(uri) = event {
                    this.totp = Some(uri.clone());
                }
                this.add_totp = None;
                this.title.update(cx, |input, cx| input.focus(window, cx));
                cx.notify();
            },
        )
        .detach();
        self.add_totp = Some(dialog);
        cx.notify();
    }

    fn new_input(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        let input = cx.new(|cx| InputState::new(window, cx));
        self._subscriptions
            .push(cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify()));
        input
    }

    pub fn add_website(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.new_input(window, cx);
        input.update(cx, |input, cx| input.focus(window, cx));
        self.websites.push(input);
        cx.notify();
    }

    pub fn add_custom_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pair = Pair {
            label: self.new_input(window, cx),
            value: self.new_input(window, cx),
            hidden: false,
        };
        pair.label.update(cx, |input, cx| input.focus(window, cx));
        self.custom.push(pair);
        cx.notify();
    }

    pub fn add_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pair = Pair {
            label: self.new_input(window, cx),
            value: self.new_input(window, cx),
            hidden: true,
        };
        pair.label.update(cx, |input, cx| input.focus(window, cx));
        self.documents.push(pair);
        cx.notify();
    }

    // ---- drawing -----------------------------------------------------------

    fn field(&self, key: &'static str, label: &'static str) -> TextField {
        TextField::new(&self.inputs[key]).label(label)
    }

    /// A field that starts masked, with its show and hide button.
    fn masked_field(
        &self,
        key: &'static str,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> TextField {
        let shown = self.shown.contains(key);
        self.field(key, label).mono(true).trailing(
            IconButton::new(
                ("show", key.len() as u64),
                if shown {
                    IconName::EyeOff
                } else {
                    IconName::Eye
                },
                if shown { "Hide" } else { "Show" },
            )
            .size(ButtonSize::Sm)
            .on_click(cx.listener(move |this, _, window, cx| this.toggle_shown(key, window, cx))),
        )
    }

    fn password_block(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let shown = self.shown.contains("password");
        let password = self.value("password", cx);
        let estimate = strength::estimate(
            &password,
            &[&self.value("username", cx), &self.title.read(cx).value()],
        );
        let trailing = h_flex()
            .child(
                IconButton::new(
                    "show-password",
                    if shown {
                        IconName::EyeOff
                    } else {
                        IconName::Eye
                    },
                    if shown {
                        "Hide password"
                    } else {
                        "Show password"
                    },
                )
                .size(ButtonSize::Sm)
                .on_click(
                    cx.listener(|this, _, window, cx| this.toggle_shown("password", window, cx)),
                ),
            )
            .child(
                IconButton::new("generate-password", IconName::Refresh, "Generate password")
                    .size(ButtonSize::Sm)
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_generator(window, cx))),
            );
        v_flex()
            .relative()
            .gap(px(tokens::SPACE_2))
            .child(
                self.field("password", "Password")
                    .icon(IconName::Key)
                    .mono(true)
                    .trailing(trailing),
            )
            .when(!password.is_empty(), |block| {
                block.child(
                    StrengthMeter::new(estimate.score)
                        .detail(format!("{} bits", estimate.bits.round())),
                )
            })
            .when_some(self.generator.clone(), |block, generator| {
                // The popover hangs under the password field, over what follows.
                block.child(deferred(
                    div()
                        .id("generator-popover")
                        .absolute()
                        .top(px(78.))
                        .right_0()
                        .w(px(380.))
                        .max_w_full()
                        .p(px(tokens::SPACE_4))
                        .rounded(px(tokens::RADIUS_LG))
                        .border_1()
                        .border_color(color(p.line))
                        .bg(color(p.surface_100))
                        .shadow(shadow(tokens::SHADOW_POP, cx.theme_kind()))
                        .occlude()
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            this.generator = None;
                            cx.notify();
                        }))
                        .child(
                            v_flex()
                                .gap(px(tokens::SPACE_3))
                                .child(
                                    div()
                                        .text_token(tokens::TITLE_MD)
                                        .text_color(color(p.ink))
                                        .child("Generate password"),
                                )
                                .child(generator),
                        ),
                ))
            })
    }

    fn pair_rows(
        &self,
        pairs: &[Pair],
        documents: bool,
        cx: &mut Context<Self>,
    ) -> Vec<gpui_kit::AnyElement> {
        pairs
            .iter()
            .enumerate()
            .map(|(index, pair)| {
                let hidden = pair.hidden;
                h_flex()
                    .items_end()
                    .gap(px(tokens::SPACE_2))
                    .child(div().flex_1().min_w_0().child(
                        TextField::new(&pair.label).label(if documents {
                            "Document"
                        } else {
                            "Label"
                        }),
                    ))
                    .child(div().flex_1().min_w_0().child(
                        TextField::new(&pair.value).label(if documents {
                            "Number"
                        } else {
                            "Value"
                        }),
                    ))
                    .when(!documents, |row| {
                        row.child(
                            v_flex().h(px(44.)).justify_center().child(
                                Toggle::new(("custom-hidden", index))
                                    .label("Hide")
                                    .checked(hidden)
                                    .on_change(cx.listener(move |this, on: &bool, _, cx| {
                                        if let Some(pair) = this.custom.get_mut(index) {
                                            pair.hidden = *on;
                                        }
                                        cx.notify();
                                    })),
                            ),
                        )
                    })
                    .child(
                        v_flex().h(px(44.)).justify_center().child(
                            IconButton::new(
                                (
                                    if documents {
                                        "remove-doc"
                                    } else {
                                        "remove-custom"
                                    },
                                    index,
                                ),
                                IconName::X,
                                "Remove",
                            )
                            .size(ButtonSize::Sm)
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    let pairs = if documents {
                                        &mut this.documents
                                    } else {
                                        &mut this.custom
                                    };
                                    if index < pairs.len() {
                                        pairs.remove(index);
                                    }
                                    cx.notify();
                                },
                            )),
                        ),
                    )
                    .into_any_element()
            })
            .collect()
    }

    fn add_button(
        id: &'static str,
        label: &'static str,
        handler: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        h_flex().child(
            Button::new(id, label)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .icon(IconName::Plus)
                .on_click(cx.listener(move |this, _, window, cx| handler(this, window, cx))),
        )
    }

    fn kind_fields(&self, cx: &mut Context<Self>) -> Vec<gpui_kit::AnyElement> {
        let mut fields: Vec<gpui_kit::AnyElement> = Vec::new();
        let row2 = |left: TextField, right: TextField| {
            h_flex()
                .items_start()
                .gap(px(tokens::SPACE_3))
                .child(div().flex_1().min_w_0().child(left))
                .child(div().flex_1().min_w_0().child(right))
                .into_any_element()
        };
        match self.kind {
            ItemKind::Login => {
                fields.push(
                    self.field("username", "Username or email")
                        .icon(IconName::User)
                        .into_any_element(),
                );
                fields.push(self.password_block(cx).into_any_element());
                fields.push(match &self.totp {
                    None => Button::new("add-totp", "Add two-factor code")
                        .icon(IconName::Clock)
                        .block()
                        .on_click(cx.listener(|this, _, window, cx| this.open_add_totp(window, cx)))
                        .into_any_element(),
                    Some(_) => h_flex()
                        .items_center()
                        .gap(px(tokens::SPACE_3))
                        .h(px(44.))
                        .px(px(tokens::SPACE_3))
                        .rounded(px(tokens::RADIUS_MD))
                        .bg(color(cx.palette().surface_200))
                        .child(
                            crate::components::Icon::new(IconName::Clock)
                                .size(px(18.))
                                .color(color(cx.palette().teal)),
                        )
                        .child(div().flex_1().child("Two-factor code added"))
                        .child(
                            Button::new("remove-totp", "Remove")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.totp = None;
                                    cx.notify();
                                })),
                        )
                        .into_any_element(),
                });
                for (index, site) in self.websites.iter().enumerate() {
                    let mut field = TextField::new(site).label("Website").icon(IconName::Globe);
                    if self.websites.len() > 1 {
                        field = field.trailing(
                            IconButton::new(
                                ("remove-website", index),
                                IconName::X,
                                "Remove website",
                            )
                            .size(ButtonSize::Sm)
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if index < this.websites.len() {
                                        this.websites.remove(index);
                                    }
                                    cx.notify();
                                },
                            )),
                        );
                    }
                    fields.push(field.into_any_element());
                }
                fields.push(
                    Self::add_button("add-website", "Add website", Self::add_website, cx)
                        .into_any_element(),
                );
            }
            ItemKind::Card => {
                fields.push(
                    self.field("cardholder", "Cardholder name")
                        .into_any_element(),
                );
                fields.push(
                    self.field("number", "Card number")
                        .icon(IconName::Card)
                        .mono(true)
                        .into_any_element(),
                );
                let expiry = self
                    .field("expiry", "Expires")
                    .mono(true)
                    .hint("Month and year, like 08/29")
                    .when_some(self.expiry_error.clone(), |field, error| field.error(error));
                fields.push(row2(expiry, self.masked_field("cvv", "Security code", cx)));
                fields.push(self.masked_field("pin", "PIN", cx).into_any_element());
            }
            ItemKind::Identity => {
                fields.push(section_label("Personal", cx).into_any_element());
                fields.push(row2(
                    self.field("first_name", "First name"),
                    self.field("last_name", "Last name"),
                ));
                fields.push(
                    self.field("email", "Email")
                        .icon(IconName::Mail)
                        .into_any_element(),
                );
                fields.push(self.field("phone", "Phone").into_any_element());
                fields.push(section_label("Address", cx).into_any_element());
                fields.push(self.field("line1", "Street").into_any_element());
                fields.push(row2(
                    self.field("city", "City"),
                    self.field("postal_code", "Postal code"),
                ));
                fields.push(row2(
                    self.field("region", "State or region"),
                    self.field("country", "Country"),
                ));
                fields.push(section_label("Work", cx).into_any_element());
                fields.push(row2(
                    self.field("company", "Company"),
                    self.field("job_title", "Job title"),
                ));
                fields.push(section_label("IDs", cx).into_any_element());
                fields.extend(self.pair_rows(&self.documents, true, cx));
                fields.push(
                    Self::add_button("add-document", "Add ID", Self::add_document, cx)
                        .into_any_element(),
                );
            }
            ItemKind::Password => fields.push(self.password_block(cx).into_any_element()),
            ItemKind::Note | ItemKind::Alias | ItemKind::Passkey => {}
        }
        fields
    }

    fn vault_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let current = self
            .vaults
            .iter()
            .find(|vault| vault.id == self.vault)
            .map(|vault| vault.meta.name.clone())
            .unwrap_or_default();
        let view = cx.entity();
        let vaults: Vec<(Uuid, String)> = self
            .vaults
            .iter()
            .map(|vault| (vault.id, vault.meta.name.clone()))
            .collect();
        let chosen = self.vault;
        v_flex()
            .gap(px(6.))
            .child(
                div()
                    .text_token(tokens::SMALL)
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .text_color(color(p.ink_muted))
                    .child("Vault"),
            )
            .child(
                h_flex().child(KitButton::new("form-vault").label(current).dropdown_menu(
                    move |menu, _, _| {
                        vaults.iter().fold(menu, |menu, (id, name)| {
                            let (view, id) = (view.clone(), *id);
                            menu.item(
                                PopupMenuItem::new(name.clone())
                                    .checked(id == chosen)
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            this.vault = id;
                                            cx.notify();
                                        });
                                    }),
                            )
                        })
                    },
                )),
            )
    }
}

impl Render for FormView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let heading = match (self.id, self.kind) {
            (Some(_), kind) => format!("Edit {}", kind_noun(kind)),
            (None, kind) => format!("New {}", kind_noun(kind)),
        };
        let header = h_flex()
            .items_center()
            .gap(px(tokens::SPACE_4))
            .child(ItemIcon::new(self.kind).size(px(40.)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .font_family(DISPLAY_FACE)
                    .text_size(px(24.))
                    .line_height(px(30.))
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .text_color(color(p.ink))
                    .child(heading),
            )
            .child(
                Button::new("form-cancel", "Cancel")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(FormEvent::CancelRequested))),
            )
            .child(
                Button::new("form-save", "Save")
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            );

        let mut body = v_flex()
            .w_full()
            .max_w(px(560.))
            .gap(px(tokens::SPACE_3))
            .child(
                TextField::new(&self.title)
                    .label("Title")
                    .when_some(self.title_error.clone(), |field, error| field.error(error)),
            )
            .children(self.kind_fields(cx))
            .child(TextArea::new(&self.notes).label("Note"));
        if self.kind != ItemKind::Note {
            if !self.custom.is_empty() {
                body = body.child(section_label("Custom fields", cx));
            }
            body = body
                .children(self.pair_rows(&self.custom, false, cx))
                .child(Self::add_button(
                    "add-custom-field",
                    "Add custom field",
                    Self::add_custom_field,
                    cx,
                ));
        }
        body = body.child(self.vault_picker(cx));
        if let Some(failure) = self.failure.clone() {
            body = body.child(error_line(failure, cx));
        }

        div()
            .id("form-pane")
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .child(
                v_flex()
                    .px(px(tokens::SPACE_6))
                    .py(px(tokens::SPACE_5))
                    .gap(px(tokens::SPACE_5))
                    .child(div().w_full().max_w(px(680.)).child(header))
                    .child(body)
                    // Room for the generator popover to hang below the last field.
                    .child(div().h(px(tokens::SPACE_7))),
            )
            .when_some(self.add_totp.clone(), |pane, dialog| pane.child(dialog))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_takes_two_or_four_digit_years() {
        let august = Some(Expiry {
            month: 8,
            year: 2029,
        });
        assert_eq!(parse_expiry("08/29"), Ok(august));
        assert_eq!(parse_expiry(" 8 / 2029 "), Ok(august));
        assert_eq!(parse_expiry(""), Ok(None));
        for bad in ["13/29", "0/29", "08-29", "aug/29", "08/", "08/1999"] {
            assert_eq!(parse_expiry(bad), Err(BadExpiry), "{bad}");
        }
    }

    #[test]
    fn each_kind_starts_from_its_own_empty_content() {
        for kind in [
            ItemKind::Login,
            ItemKind::Card,
            ItemKind::Note,
            ItemKind::Identity,
            ItemKind::Password,
        ] {
            assert_eq!(empty_content(kind).kind(), kind);
        }
    }
}
