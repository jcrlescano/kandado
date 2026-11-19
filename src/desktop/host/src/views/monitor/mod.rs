//! Health (Monitor): D-35 overview (issues, clear), D-36 breaches and exposed
//! passwords, D-37 weak, D-38 reused, D-39 could add two-factor. Plus
//! checking, could not check, and the parts that could not be reached.
//!
//! The page asks `kandado-app` for a [`Health`] check and draws it. Fixing
//! happens in the item's form ("Change", "Add code"), which the shell opens;
//! "Mark as resolved" and "Not for this item" are saved here, and the page
//! checks again.

use gpui_kit::{
    AnyElement, Context, EventEmitter, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Window,
    component::{h_flex, scroll::ScrollableElement as _, v_flex},
    div,
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::monitor::{BreachGroup, Health, HealthItem, Unchecked};
use kandado_model::{
    Secret,
    generator::{self, Options},
};
use kandado_protocol::{HIBP_ATTRIBUTION, TWO_FACTOR_ATTRIBUTION};
use uuid::Uuid;

use super::{Services, date, error_line, explain, heading, items::detail::copied_message};
use crate::{
    assets::IconName,
    components::{
        Banner, Button, ButtonSize, ButtonVariant, Icon, ItemIcon, LightState, StatusLight, Tag,
        TagTone, Tone, lcd, raised, section_label, well,
    },
    theme::{KandadoTheme, TextStyles as _, color, focus_ring, tokens},
    views::items::detail::letter,
};

/// Which part of Health is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Overview,
    /// D-36. `None` picks the first entry.
    Breaches(Option<BreachPick>),
    Weak,
    Reused,
    TwoFactor,
}

/// An entry of the D-36 list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BreachPick {
    Site(String),
    Exposed,
}

/// One D-35 tile: what it counts and where it leads.
struct TileSpec {
    id: &'static str,
    icon: IconName,
    label: &'static str,
    count: usize,
    sub: String,
    /// Lit orange when the count is not zero; two-factor is only a suggestion.
    warn: bool,
    screen: Screen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorEvent {
    /// Open this item's form to change its password or add a code.
    Edit(Uuid),
    Toast(SharedString),
}

enum Load {
    Checking,
    Ready(Health),
    Failed(SharedString),
}

pub struct MonitorPage {
    services: Services,
    load: Load,
    screen: Screen,
    /// D-37's replacement password: shown on the LCD until a new one or a copy.
    replacement: Option<Secret>,
    working: bool,
    failure: Option<SharedString>,
}

impl EventEmitter<MonitorEvent> for MonitorPage {}

const WHAT_TO_DO: [&str; 3] = [
    "Change your password on the site. Kandado can generate one.",
    "If you used the same password anywhere else, change it there too. Kandado lists those under Reused.",
    "Turn on two-factor for the site if it offers it.",
];

impl MonitorPage {
    pub fn new(services: Services, cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            services,
            load: Load::Checking,
            screen: Screen::Overview,
            replacement: None,
            working: false,
            failure: None,
        };
        page.check(cx);
        page
    }

    pub fn health(&self) -> Option<&Health> {
        match &self.load {
            Load::Ready(health) => Some(health),
            _ => None,
        }
    }

    pub fn checking(&self) -> bool {
        matches!(self.load, Load::Checking)
    }

    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    pub fn show(&mut self, screen: Screen, cx: &mut Context<Self>) {
        if screen == Screen::Weak && self.replacement.is_none() {
            self.new_replacement();
        }
        self.screen = screen;
        cx.notify();
    }

    /// Runs the check again ("Check again", and after a fix).
    pub fn check(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.load, Load::Ready(_)) {
            self.load = Load::Checking;
        }
        self.working = true;
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let result = services
                .tasks
                .run(async move { app.check_health().await })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.working = false;
                this.load = match result {
                    Ok(health) => Load::Ready(health),
                    Err(error) => Load::Failed(explain(&error)),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// D-36: every affected login stops counting this breach.
    pub fn resolve(&mut self, breach: &str, cx: &mut Context<Self>) {
        let Some(group) = self
            .health()
            .and_then(|h| h.breaches.iter().find(|g| g.name == breach))
            .cloned()
        else {
            return;
        };
        let app = &self.services.app;
        let result = group
            .items
            .iter()
            .try_for_each(|item| app.resolve_breach(item.id, &group.name));
        match result {
            Ok(()) => {
                cx.emit(MonitorEvent::Toast(
                    format!("{} marked as resolved.", group.title).into(),
                ));
                self.check(cx);
            }
            Err(error) => {
                self.failure = Some(explain(&error));
                cx.notify();
            }
        }
    }

    /// D-39 "Not for this item".
    pub fn dismiss_two_factor(&mut self, item: Uuid, cx: &mut Context<Self>) {
        match self.services.app.dismiss_two_factor(item) {
            Ok(()) => self.check(cx),
            Err(error) => {
                self.failure = Some(explain(&error));
                cx.notify();
            }
        }
    }

    fn new_replacement(&mut self) {
        self.replacement = generator::generate(&Options::default())
            .ok()
            .map(|generated| generated.password);
    }

    fn copy_replacement(&mut self, cx: &mut Context<Self>) {
        let Some(password) = &self.replacement else {
            return;
        };
        match self
            .services
            .app
            .copy_text("Password", password.expose(), true)
        {
            Ok(copied) => cx.emit(MonitorEvent::Toast(copied_message(&copied))),
            Err(error) => self.failure = Some(explain(&error)),
        }
        cx.notify();
    }

    fn vault_name(&self, id: Uuid) -> String {
        self.services
            .app
            .vaults()
            .unwrap_or_default()
            .into_iter()
            .find(|vault| vault.id == id)
            .map(|vault| vault.meta.name)
            .unwrap_or_default()
    }

    // ---- parts ------------------------------------------------------------

    fn back(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div().child(
            Button::new("health-back", "Health")
                .size(ButtonSize::Sm)
                .icon(IconName::ArrowLeft)
                .on_click(cx.listener(|this, _, _, cx| this.show(Screen::Overview, cx))),
        )
    }

    fn title(&self, text: &str, sub: String, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        v_flex()
            .gap(px(6.))
            .child(heading(text.to_owned(), cx))
            .child(
                div()
                    .text_size(px(15.))
                    .text_color(color(p.ink_2))
                    .child(SharedString::from(sub)),
            )
    }

    /// An item as a raised row: badge, title and username with vault, then `end`.
    fn row(
        &self,
        item: &HealthItem,
        end: Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = cx.palette();
        let mut icon = ItemIcon::new(item.kind).size(px(32.));
        if let Some(letter) = letter(item.kind, &item.title) {
            icon = icon.letter(letter);
        }
        let vault = self.vault_name(item.vault_id);
        let sub = [item.subtitle.as_str(), vault.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" \u{b7} ");
        raised(cx)
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(tokens::SPACE_3))
            .py(px(10.))
            .px(px(tokens::SPACE_3))
            .child(icon)
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(180.))
                    .child(
                        div()
                            .truncate()
                            .font_weight(FontWeight::BOLD)
                            .text_color(color(p.ink))
                            .child(SharedString::from(if item.title.is_empty() {
                                "Untitled".to_owned()
                            } else {
                                item.title.clone()
                            })),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_token(tokens::SMALL)
                            .text_color(color(p.ink_2))
                            .child(SharedString::from(sub)),
                    ),
            )
            .children(end)
    }

    fn change(
        &self,
        id: Uuid,
        label: &'static str,
        go: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Button::new(("health-change", id.as_u128() as usize), label)
            .size(ButtonSize::Sm)
            .variant(if go {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            })
            .on_click(cx.listener(move |_, _, _, cx| cx.emit(MonitorEvent::Edit(id))))
            .into_any_element()
    }

    fn mono(&self, text: String, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        div()
            .font_family(tokens::FONT_MONO)
            .text_size(px(11.))
            .text_color(color(p.ink_2))
            .child(SharedString::from(text))
            .into_any_element()
    }

    /// Five bars, lit to the strength score (D-37).
    fn bars(&self, score: u8, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let lit = usize::from(score) + 1;
        h_flex()
            .gap(px(3.))
            .children((0..5).map(|bar| {
                div()
                    .w(px(16.))
                    .h(px(6.))
                    .rounded(px(2.))
                    .bg(color(if bar < lit {
                        if score <= 1 { p.led_warn } else { p.led_ok }
                    } else {
                        p.well
                    }))
            }))
            .into_any_element()
    }

    fn unchecked_banner(&self, health: &Health) -> Option<AnyElement> {
        if health.unchecked.is_empty() {
            return None;
        }
        let parts: Vec<&str> = health
            .unchecked
            .iter()
            .map(|part| match part {
                Unchecked::Passwords => "exposed passwords",
                Unchecked::Breaches => "breached sites",
                Unchecked::TwoFactor => "two-factor sites",
            })
            .collect();
        Some(
            Banner::new(
                Tone::Warning,
                format!(
                    "Kandado can\u{2019}t reach the server, so {} were not checked. Weak and reused \
                     passwords were.",
                    parts.join(", ")
                ),
            )
            .icon(IconName::Offline)
            .into_any_element(),
        )
    }

    fn attribution(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        div()
            .text_token(tokens::SMALL)
            .text_color(color(p.ink_3))
            .child(format!(
                "{HIBP_ATTRIBUTION} (CC BY 4.0). {TWO_FACTOR_ATTRIBUTION}."
            ))
    }

    // ---- D-35 overview ----------------------------------------------------

    fn tile(&self, tile: TileSpec, cx: &mut Context<Self>) -> impl IntoElement {
        let TileSpec {
            id,
            icon,
            label,
            count,
            sub,
            warn,
            screen,
        } = tile;
        let p = cx.palette();
        raised(cx)
            .id(id)
            .flex()
            .flex_col()
            .gap(px(tokens::SPACE_2))
            .flex_grow(1.)
            .flex_basis(px(200.))
            .min_w(px(200.))
            .p(px(tokens::SPACE_4))
            .cursor_pointer()
            .tab_index(0)
            .focus_visible(|tile| tile.shadow(focus_ring(p, p.panel)))
            .on_click(cx.listener(move |this, _, _, cx| this.show(screen.clone(), cx)))
            .child(
                h_flex()
                    .items_center()
                    .gap(px(tokens::SPACE_2))
                    .child(Icon::new(icon).size(px(18.)).color(color(p.ink_2)))
                    .child(section_label(label, cx).flex_1())
                    .child(StatusLight::new(if warn && count > 0 {
                        LightState::Warn
                    } else {
                        LightState::Off
                    })),
            )
            .child(
                div()
                    .font_family(tokens::FONT_MONO)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(px(34.))
                    .line_height(px(36.))
                    .text_color(color(p.ink))
                    .child(count.to_string()),
            )
            .child(
                div()
                    .text_token(tokens::SMALL)
                    .text_color(color(p.ink_2))
                    .child(SharedString::from(sub)),
            )
    }

    fn overview(&self, health: &Health, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let clear = health.to_fix() == 0;
        let checked = {
            let at = time::OffsetDateTime::from_unix_timestamp(health.checked_at)
                .unwrap_or(time::OffsetDateTime::UNIX_EPOCH)
                .to_offset(time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC));
            format!("CHECKED {:02}:{:02}", at.hour(), at.minute())
        };
        let summary = if clear {
            format!("ALL CLEAR \u{b7} {} ITEMS CHECKED", health.items_checked)
        } else {
            let mut parts = vec![format!("{} TO FIX", health.to_fix())];
            let breaches = health.breach_count();
            if breaches > 0 {
                parts.push(format!(
                    "{breaches} {}",
                    if breaches == 1 { "BREACH" } else { "BREACHES" }
                ));
            }
            if !health.reused.is_empty() {
                parts.push(format!(
                    "{} REUSED",
                    health.reused.iter().map(Vec::len).sum::<usize>()
                ));
            }
            if !health.weak.is_empty() {
                parts.push(format!("{} WEAK", health.weak.len()));
            }
            parts.join(" \u{b7} ")
        };
        let readout = lcd(cx)
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(tokens::SPACE_3))
            .py(px(14.))
            .px(px(18.))
            .child(StatusLight::new(if clear {
                LightState::Ok
            } else {
                LightState::Warn
            }))
            .child(
                div()
                    .flex_1()
                    .min_w(px(200.))
                    .font_family(tokens::FONT_MONO)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(px(15.))
                    .text_color(color(p.lcd_ink))
                    .child(SharedString::from(summary)),
            )
            .child(
                div()
                    .text_token(tokens::LABEL)
                    .text_color(color(p.lcd_label))
                    .child(SharedString::from(checked)),
            );

        let first_breach = health
            .breaches
            .first()
            .map(|g| format!("{}, {}", g.domain, date(g.breach_date)))
            .or_else(|| (!health.exposed.is_empty()).then(|| "Exposed passwords".to_owned()))
            .unwrap_or_else(|| "None found".to_owned());
        let biggest_reuse = health
            .reused
            .first()
            .map(|g| format!("One password on {} items", g.len()))
            .unwrap_or_else(|| "Every password is its own".to_owned());
        let two_factor = health.two_factor.len();
        let body: AnyElement = if clear {
            well(cx)
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(tokens::SPACE_5))
                .p(px(20.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(44.))
                        .rounded(px(12.))
                        .bg(color(p.green))
                        .child(
                            Icon::new(IconName::Check)
                                .size(px(22.))
                                .color(color(p.on_green)),
                        ),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(240.))
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .text_size(px(17.))
                                .text_color(color(p.ink))
                                .child("Nothing to fix"),
                        )
                        .child(
                            div()
                                .text_token(tokens::SMALL)
                                .text_color(color(p.ink_2))
                                .child(
                                    "No breaches, no reused or weak passwords. Kandado checks \
                                     again whenever you open Health.",
                                ),
                        ),
                )
                .when(two_factor > 0, |row| {
                    row.child(
                        Button::new(
                            "health-clear-2fa",
                            format!("{two_factor} could add two-factor"),
                        )
                        .icon(IconName::Shield)
                        .on_click(cx.listener(|this, _, _, cx| this.show(Screen::TwoFactor, cx))),
                    )
                })
                .into_any_element()
        } else {
            h_flex()
                .flex_wrap()
                .gap(px(tokens::SPACE_3))
                .child(self.tile(
                    TileSpec {
                        id: "tile-breaches",
                        icon: IconName::Breach,
                        label: "Breaches",
                        count: health.breach_count(),
                        sub: first_breach,
                        warn: true,
                        screen: Screen::Breaches(None),
                    },
                    cx,
                ))
                .child(self.tile(
                    TileSpec {
                        id: "tile-reused",
                        icon: IconName::Copy,
                        label: "Reused",
                        count: health.reused.iter().map(Vec::len).sum(),
                        sub: biggest_reuse,
                        warn: true,
                        screen: Screen::Reused,
                    },
                    cx,
                ))
                .child(self.tile(
                    TileSpec {
                        id: "tile-weak",
                        icon: IconName::Alert,
                        label: "Weak",
                        count: health.weak.len(),
                        sub: "Easy to guess".to_owned(),
                        warn: true,
                        screen: Screen::Weak,
                    },
                    cx,
                ))
                .child(self.tile(
                    TileSpec {
                        id: "tile-2fa",
                        icon: IconName::Shield,
                        label: "Could add two-factor",
                        count: two_factor,
                        sub: "Sites that offer it".to_owned(),
                        warn: false,
                        screen: Screen::TwoFactor,
                    },
                    cx,
                ))
                .into_any_element()
        };
        v_flex()
            .gap(px(20.))
            .child(
                h_flex()
                    .flex_wrap()
                    .items_end()
                    .gap(px(tokens::SPACE_3))
                    .child(div().flex_1().min_w(px(240.)).child(self.title(
                        "Health",
                        "Kandado checks your passwords on this device. Breach checks send only \
                         the first five characters of a hash."
                            .to_owned(),
                        cx,
                    )))
                    .child(
                        Button::new(
                            "health-check-again",
                            if self.working { "Checking\u{2026}" } else { "Check again" },
                        )
                        .icon(IconName::Refresh)
                        .disabled(self.working)
                        .on_click(cx.listener(|this, _, _, cx| this.check(cx))),
                    ),
            )
            .when_some(self.unchecked_banner(health), |page, banner| page.child(banner))
            .child(readout)
            .child(body)
            .child(self.attribution(cx))
            .into_any_element()
    }

    // ---- D-36 breaches ----------------------------------------------------

    fn breaches(
        &self,
        health: &Health,
        pick: Option<&BreachPick>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = cx.palette();
        let pick = pick.cloned().or_else(|| {
            health
                .breaches
                .first()
                .map(|g| BreachPick::Site(g.name.clone()))
                .or_else(|| (!health.exposed.is_empty()).then_some(BreachPick::Exposed))
                .or_else(|| {
                    health
                        .resolved
                        .first()
                        .map(|g| BreachPick::Site(g.name.clone()))
                })
        });

        let entry = |id: String,
                     title: String,
                     sub: String,
                     open: bool,
                     selected: bool,
                     target: BreachPick,
                     cx: &mut Context<Self>| {
            div()
                .id(SharedString::from(format!("breach-{id}")))
                .flex()
                .items_center()
                .gap(px(10.))
                .py(px(8.))
                .px(px(10.))
                .rounded(px(tokens::RADIUS_KEY))
                .cursor_pointer()
                .when(selected, |row| row.bg(color(p.raised)))
                .hover(|row| row.bg(color(p.raised)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.show(Screen::Breaches(Some(target.clone())), cx)
                }))
                .child(StatusLight::new(if open {
                    LightState::Warn
                } else {
                    LightState::Off
                }))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .truncate()
                                .font_weight(FontWeight::BOLD)
                                .text_color(color(p.ink))
                                .child(SharedString::from(title)),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_token(tokens::SMALL)
                                .text_color(color(p.ink_2))
                                .child(SharedString::from(sub)),
                        ),
                )
                .child(
                    Tag::new(if open { "Open" } else { "Resolved" }).tone(if open {
                        TagTone::Warn
                    } else {
                        TagTone::Plain
                    }),
                )
        };
        let count = |n: usize| format!("{n} {}", if n == 1 { "item" } else { "items" });
        let mut list = v_flex().gap(px(4.));
        for group in &health.breaches {
            list = list.child(entry(
                group.name.clone(),
                group.domain.clone(),
                format!(
                    "{} \u{b7} {}",
                    date(group.breach_date),
                    count(group.items.len())
                ),
                true,
                pick == Some(BreachPick::Site(group.name.clone())),
                BreachPick::Site(group.name.clone()),
                cx,
            ));
        }
        if !health.exposed.is_empty() {
            list = list.child(entry(
                "exposed".to_owned(),
                "Exposed passwords".to_owned(),
                format!("Found in leaks \u{b7} {}", count(health.exposed.len())),
                true,
                pick == Some(BreachPick::Exposed),
                BreachPick::Exposed,
                cx,
            ));
        }
        for group in &health.resolved {
            list = list.child(entry(
                group.name.clone(),
                group.domain.clone(),
                format!("{} \u{b7} resolved", date(group.breach_date)),
                false,
                pick == Some(BreachPick::Site(group.name.clone())),
                BreachPick::Site(group.name.clone()),
                cx,
            ));
        }

        let detail: AnyElement = match &pick {
            None => v_flex()
                .gap(px(tokens::SPACE_3))
                .child(heading("No breaches", cx))
                .child(div().text_color(color(p.ink_2)).child(
                    "None of your sites is in a known breach, and no password is in a known leak.",
                ))
                .into_any_element(),
            Some(BreachPick::Exposed) => self.exposed_detail(health, cx),
            Some(BreachPick::Site(name)) => {
                let open = health.breaches.iter().find(|g| &g.name == name);
                let group = open.or_else(|| health.resolved.iter().find(|g| &g.name == name));
                match group {
                    Some(group) => self.breach_detail(group, open.is_some(), cx),
                    None => div().into_any_element(),
                }
            }
        };
        v_flex()
            .gap(px(tokens::SPACE_4))
            .child(self.back(cx))
            .child(
                h_flex()
                    .items_start()
                    .gap(px(tokens::SPACE_5))
                    .child(
                        v_flex()
                            .w(px(300.))
                            .flex_none()
                            .gap(px(tokens::SPACE_3))
                            .child(
                                div()
                                    .text_token(tokens::HEADING)
                                    .text_color(color(p.ink))
                                    .child("Breaches"),
                            )
                            .child(well(cx).p(px(6.)).child(list)),
                    )
                    .child(v_flex().flex_1().min_w_0().gap(px(20.)).child(detail)),
            )
            .child(self.attribution(cx))
            .into_any_element()
    }

    fn badge(&self, icon: IconName, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(52.))
            .rounded(px(14.))
            .bg(color(p.danger))
            .child(Icon::new(icon).size(px(26.)).color(color(p.on_danger)))
    }

    fn breach_detail(&self, group: &BreachGroup, open: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let name = group.name.clone();
        let site = format!("https://{}", group.domain);
        let mut affected = v_flex().gap(px(6.));
        for item in &group.items {
            let end = if open {
                vec![
                    Tag::new("Password unchanged")
                        .tone(TagTone::Warn)
                        .into_any_element(),
                    self.change(item.id, "Change password", true, cx),
                ]
            } else {
                vec![Tag::new("Resolved").into_any_element()]
            };
            affected = affected.child(self.row(item, end, cx));
        }
        v_flex()
            .gap(px(20.))
            .child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(14.))
                    .child(self.badge(IconName::Breach, cx))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(200.))
                            .child(heading(format!("{} breach", group.title), cx))
                            .child(
                                section_label(
                                    format!("Breach dated {}", date(group.breach_date)),
                                    cx,
                                )
                                .mt(px(4.)),
                            ),
                    )
                    .child(
                        Button::new("breach-open-site", format!("Open {}", group.domain))
                            .icon(IconName::External)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if kandado_app::platform::open_url(&site).is_err() {
                                    this.failure = Some("Kandado could not open the site.".into());
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .when(!group.data_classes.is_empty(), |detail| {
                detail.child(
                    lcd(cx)
                        .flex()
                        .flex_col()
                        .gap(px(tokens::SPACE_2))
                        .py(px(tokens::SPACE_5))
                        .px(px(18.))
                        .child(
                            div()
                                .text_token(tokens::LABEL)
                                .text_color(color(p.lcd_label))
                                .child("EXPOSED IN THIS BREACH"),
                        )
                        .child(
                            div()
                                .font_family(tokens::FONT_MONO)
                                .font_weight(FontWeight::MEDIUM)
                                .text_size(px(16.))
                                .text_color(color(p.lcd_warn))
                                .child(SharedString::from(
                                    group.data_classes.join(" \u{b7} ").to_uppercase(),
                                )),
                        ),
                )
            })
            .child(section_label("Affected items", cx))
            .child(well(cx).p(px(6.)).child(affected))
            .when(open, |detail| {
                detail
                    .child(section_label("What to do", cx))
                    .child(
                        well(cx)
                            .py(px(tokens::SPACE_3))
                            .px(px(tokens::SPACE_5))
                            .child(v_flex().gap(px(tokens::SPACE_2)).children(
                                WHAT_TO_DO.iter().enumerate().map(|(index, line)| {
                                    div()
                                        .text_color(color(p.ink))
                                        .child(SharedString::from(format!("{}. {line}", index + 1)))
                                }),
                            )),
                    )
                    .child(
                        h_flex().justify_end().child(
                            Button::new("breach-resolve", "Mark as resolved")
                                .icon(IconName::Check)
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.resolve(&name, cx)),
                                ),
                        ),
                    )
            })
            .into_any_element()
    }

    fn exposed_detail(&self, health: &Health, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = v_flex().gap(px(6.));
        for exposed in &health.exposed {
            let seen = exposed.times_seen;
            let text = format!("SEEN {seen} {}", if seen == 1 { "TIME" } else { "TIMES" });
            let end = vec![
                self.mono(text, cx),
                self.change(exposed.item.id, "Change password", true, cx),
            ];
            rows = rows.child(self.row(&exposed.item, end, cx));
        }
        v_flex()
            .gap(px(20.))
            .child(
                h_flex()
                    .items_center()
                    .gap(px(14.))
                    .child(self.badge(IconName::Alert, cx))
                    .child(self.title(
                        "Exposed passwords",
                        "These passwords are in lists of leaked passwords. Anyone can try them, \
                         on any site."
                            .to_owned(),
                        cx,
                    )),
            )
            .child(well(cx).p(px(6.)).child(rows))
            .into_any_element()
    }

    // ---- D-37, D-38, D-39 ---------------------------------------------------

    fn empty(&self, text: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        well(cx)
            .flex()
            .items_center()
            .gap(px(tokens::SPACE_3))
            .py(px(tokens::SPACE_3))
            .px(px(tokens::SPACE_5))
            .child(StatusLight::new(LightState::Ok))
            .child(div().text_color(color(p.ink_2)).child(text))
            .into_any_element()
    }

    fn weak(&self, health: &Health, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let n = health.weak.len();
        let mut rows = v_flex().gap(px(6.));
        for weak in &health.weak {
            let end = vec![
                self.bars(weak.score, cx),
                self.mono(format!("WEAK \u{b7} {} BITS", weak.bits.round() as i64), cx),
                self.change(weak.item.id, "Change", true, cx),
            ];
            rows = rows.child(self.row(&weak.item, end, cx));
        }
        let replacement = self.replacement.as_ref().map(|p| p.expose().to_owned());
        v_flex()
            .gap(px(20.))
            .child(self.back(cx))
            .child(self.title(
                "Weak passwords",
                match n {
                    0 => "None of your passwords is easy to guess.".to_owned(),
                    1 => "1 password is easy to guess. Replace it with a generated one.".to_owned(),
                    _ => format!(
                        "{n} passwords are easy to guess. Replace them with generated ones."
                    ),
                },
                cx,
            ))
            .child(if n == 0 {
                self.empty("No weak passwords.", cx)
            } else {
                well(cx).p(px(6.)).child(rows).into_any_element()
            })
            .when_some(replacement.filter(|_| n > 0), |page, password| {
                page.child(section_label("A replacement, ready to use", cx))
                    .child(
                        lcd(cx)
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(tokens::SPACE_5))
                            .py(px(tokens::SPACE_5))
                            .px(px(18.))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w(px(240.))
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .text_token(tokens::LABEL)
                                            .text_color(color(p.lcd_label))
                                            .child("STRONG \u{b7} GENERATED"),
                                    )
                                    .child(
                                        div()
                                            .font_family(tokens::FONT_MONO)
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_size(px(22.))
                                            .line_height(px(28.))
                                            .text_color(color(p.lcd_ink))
                                            .child(SharedString::from(password)),
                                    ),
                            )
                            .child(
                                Button::new("weak-new", "New")
                                    .size(ButtonSize::Sm)
                                    .icon(IconName::Refresh)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.new_replacement();
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("weak-copy", "Copy")
                                    .size(ButtonSize::Sm)
                                    .icon(IconName::Copy)
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.copy_replacement(cx)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .text_token(tokens::SMALL)
                            .text_color(color(p.ink_2))
                            .child(
                                "Change it on the site first, then save it here. Kandado keeps the \
                             old one in the item\u{2019}s history.",
                            ),
                    )
            })
            .into_any_element()
    }

    fn reused(&self, health: &Health, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let mut page = v_flex().gap(px(20.)).child(self.back(cx)).child(
            self.title(
                "Reused passwords",
                "If one of these sites is breached, the others are open too. Give each its own \
                 password."
                    .to_owned(),
                cx,
            ),
        );
        if health.reused.is_empty() {
            return page
                .child(self.empty("No password is used twice.", cx))
                .into_any_element();
        }
        for group in &health.reused {
            let mut rows = v_flex().gap(px(6.));
            for item in group {
                let end = vec![self.change(item.id, "Change", false, cx)];
                rows = rows.child(self.row(item, end, cx));
            }
            page = page.child(
                v_flex()
                    .gap(px(10.))
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(10.))
                            .child(StatusLight::new(LightState::Warn))
                            .child(
                                section_label(
                                    format!("One password \u{b7} {} items", group.len()),
                                    cx,
                                )
                                .flex_1(),
                            )
                            .child(
                                div()
                                    .font_family(tokens::FONT_MONO)
                                    .text_size(px(13.))
                                    .text_color(color(p.ink_2))
                                    .child(crate::components::MASK),
                            ),
                    )
                    .child(well(cx).p(px(6.)).child(rows)),
            );
        }
        page.child(
            div()
                .text_token(tokens::SMALL)
                .text_color(color(p.ink_2))
                .child(
                    "Change one of them and the group clears. Kandado compares passwords on this \
                     device only.",
                ),
        )
        .into_any_element()
    }

    fn two_factor(&self, health: &Health, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = v_flex().gap(px(6.));
        for entry in &health.two_factor {
            let id = entry.item.id;
            let end = vec![
                Button::new(
                    ("two-factor-no", id.as_u128() as usize),
                    "Not for this item",
                )
                .size(ButtonSize::Sm)
                .variant(ButtonVariant::Ghost)
                .on_click(cx.listener(move |this, _, _, cx| this.dismiss_two_factor(id, cx)))
                .into_any_element(),
                Button::new(("two-factor-add", id.as_u128() as usize), "Add code")
                    .size(ButtonSize::Sm)
                    .primary()
                    .icon(IconName::Plus)
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(MonitorEvent::Edit(id))))
                    .into_any_element(),
            ];
            rows = rows.child(self.row(&entry.item, end, cx));
        }
        v_flex()
            .gap(px(20.))
            .child(self.back(cx))
            .child(
                self.title(
                    "Could add two-factor",
                    "These sites offer two-factor codes. Add one and Kandado fills it after the \
                 password."
                        .to_owned(),
                    cx,
                ),
            )
            .child(if health.two_factor.is_empty() {
                self.empty("Every login whose site offers codes has one.", cx)
            } else {
                well(cx).p(px(6.)).child(rows).into_any_element()
            })
            .child(
                Banner::new(
                    Tone::Neutral,
                    format!(
                        "Kandado knows which sites offer two-factor from a public list it \
                         downloads; it never asks the sites. {TWO_FACTOR_ATTRIBUTION}."
                    ),
                )
                .icon(IconName::Info),
            )
            .into_any_element()
    }
}

impl Render for MonitorPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let body: AnyElement = match &self.load {
            Load::Checking => v_flex()
                .gap(px(20.))
                .child(self.title(
                    "Health",
                    "Kandado checks your passwords on this device.".to_owned(),
                    cx,
                ))
                .child(
                    lcd(cx)
                        .flex()
                        .items_center()
                        .gap(px(tokens::SPACE_3))
                        .py(px(14.))
                        .px(px(18.))
                        .child(StatusLight::new(LightState::Warn))
                        .child(
                            div()
                                .font_family(tokens::FONT_MONO)
                                .text_size(px(15.))
                                .text_color(color(p.lcd_ink))
                                .child("CHECKING\u{2026}"),
                        ),
                )
                .into_any_element(),
            Load::Failed(message) => v_flex()
                .gap(px(tokens::SPACE_3))
                .items_start()
                .child(self.title("Health", String::new(), cx))
                .child(error_line(message.clone(), cx))
                .child(
                    Button::new("health-retry", "Try again")
                        .on_click(cx.listener(|this, _, _, cx| this.check(cx))),
                )
                .into_any_element(),
            Load::Ready(health) => {
                let health = health.clone();
                match self.screen.clone() {
                    Screen::Overview => self.overview(&health, cx),
                    Screen::Breaches(pick) => self.breaches(&health, pick.as_ref(), cx),
                    Screen::Weak => self.weak(&health, cx),
                    Screen::Reused => self.reused(&health, cx),
                    Screen::TwoFactor => self.two_factor(&health, cx),
                }
            }
        };
        v_flex()
            .id("health")
            .size_full()
            .overflow_y_scrollbar()
            .gap(px(20.))
            .py(px(22.))
            .px(px(28.))
            .when_some(self.failure.clone(), |page, failure| {
                page.child(error_line(failure, cx))
            })
            .child(body)
    }
}
