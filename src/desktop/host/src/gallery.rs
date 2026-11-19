//! The component gallery: every component, drawn as Kandado Hardware, in the
//! states its preview shows, for side-by-side review with
//! `docs/design-system/project/components`.
//!
//! All data here is made up. The "secrets" are sample text, not real ones.

use gpui_kit::{
    component::{
        TitleBar, WindowExt as _,
        button::Button as KitButton,
        h_flex,
        input::InputState,
        menu::{DropdownMenu as _, PopupMenuItem},
        popover::Popover,
        scroll::ScrollableElement as _,
        v_flex,
    },
    prelude::*,
    *,
};

use crate::{
    components::*,
    theme::{self, KandadoTheme, TextStyles as _, color, tokens},
};

pub struct Gallery {
    search: Entity<InputState>,
    website: Entity<InputState>,
    email: Entity<InputState>,
    password: Entity<InputState>,
    generator: Entity<PasswordGenerator>,
    lock_on_sleep: bool,
    biometrics: bool,
    revealed: bool,
    copied: Option<&'static str>,
    selected_row: usize,
    selected_nav: usize,
    last_event: SharedString,
}

impl Gallery {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = |placeholder: &'static str,
                     value: &'static str,
                     window: &mut Window,
                     cx: &mut Context<Self>| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(value)
            })
        };
        let generator = cx.new(|cx| PasswordGenerator::new(window, cx).with_use_button());
        cx.subscribe(&generator, |this, _, event: &GeneratorEvent, cx| {
            // A real view would copy or fill here. The gallery only says what was asked.
            this.last_event = match event {
                GeneratorEvent::Copy(_) => "Generator: copy requested".into(),
                GeneratorEvent::Use(_) => "Generator: use requested".into(),
            };
            cx.notify();
        })
        .detach();
        // Revealed values are masked again as soon as the window loses focus.
        cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() && this.revealed {
                this.revealed = false;
                cx.notify();
            }
        })
        .detach();

        Self {
            search: input("Search in all vaults", "", window, cx),
            website: input("github.com", "", window, cx),
            email: input("name@example.com", "juan@", window, cx),
            password: input("", "Tq7#vL2m!Ox0-r9P", window, cx),
            generator,
            lock_on_sleep: true,
            biometrics: false,
            revealed: false,
            copied: None,
            selected_row: 0,
            selected_nav: 0,
            last_event: "Nothing pressed yet".into(),
        }
    }

    /// Shows the check mark on one copy button for 1.4 s, as the design does.
    fn mark_copied(&mut self, which: &'static str, cx: &mut Context<Self>) {
        self.copied = Some(which);
        self.last_event = format!("Copy requested: {which}").into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1400))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.copied == Some(which) {
                    this.copied = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

/// One titled panel, like a preview card in the design system.
fn section(title: &'static str, cx: &App, body: impl IntoElement) -> impl IntoElement {
    let p = cx.palette();
    v_flex()
        .gap(px(tokens::SPACE_3))
        .child(
            div()
                .text_token(tokens::LABEL)
                .text_color(color(p.ink_3))
                .child(title.to_uppercase()),
        )
        .child(panel(cx).p(px(tokens::SPACE_5)).child(body))
}

fn row() -> Div {
    h_flex().flex_wrap().items_center().gap(px(tokens::SPACE_3))
}

fn column() -> Div {
    v_flex().gap(px(tokens::SPACE_3))
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let other = cx.theme_kind().other();

        let logos = row()
            .child(Logo::new().size(px(40.)).wordmark())
            .child(Logo::new())
            .child(Logo::new().size(px(20.)));

        let icons = row().children(IconName::ALL.iter().map(|&icon| {
            v_flex()
                .w(px(76.))
                .items_center()
                .gap(px(tokens::SPACE_1))
                .child(Icon::new(icon).size(px(24.)))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(color(p.ink_2))
                        .child(icon.name()),
                )
        }));

        let type_styles = column().children(
            [
                ("display", tokens::DISPLAY, "Your vault, your keys."),
                ("title", tokens::TITLE, "GitHub"),
                ("heading", tokens::HEADING, "Personal"),
                ("subheading", tokens::SUBHEADING, "Generate password"),
                (
                    "body",
                    tokens::BODY,
                    "Kandado locks after 5 minutes of inactivity.",
                ),
                (
                    "body-strong",
                    tokens::BODY_STRONG,
                    "Unlock with Windows Hello",
                ),
                ("key", tokens::KEY, "Copy"),
                ("small", tokens::SMALL, "juan.delacruz@kandado.me"),
                ("label", tokens::LABEL, "USERNAME"),
                ("segment", tokens::SEGMENT, "15 MIN"),
                ("tag", tokens::TAG, "LOGIN"),
                ("secret", tokens::SECRET, "mV7x-Tq2#Lp9-wR4k!Nz8"),
                ("lcd-code", tokens::LCD_CODE, "482 913"),
                ("lcd-text", tokens::LCD_TEXT, "LOCKED · 09:42"),
            ]
            .map(|(name, token, sample)| {
                h_flex()
                    .items_center()
                    .gap(px(tokens::SPACE_4))
                    .child(
                        div()
                            .w(px(96.))
                            .flex_none()
                            .text_token(tokens::SMALL)
                            .text_color(color(p.ink_2))
                            .child(name),
                    )
                    .child(div().text_token(token).child(sample))
            }),
        );

        let buttons = column()
            .child(
                row()
                    .child(Button::new("b-primary", "Unlock").primary())
                    .child(Button::new("b-secondary", "Cancel"))
                    .child(Button::new("b-ghost", "Skip").variant(ButtonVariant::Ghost))
                    .child(Button::new("b-dark", "Lock").variant(ButtonVariant::Dark))
                    .child(Button::new("b-danger", "Delete forever").variant(ButtonVariant::Danger))
                    .child(
                        Button::new("b-quiet", "Move to trash")
                            .variant(ButtonVariant::DangerQuiet)
                            .icon(IconName::Trash),
                    ),
            )
            .child(
                row()
                    .child(
                        Button::new("b-sm", "Create login")
                            .primary()
                            .size(ButtonSize::Sm)
                            .icon(IconName::Plus),
                    )
                    .child(Button::new("b-md", "Share").icon(IconName::Share))
                    .child(Button::new("b-lg", "Unlock").primary().size(ButtonSize::Lg))
                    .child(
                        Button::new("b-trailing", "Personal").trailing_icon(IconName::ChevronDown),
                    )
                    .child(Button::new("b-disabled", "Save").primary().disabled(true)),
            )
            .child(
                row()
                    .child(
                        IconButton::new("ib-copy", IconName::Copy, "Copy password")
                            .size(ButtonSize::Sm),
                    )
                    .child(IconButton::new("ib-eye", IconName::Eye, "Reveal password"))
                    .child(
                        IconButton::new("ib-more", IconName::More, "More actions")
                            .variant(IconButtonVariant::Secondary),
                    )
                    .child(
                        IconButton::new("ib-plus", IconName::Plus, "Create item")
                            .variant(IconButtonVariant::Primary)
                            .size(ButtonSize::Lg),
                    )
                    .child(
                        IconButton::new("ib-done", IconName::Check, "Copied")
                            .size(ButtonSize::Sm)
                            .confirmed(true),
                    )
                    .child(
                        IconButton::new("ib-star", IconName::Star, "Favourite")
                            .variant(IconButtonVariant::Secondary)
                            .checked(true),
                    )
                    .child(
                        IconButton::new("ib-lock", IconName::Lock, "Lock")
                            .variant(IconButtonVariant::Dark),
                    ),
            );

        let surfaces = div().child(
            column()
                .child(section_label("Panel", cx))
                .child(
                    well(cx).p(px(tokens::SPACE_2)).child(
                        row()
                            .child(
                                div()
                                    .px(px(tokens::SPACE_2))
                                    .text_token(tokens::LABEL)
                                    .text_color(color(p.ink_2))
                                    .child("WELL"),
                            )
                            .child(raised(cx).px(px(14.)).py(px(10.)).child("Raised face"))
                            .child(raised(cx).px(px(14.)).py(px(10.)).child("Raised face")),
                    ),
                )
                .child(Banner::new(
                    Tone::Warning,
                    "JSON and CSV are not encrypted. Anyone with the file can read every password.",
                )),
        );

        let lights = column()
            .child(
                row()
                    .gap(px(22.))
                    .child(
                        row()
                            .gap(px(8.))
                            .child(StatusLight::new(LightState::Ok))
                            .child(
                                div()
                                    .text_token(tokens::SEGMENT)
                                    .font_weight(FontWeight::NORMAL)
                                    .child("SYNCED"),
                            ),
                    )
                    .child(
                        row()
                            .gap(px(8.))
                            .child(StatusLight::new(LightState::Warn))
                            .child(
                                div()
                                    .text_token(tokens::SEGMENT)
                                    .font_weight(FontWeight::NORMAL)
                                    .child("3 TO FIX"),
                            ),
                    )
                    .child(
                        row()
                            .gap(px(8.))
                            .child(StatusLight::new(LightState::Off))
                            .child(
                                div()
                                    .text_token(tokens::SEGMENT)
                                    .font_weight(FontWeight::NORMAL)
                                    .child("NOT SET"),
                            ),
                    ),
            )
            .child(
                row()
                    .gap(px(8.))
                    .children(
                        [
                            "Login", "Alias", "Card", "Note", "ID", "Passkey", "Password",
                        ]
                        .map(Tag::new),
                    )
                    .child(Tag::new("This device").tone(TagTone::Go))
                    .child(Tag::new("Differs").tone(TagTone::Warn)),
            )
            .child(
                row()
                    .gap(px(5.))
                    .child(kbd("Ctrl", cx))
                    .child(kbd("Shift", cx))
                    .child(kbd("Space", cx)),
            );

        let lcds = column()
            .max_w(px(620.))
            .child(
                TotpCode::new("lcd-totp", "482913", 18)
                    .copied(self.copied == Some("lcd code"))
                    .on_copy(cx.listener(|this, _, _, cx| this.mark_copied("lcd code", cx))),
            )
            .child(
                LcdStatus::new("Locked · 09:42")
                    .light(LightState::Warn)
                    .label("E2E"),
            )
            .child(LcdStatus::new("github.com · 2 logins").light(LightState::Ok));

        let fields = column()
            .max_w(px(420.))
            .child(SearchBar::new(&self.search).shortcut("Ctrl F"))
            .child(
                TextField::new(&self.website)
                    .label("Website")
                    .icon(IconName::Globe)
                    .hint("Kandado fills this login on this website."),
            )
            .child(
                TextField::new(&self.email)
                    .label("Email")
                    .icon(IconName::Mail)
                    .error("Enter an email, like name@example.com"),
            )
            .child(
                TextField::new(&self.password)
                    .label("Password")
                    .mono(true)
                    .trailing(
                        IconButton::new("tf-generate", IconName::Refresh, "Generate password")
                            .size(ButtonSize::Sm),
                    ),
            );

        let toggles = column()
            .max_w(px(420.))
            .child(
                Toggle::new("t-sleep")
                    .label("Lock on sleep")
                    .description("Kandado locks when your device sleeps.")
                    .checked(self.lock_on_sleep)
                    .on_change(cx.listener(|this, value: &bool, _, cx| {
                        this.lock_on_sleep = *value;
                        cx.notify();
                    })),
            )
            .child(
                Toggle::new("t-hello")
                    .label("Unlock with Windows Hello")
                    .checked(self.biometrics)
                    .on_change(cx.listener(|this, value: &bool, _, cx| {
                        this.biometrics = *value;
                        cx.notify();
                    })),
            )
            .child(
                Toggle::new("t-disabled")
                    .label("Suggest aliases in email fields")
                    .checked(true)
                    .disabled(true),
            );

        let badges = row()
            .child(Badge::new("Breached").tone(Tone::Danger))
            .child(Badge::new("Weak").tone(Tone::Danger))
            .child(Badge::new("Reused").tone(Tone::Warning))
            .child(Badge::new("Missing 2FA").tone(Tone::Warning))
            .child(Badge::new("Healthy").tone(Tone::Teal))
            .child(Badge::new("Shared").tone(Tone::Brass))
            .child(Badge::new("12"));

        let chips = row()
            .child(
                VaultChip::new("v-personal", "Personal")
                    .count(48)
                    .active(true),
            )
            .child(
                VaultChip::new("v-family", "Family")
                    .count(12)
                    .hue(Hue::Coral)
                    .icon(IconName::User),
            )
            .child(
                VaultChip::new("v-work", "Work")
                    .count(31)
                    .hue(Hue::Sky)
                    .icon(IconName::Grid),
            )
            .child(
                VaultChip::new("v-switch", "Personal")
                    .hue(Hue::Teal)
                    .dropdown(true),
            );

        let item_icons = row()
            .children(ItemKind::ALL.map(ItemIcon::new))
            .child(ItemIcon::new(ItemKind::Login).letter("G"))
            .child(ItemIcon::new(ItemKind::Login).size(px(32.)))
            .child(ItemIcon::new(ItemKind::Card).size(px(56.)));

        let rows: [(ItemKind, &str, &str); 5] = [
            (ItemKind::Login, "GitHub", "juan@kandado.me"),
            (ItemKind::Card, "BPI Visa", "•••• 4821"),
            (ItemKind::Note, "Wi-Fi at home", "Router is behind the TV."),
            (ItemKind::Alias, "Shopping", "shop.k3x@kandado.example"),
            (ItemKind::Identity, "Juan Dela Cruz", "Makati, Philippines"),
        ];
        let item_rows = well(cx)
            .flex()
            .flex_col()
            .max_w(px(360.))
            .p(px(6.))
            .gap(px(4.))
            .children(
                rows.into_iter()
                    .enumerate()
                    .map(|(index, (kind, title, subtitle))| {
                        ItemRow::new(("item", index), kind, title)
                            .subtitle(subtitle)
                            .selected(index == self.selected_row)
                            .favorite(index == 0)
                            .when(index == 1, |row| {
                                row.badge(Badge::new("Expiring").tone(Tone::Warning))
                            })
                            .when(kind == ItemKind::Login, |row| row.letter("G"))
                            .on_copy(cx.listener(|this, _, _, cx| this.mark_copied("item row", cx)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_row = index;
                                cx.notify();
                            }))
                    }),
            );

        let secret_fields = div().max_w(px(420.)).child(
            FieldGroup::new()
                .child(
                    SecretField::new("sf-user", "Username")
                        .plain("juan@kandado.me")
                        .icon(IconName::User)
                        .copied(self.copied == Some("username"))
                        .on_copy(cx.listener(|this, _, _, cx| this.mark_copied("username", cx))),
                )
                .child(
                    SecretField::new("sf-password", "Password")
                        .icon(IconName::Key)
                        .revealed(self.revealed.then(|| "Tq7#vL2m!Ox0-r9P".into()))
                        .copied(self.copied == Some("password"))
                        .on_reveal(cx.listener(|this, _, _, cx| {
                            this.revealed = !this.revealed;
                            cx.notify();
                        }))
                        .on_copy(cx.listener(|this, _, _, cx| this.mark_copied("password", cx))),
                )
                .child(
                    TotpCode::new("totp-ok", "482913", 21)
                        .copied(self.copied == Some("code"))
                        .on_copy(cx.listener(|this, _, _, cx| this.mark_copied("code", cx))),
                )
                .child(TotpCode::new("totp-low", "107355", 4))
                .child(
                    SecretField::new("sf-site", "Website")
                        .plain("https://github.com")
                        .icon(IconName::Globe)
                        .link(true),
                ),
        );

        let meters = column()
            .child(StrengthMeter::new(1).detail("Used on 3 sites"))
            .child(StrengthMeter::new(2).detail("52 bits"))
            .child(StrengthMeter::new(3).detail("71 bits"))
            .child(StrengthMeter::new(4).detail("122 bits"));

        let generator = div().w(px(360.)).child(self.generator.clone());

        let toasts = column()
            .items_start()
            .child(Toast::new("toast-copy", "Password copied. Clears in 60 s."))
            .child(
                Toast::new("toast-undo", "Moved to trash")
                    .icon(IconName::Trash)
                    .action(
                        "Undo",
                        cx.listener(|this, _, _, cx| {
                            this.last_event = "Toast: undo requested".into();
                            cx.notify();
                        }),
                    ),
            )
            .child(Toast::new("toast-fail", "Couldn't sync. Retrying.").danger());

        let banners = column()
            .child(
                Banner::new(
                    Tone::Danger,
                    "GitHub was in a breach on 12 Oct 2026. Change this password.",
                )
                .action(
                    Button::new("banner-change", "Change password")
                        .size(ButtonSize::Sm)
                        .variant(ButtonVariant::DangerQuiet),
                ),
            )
            .child(
                Banner::new(
                    Tone::Warning,
                    "You're offline. Changes sync when you reconnect.",
                )
                .action(
                    Button::new("banner-retry", "Retry")
                        .size(ButtonSize::Sm)
                        .variant(ButtonVariant::Ghost),
                ),
            )
            .child(Banner::new(
                Tone::Teal,
                "No weak, reused or breached passwords.",
            ))
            .child(
                Banner::new(Tone::Brass, "Maria D. invited you to the Family vault.").action(
                    Button::new("banner-view", "View")
                        .size(ButtonSize::Sm)
                        .primary(),
                ),
            );

        let nav_entries = [
            (IconName::Grid, "All items"),
            (IconName::Shield, "Monitor"),
            (IconName::Trash, "Trash"),
            (IconName::Settings, "Settings"),
        ];
        let navigation =
            h_flex()
                .items_start()
                .gap(px(tokens::SPACE_5))
                .child(
                    v_flex().w(px(240.)).gap(px(2.)).children(
                        nav_entries
                            .into_iter()
                            .enumerate()
                            .map(|(index, (icon, label))| {
                                NavItem::new(("nav", index), icon, label)
                                    .selected(index == self.selected_nav)
                                    .when(index == 1, |item| {
                                        item.trailing(Badge::new("3").tone(Tone::Danger).no_icon())
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.selected_nav = index;
                                        cx.notify();
                                    }))
                            }),
                    ),
                )
                .child(
                    v_flex().w(px(64.)).items_center().gap(px(2.)).children(
                        nav_entries
                            .into_iter()
                            .enumerate()
                            .map(|(index, (icon, label))| {
                                NavItem::new(("rail", index), icon, label)
                                    .rail(true)
                                    .selected(index == self.selected_nav)
                            }),
                    ),
                );

        let table = Table::new(["Item", "Expires", "Views", ""])
            .row(vec![
                "GitHub".into_any_element(),
                "Today, 09:41".into_any_element(),
                "1 of 1".into_any_element(),
                IconButton::new("link-revoke-1", IconName::Trash, "Revoke link")
                    .size(ButtonSize::Sm)
                    .into_any_element(),
            ])
            .row(vec![
                "Wi-Fi at home".into_any_element(),
                "12 Oct 2026".into_any_element(),
                "0 of 5".into_any_element(),
                IconButton::new("link-revoke-2", IconName::Trash, "Revoke link")
                    .size(ButtonSize::Sm)
                    .into_any_element(),
            ]);

        // Dialog, popover and dropdown menu are GPUI Kit's, restyled by the theme.
        let overlays = row()
            .child(
                Button::new("open-dialog", "Delete forever")
                    .variant(ButtonVariant::Danger)
                    .on_click(|_, window, cx| {
                        window.open_dialog(cx, |dialog, _, _| {
                            dialog
                                .title("Delete this item forever?")
                                .child("This cannot be undone.")
                        });
                    }),
            )
            .child(
                Popover::new("generator-popover")
                    .trigger(KitButton::new("open-popover").label("Generator popover"))
                    .content(|_, _, cx| {
                        div()
                            .w(px(360.))
                            .p(px(tokens::SPACE_4))
                            .text_color(color(cx.palette().ink))
                            .child("The generator opens here, 360 px wide.")
                    }),
            )
            .child(
                KitButton::new("open-menu")
                    .label("Create item")
                    .dropdown_menu(|menu, _, _| {
                        menu.item(PopupMenuItem::new("Login"))
                            .item(PopupMenuItem::new("Card"))
                            .item(PopupMenuItem::new("Note"))
                            .item(PopupMenuItem::new("Identity"))
                            .item(PopupMenuItem::separator())
                            .item(PopupMenuItem::new("Password"))
                    }),
            );

        v_flex()
            .size_full()
            .bg(color(p.body))
            .text_color(color(p.ink))
            .text_token(tokens::BODY)
            .child(
                TitleBar::new().child(
                    h_flex()
                        .flex_1()
                        .justify_between()
                        .items_center()
                        .pr(px(tokens::SPACE_3))
                        .child(Logo::new().size(px(18.)).wordmark())
                        .child(
                            div()
                                .text_token(tokens::SMALL)
                                .text_color(color(p.ink_2))
                                .child(self.last_event.clone()),
                        )
                        .child(
                            Button::new(
                                "toggle-theme",
                                if other == theme::ThemeKind::Enamel {
                                    "Enamel theme"
                                } else {
                                    "Graphite theme"
                                },
                            )
                            .size(ButtonSize::Sm)
                            .variant(ButtonVariant::Ghost)
                            .on_click(move |_, window, cx| theme::apply(other, Some(window), cx)),
                        ),
                ),
            )
            .child(
                div()
                    .id("gallery-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(
                        v_flex()
                            .p(px(tokens::SPACE_5))
                            .gap(px(tokens::SPACE_5))
                            .child(section("Logo", cx, logos))
                            .child(section("Surfaces", cx, surfaces))
                            .child(section("StatusLight, Tag and keycap", cx, lights))
                            .child(section("Lcd", cx, lcds))
                            .child(section("Type", cx, type_styles))
                            .child(section("Icon", cx, icons))
                            .child(section("Key (Button and IconButton)", cx, buttons))
                            .child(section("SearchBar and TextField", cx, fields))
                            .child(section("Switch (Toggle)", cx, toggles))
                            .child(section("Badge", cx, badges))
                            .child(section("VaultChip", cx, chips))
                            .child(section("ItemIcon", cx, item_icons))
                            .child(section("ItemRow", cx, item_rows))
                            .child(section("SecretField and TotpCode", cx, secret_fields))
                            .child(section("StrengthMeter", cx, meters))
                            .child(section("PasswordGenerator", cx, generator))
                            .child(section("Toast", cx, toasts))
                            .child(section("Banner", cx, banners))
                            .child(section("Navigation", cx, navigation))
                            .child(section("Table", cx, table))
                            .child(section("Dialog, popover and dropdown menu", cx, overlays)),
                    ),
            )
    }
}
