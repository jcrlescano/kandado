//! Settings. They replace the vault with a navigation on the left and one
//! page on the right: General (D-43), Security (D-44), Devices (D-49) and
//! Account (D-50); the other pages arrive with the phases that need them.

mod account;
mod autofill;
mod confirm;
mod devices;
mod export;
mod general;
mod import;
pub(crate) mod rows;
mod security;

use gpui_kit::AppContext as _;
use gpui_kit::{
    Context, Entity, EventEmitter, IntoElement, ParentElement, Render, SharedString, Styled,
    Window, component::h_flex, div, px,
};

use self::{
    account::AccountPage, autofill::AutofillPage, devices::DevicesPage, general::GeneralPage,
    security::SecurityPage,
};
pub use self::{export::ExportPage, import::ImportPage};
use super::{Nav, Services, system::sign_out_dialog, with_overlay};
use crate::{
    assets::IconName,
    components::{Button, ButtonSize, ButtonVariant, NavItem, panel},
    theme::{KandadoTheme, TextStyles as _, color, tokens},
};

/// A page of settings.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    General,
    Security,
    Autofill,
    Import,
    Export,
    Devices,
    Account,
}

impl Page {
    pub const ALL: [Page; 7] = [
        Page::General,
        Page::Security,
        Page::Autofill,
        Page::Import,
        Page::Export,
        Page::Devices,
        Page::Account,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::General => "General and appearance",
            Self::Security => "Security",
            Self::Autofill => "Autofill and browser",
            Self::Import => "Import",
            Self::Export => "Export",
            Self::Devices => "Devices",
            Self::Account => "Account",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Self::General => IconName::Settings,
            Self::Security => IconName::Lock,
            Self::Autofill => IconName::Key,
            Self::Import => IconName::Refresh,
            Self::Export => IconName::Share,
            Self::Devices => IconName::Grid,
            Self::Account => IconName::User,
        }
    }
}

struct SigningOut {
    busy: bool,
    failure: Option<SharedString>,
}

pub struct SettingsView {
    services: Services,
    page: Page,
    general: Entity<GeneralPage>,
    security: Entity<SecurityPage>,
    autofill: Entity<AutofillPage>,
    import: Entity<ImportPage>,
    export: Entity<ExportPage>,
    devices: Entity<DevicesPage>,
    account: Entity<AccountPage>,
    signing_out: Option<SigningOut>,
}

impl EventEmitter<Nav> for SettingsView {}

impl SettingsView {
    pub fn new(services: Services, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let general = cx.new(|_| GeneralPage::new(services.clone()));
        let security = cx.new(|_| SecurityPage::new(services.clone()));
        let autofill = cx.new(|cx| AutofillPage::new(services.clone(), cx));
        let import = cx.new(|cx| ImportPage::new(services.clone(), window, cx));
        let export = cx.new(|cx| ExportPage::new(services.clone(), window, cx));
        cx.subscribe(&import, |_, _, nav: &Nav, cx| cx.emit(nav.clone()))
            .detach();
        let devices = cx.new(|cx| DevicesPage::new(services.clone(), cx));
        let account = cx.new(|cx| AccountPage::new(services.clone(), window, cx));
        // Pages ask the window to move (after deleting the account, say).
        for source in [&devices.clone().into_any(), &account.clone().into_any()] {
            let _ = source;
        }
        cx.subscribe(&devices, |_, _, nav: &Nav, cx| cx.emit(nav.clone()))
            .detach();
        cx.subscribe(&account, |_, _, nav: &Nav, cx| cx.emit(nav.clone()))
            .detach();
        Self {
            services,
            page: Page::General,
            general,
            security,
            autofill,
            import,
            export,
            devices,
            account,
            signing_out: None,
        }
    }

    pub fn select(&mut self, page: Page, cx: &mut Context<Self>) {
        if page == Page::Devices && self.page != Page::Devices {
            // Always show who is signed in now, not who was when settings opened.
            self.devices.update(cx, |devices, cx| devices.reload(cx));
        }
        self.page = page;
        cx.notify();
    }

    fn confirm_sign_out(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.signing_out.as_mut() else {
            return;
        };
        if state.busy {
            return;
        }
        state.busy = true;
        state.failure = None;
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let app = services.app.clone();
            let result = services
                .tasks
                .run(async move { app.sign_out().await })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(()) => cx.emit(Nav::Phase(kandado_app::Phase::SignedOut)),
                Err(error) => {
                    if let Some(state) = this.signing_out.as_mut() {
                        state.busy = false;
                        state.failure = Some(super::explain(&error));
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        // The settings menu (Settings-Nav board): a panel with the way back,
        // the pages, and Sign out at the bottom.
        let nav = panel(cx)
            .flex()
            .flex_col()
            .w(px(260.))
            .flex_none()
            .h_full()
            .gap(px(tokens::SPACE_2))
            .p(px(tokens::SPACE_4))
            .child(
                h_flex().child(
                    Button::new("back-to-vault", "← Back to vault")
                        .size(ButtonSize::Sm)
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(Nav::Home))),
                ),
            )
            .child(
                div()
                    .pt(px(10.))
                    .pb(px(6.))
                    .px(px(4.))
                    .text_token(tokens::HEADING)
                    .text_color(color(p.ink))
                    .child("Settings"),
            )
            .children(Page::ALL.into_iter().map(|page| {
                NavItem::new(("settings-nav", page as u64), page.icon(), page.title())
                    .selected(page == self.page)
                    .on_click(cx.listener(move |this, _, _, cx| this.select(page, cx)))
            }))
            .child(div().flex_1())
            .child(
                Button::new("settings-sign-out", "Sign out")
                    .variant(ButtonVariant::DangerQuiet)
                    .block()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.signing_out = Some(SigningOut {
                            busy: false,
                            failure: None,
                        });
                        cx.notify();
                    })),
            );

        let page: gpui_kit::AnyElement = match self.page {
            Page::General => self.general.clone().into_any_element(),
            Page::Security => self.security.clone().into_any_element(),
            Page::Autofill => self.autofill.clone().into_any_element(),
            Page::Import => self.import.clone().into_any_element(),
            Page::Export => self.export.clone().into_any_element(),
            Page::Devices => self.devices.clone().into_any_element(),
            Page::Account => self.account.clone().into_any_element(),
        };
        let main = panel(cx)
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .gap(px(26.))
            .px(px(tokens::SPACE_7))
            .py(px(28.))
            .child(
                div()
                    .text_token(tokens::TITLE)
                    .text_color(color(p.ink))
                    .child(self.page.title()),
            )
            .child(div().w_full().max_w(px(720.)).child(page));

        let dialog = self.signing_out.as_ref().map(|state| {
            sign_out_dialog(
                state.busy,
                state.failure.clone(),
                cx.listener(|this, _, _, cx| {
                    this.signing_out = None;
                    cx.notify();
                }),
                cx.listener(|this, _, _, cx| this.confirm_sign_out(cx)),
                cx,
            )
            .into_any_element()
        });
        with_overlay(
            h_flex()
                .size_full()
                .gap(px(tokens::SPACE_4))
                .child(nav)
                .child(main)
                .into_any_element(),
            dialog,
        )
    }
}

impl SettingsView {
    /// Opens the Import page, on a source if the first-run offer chose one.
    pub fn open_import(&mut self, source: Option<kandado_app::Source>, cx: &mut Context<Self>) {
        self.import
            .update(cx, |import, cx| import.start(source, cx));
        self.select(Page::Import, cx);
    }

    /// For tests: the pages' own state.
    pub fn import_page(&self) -> &Entity<ImportPage> {
        &self.import
    }

    pub fn export_page(&self) -> &Entity<ExportPage> {
        &self.export
    }
}
