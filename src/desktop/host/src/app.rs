//! The window: a title bar over whichever screen the app is on. It owns the
//! flow between screens, auto-lock, the lock shortcut, the global shortcut,
//! staying in the tray when closed, and the menu of the tray icon (D-53).

use std::{sync::mpsc, time::Duration};

use gpui_kit::AppContext as _;
use gpui_kit::TestSupportExt as _;
use gpui_kit::{
    Context, Entity, Global, InteractiveElement as _, IntoElement, KeyBinding, ParentElement,
    Render, Styled, Subscription, Window, actions,
    component::{TitleBar, h_flex, v_flex},
    div,
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::{Phase, lock::LockReason, platform::HotkeyControl};
use kandado_model::generator::{self, Options};

use crate::{
    assets::IconName,
    components::{Banner, Button, ButtonSize, IconButton, Logo, SearchBar, Tone},
    shell::{ShellEvent, VaultShell},
    theme::{self, KandadoTheme, TextStyles as _, color, tokens},
    views::{
        Nav, Services,
        settings::SettingsView,
        start::{MasterPasswordView, RecoverView, RecoveryPhraseView, SignInView, UnlockView},
    },
};

actions!(kandado, [LockNow, OpenSettings]);

/// How often the window checks for system events, the global shortcut and
/// the tray menu.
const TICK: Duration = Duration::from_millis(200);
/// Idle time is checked every this many ticks.
const IDLE_EVERY: u32 = 25;
/// The design's title bar (`.kt-titlebar`): tall enough for the 40 px search
/// field with room above and below. The library default (34 px) is shorter than
/// the field, which then spilled over the bar.
const TITLE_BAR_HEIGHT: f32 = 48.;
/// Three 35 px window controls on the right, less the title bar's own padding.
const TITLE_BAR_INSET: f32 = 3. * 35. - 12.;

/// Turns the global shortcut on and off; set at startup when it is watched.
pub struct Hotkey(pub HotkeyControl);
impl Global for Hotkey {}

/// What the tray menu or the global shortcut asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Show,
    QuickSearch,
    GeneratePassword,
    Lock,
    Sync,
    Settings,
    Quit,
}

enum Screen {
    Starting,
    SignIn(Entity<SignInView>),
    MasterPassword(Entity<MasterPasswordView>),
    RecoveryPhrase(Entity<RecoveryPhraseView>),
    Unlock(Entity<UnlockView>),
    Recover(Entity<RecoverView>),
    Vault(Entity<VaultShell>),
    Settings(Entity<SettingsView>),
}

pub struct AppView {
    services: Services,
    screen: Screen,
    system_events: mpsc::Receiver<LockReason>,
    requests: Option<mpsc::Receiver<Request>>,
    /// What the sync banner and the sidebar last showed; a change repaints.
    shown_sync: kandado_app::sync::SyncStatus,
    _subscriptions: Vec<Subscription>,
}

/// Call once at startup: the keyboard shortcuts.
pub fn bind_keys(cx: &mut gpui_kit::App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-l", LockNow, None),
        KeyBinding::new("ctrl-,", OpenSettings, None),
    ]);
}

/// The window's native handle, for hiding it to the tray.
#[cfg(windows)]
fn hwnd(window: &Window) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

#[cfg(not(windows))]
fn hwnd(_: &Window) -> Option<isize> {
    None
}

impl AppView {
    pub fn new(services: Services, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (sender, receiver) = mpsc::channel();
        if let Err(error) = kandado_app::platform::watch_system_events(move |reason| {
            let _ = sender.send(reason);
        }) {
            tracing::warn!(%error, "not watching for sleep and session changes");
        }

        // Closing the window keeps Kandado in the tray, when the setting asks.
        let app = services.app.clone();
        window.on_window_should_close(cx, move |window, _| {
            if app.settings().keep_in_tray
                && let Some(hwnd) = hwnd(window)
            {
                kandado_app::platform::set_window_visible(hwnd, false);
                return false;
            }
            app.lock(LockReason::Manual);
            true
        });

        let view = Self {
            services: services.clone(),
            screen: Screen::Starting,
            system_events: receiver,
            requests: None,
            shown_sync: services.app.sync_status(),
            _subscriptions: Vec::new(),
        };

        // Work out which screen to open.
        cx.spawn_in(window, async move |this, cx| {
            let app = services.app.clone();
            let phase = services.tasks.run(async move { app.restore().await }).await;
            let _ = this.update_in(cx, |this, window, cx| match phase {
                Ok(phase) => this.show_phase(phase, window, cx),
                Err(error) => {
                    tracing::error!(%error, "could not restore the session");
                    this.show_phase(Phase::SignedOut, window, cx);
                }
            });
        })
        .detach();

        // Auto-lock, lock-on-sleep, the tray and the global shortcut.
        cx.spawn_in(window, async move |this, cx| {
            let mut ticks = 0u32;
            loop {
                cx.background_executor().timer(TICK).await;
                ticks = ticks.wrapping_add(1);
                let alive = this.update_in(cx, |this, window, cx| {
                    this.check_locks(ticks.is_multiple_of(IDLE_EVERY), window, cx);
                    this.check_requests(window, cx);
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        view
    }

    /// Where the tray menu and the global shortcut send their requests.
    pub fn listen_for(&mut self, requests: mpsc::Receiver<Request>) {
        self.requests = Some(requests);
    }

    fn check_locks(&mut self, check_idle: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut locked = false;
        while let Ok(reason) = self.system_events.try_recv() {
            locked |= self.services.app.lock_for_system_event(reason);
        }
        if check_idle && !locked {
            locked = self.services.app.lock_if_idle();
        }
        if locked {
            self.show_locked(window, cx);
        }
    }

    /// Notices what sync did since the last look: the lists and the open item
    /// follow what came from the server, and the banner and sidebar repaint.
    fn check_sync(&mut self, cx: &mut Context<Self>) {
        let status = self.services.app.sync_status();
        if self.services.app.take_sync_changes()
            && let Screen::Vault(shell) = &self.screen
        {
            shell.update(cx, |shell, cx| shell.on_synced(cx));
        }
        if status != self.shown_sync {
            self.shown_sync = status;
            cx.notify();
        }
    }

    fn check_requests(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.check_sync(cx);
        let mut pending = Vec::new();
        if let Some(requests) = &self.requests {
            while let Ok(request) = requests.try_recv() {
                pending.push(request);
            }
        }
        for request in pending {
            self.handle(request, window, cx);
        }
    }

    /// Does what the tray menu or the global shortcut asked.
    pub fn handle(&mut self, request: Request, window: &mut Window, cx: &mut Context<Self>) {
        let show = |window: &mut Window| {
            if let Some(hwnd) = hwnd(window) {
                kandado_app::platform::set_window_visible(hwnd, true);
            }
            window.activate_window();
        };
        match request {
            Request::Show => show(window),
            Request::QuickSearch => {
                show(window);
                if let Screen::Vault(shell) = &self.screen {
                    shell.update(cx, |shell, cx| shell.focus_search(window, cx));
                }
            }
            Request::GeneratePassword => {
                // Copied, and cleared like any other copied password.
                let copied = generator::generate(&Options::default())
                    .map_err(|_| ())
                    .and_then(|generated| {
                        self.services
                            .app
                            .copy_text("Password", generated.password.expose(), true)
                            .map_err(|_| ())
                    });
                if let Screen::Vault(shell) = &self.screen {
                    let message = match &copied {
                        Ok(copied) => crate::views::items::detail::copied_message(copied),
                        Err(()) => "Couldn't generate a password.".into(),
                    };
                    shell.update(cx, |shell, cx| shell.toast(message, cx));
                }
            }
            Request::Lock => self.do_lock(window, cx),
            Request::Sync => self.services.app.sync_soon(),
            Request::Settings => {
                show(window);
                self.lock_or_settings(false, window, cx);
            }
            Request::Quit => {
                self.services.app.lock(LockReason::Manual);
                cx.quit();
            }
        }
    }

    /// Opens the screen for a phase.
    pub fn show_phase(&mut self, phase: Phase, window: &mut Window, cx: &mut Context<Self>) {
        let services = self.services.clone();
        match phase {
            Phase::SignedOut => {
                let view = cx.new(|_| SignInView::new(services));
                self.screen = Screen::SignIn(view);
            }
            Phase::NeedsMasterPassword { email } => {
                let view = cx.new(|cx| MasterPasswordView::new(services, email, window, cx));
                self.screen = Screen::MasterPassword(view);
            }
            Phase::Locked { email, hello } => {
                let view = cx.new(|cx| UnlockView::new(services, email, hello, window, cx));
                self.screen = Screen::Unlock(view.clone());
                self.follow_current(window, cx);
                view.update(cx, |view, cx| view.ask_hello(cx));
                cx.notify();
                return;
            }
            Phase::Unlocked { email } => {
                let view = cx.new(|cx| VaultShell::new(services, email, window, cx));
                self.screen = Screen::Vault(view);
            }
        }
        self.follow_current(window, cx);
        cx.notify();
    }

    fn show_locked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(email) = self.services.app.email() {
            let hello = self.services.app.settings().windows_hello;
            self.show_phase(Phase::Locked { email, hello }, window, cx);
        }
    }

    /// Listens to the screen that is now showing.
    fn follow_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut subscriptions = Vec::new();
        match &self.screen {
            Screen::Starting => {}
            Screen::SignIn(view) => subscriptions.push(self.listen(view, window, cx)),
            Screen::MasterPassword(view) => subscriptions.push(self.listen(view, window, cx)),
            Screen::RecoveryPhrase(view) => subscriptions.push(self.listen(view, window, cx)),
            Screen::Unlock(view) => subscriptions.push(self.listen(view, window, cx)),
            Screen::Recover(view) => subscriptions.push(self.listen(view, window, cx)),
            Screen::Settings(view) => subscriptions.push(self.listen(view, window, cx)),
            Screen::Vault(view) => {
                subscriptions.push(self.listen(view, window, cx));
                subscriptions.push(cx.subscribe_in(
                    view,
                    window,
                    |this, _, event: &ShellEvent, window, cx| match event {
                        ShellEvent::Lock => this.do_lock(window, cx),
                    },
                ));
            }
        }
        self._subscriptions = subscriptions;
    }

    fn listen<V: gpui_kit::EventEmitter<Nav> + 'static>(
        &self,
        view: &Entity<V>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe_in(view, window, |this, _, nav: &Nav, window, cx| {
            this.navigate(nav.clone(), window, cx);
        })
    }

    fn navigate(&mut self, nav: Nav, window: &mut Window, cx: &mut Context<Self>) {
        let services = self.services.clone();
        match nav {
            Nav::Phase(phase) => self.show_phase(phase, window, cx),
            Nav::Phrase { email } => {
                let view = cx.new(|_| RecoveryPhraseView::new(services, email));
                self.screen = Screen::RecoveryPhrase(view);
                self.follow_current(window, cx);
                cx.notify();
            }
            Nav::Recover => {
                let email = services.app.email().unwrap_or_default();
                let view = cx.new(|cx| RecoverView::new(services, email, window, cx));
                self.screen = Screen::Recover(view);
                self.follow_current(window, cx);
                cx.notify();
            }
            Nav::Unlock => self.show_locked(window, cx),
            Nav::Settings => self.open_settings(window, cx),
            Nav::Import(source) => {
                self.open_settings(window, cx);
                if let Screen::Settings(settings) = &self.screen {
                    settings.update(cx, |settings, cx| settings.open_import(source, cx));
                }
            }
            Nav::Home => {
                if let Some(email) = self.services.app.email()
                    && self.services.app.is_unlocked()
                {
                    self.show_phase(Phase::Unlocked { email }, window, cx);
                }
            }
        }
    }

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.services.app.is_unlocked() || matches!(self.screen, Screen::Settings(_)) {
            return;
        }
        let services = self.services.clone();
        let view = cx.new(|cx| SettingsView::new(services, window, cx));
        self.screen = Screen::Settings(view);
        self.follow_current(window, cx);
        cx.notify();
    }

    /// Lock or Settings from the title bar or a shortcut. In the vault the
    /// shell asks first if a form has edits (D-26).
    fn lock_or_settings(&mut self, lock: bool, window: &mut Window, cx: &mut Context<Self>) {
        match &self.screen {
            Screen::Vault(shell) => shell.update(cx, |shell, cx| {
                if lock {
                    shell.ask_lock(window, cx)
                } else {
                    shell.ask_settings(window, cx)
                }
            }),
            _ if lock => self.do_lock(window, cx),
            _ => self.open_settings(window, cx),
        }
    }

    fn do_lock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.services.app.is_unlocked() {
            self.services.app.lock(LockReason::Manual);
            self.show_locked(window, cx);
        }
    }

    fn unlocked(&self) -> bool {
        matches!(self.screen, Screen::Vault(_) | Screen::Settings(_))
    }

    /// The vault shell, when it is showing.
    pub fn shell(&self) -> Option<&Entity<VaultShell>> {
        match &self.screen {
            Screen::Vault(shell) => Some(shell),
            _ => None,
        }
    }
}

impl Render for AppView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let screen = match &self.screen {
            Screen::Starting => div().flex_1().into_any_element(),
            Screen::SignIn(view) => view.clone().into_any_element(),
            Screen::MasterPassword(view) => view.clone().into_any_element(),
            Screen::RecoveryPhrase(view) => view.clone().into_any_element(),
            Screen::Unlock(view) => view.clone().into_any_element(),
            Screen::Recover(view) => view.clone().into_any_element(),
            Screen::Vault(view) => view.clone().into_any_element(),
            Screen::Settings(view) => view.clone().into_any_element(),
        };
        let unlocked = self.unlocked();
        // D-54: under the title bar, while sync cannot reach the server.
        let banner = if unlocked {
            crate::views::sync_banner(&self.services.app.sync_status()).map(|(message, retry)| {
                div()
                    .id("sync-banner")
                    .test_support()
                    .px(px(tokens::SPACE_3))
                    .pt(px(tokens::SPACE_2))
                    .child(
                        Banner::new(Tone::Warning, message)
                            .icon(IconName::Globe)
                            .when(retry, |banner| {
                                banner.action(
                                    Button::new("sync-retry", "Retry")
                                        .size(ButtonSize::Sm)
                                        .on_click(cx.listener(|this, _, _, _| {
                                            this.services.app.sync_soon()
                                        })),
                                )
                            }),
                    )
            })
        } else {
            None
        };
        let search = match &self.screen {
            Screen::Vault(shell) => Some(shell.read(cx).search_input().clone()),
            _ => None,
        };
        v_flex()
            .size_full()
            .bg(color(p.surface_000))
            .text_color(color(p.ink))
            .text_token(tokens::BODY)
            .key_context("Kandado")
            .on_action(
                cx.listener(|this, _: &LockNow, window, cx| {
                    this.lock_or_settings(true, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                this.lock_or_settings(false, window, cx)
            }))
            // Any use of the window counts as activity, so auto-lock waits.
            .capture_any_mouse_down(cx.listener(|this, _, _, _| this.services.app.touch()))
            .capture_key_down(
                cx.listener(|this, event: &gpui_kit::KeyDownEvent, window, cx| {
                    this.services.app.touch();
                    // The search field is in this title bar, outside the shell:
                    // its arrows, Enter, Esc and Ctrl C go to the shell from here.
                    if let Screen::Vault(shell) = &this.screen {
                        let shell = shell.clone();
                        let handled = shell.update(cx, |shell, cx| {
                            shell.search_focused(window, cx)
                                && shell.handle_keystroke(&event.keystroke, window, cx)
                        });
                        if handled {
                            cx.stop_propagation();
                        }
                    }
                }),
            )
            .child(
                TitleBar::new().h(px(TITLE_BAR_HEIGHT)).child(
                    h_flex()
                        .flex_1()
                        .items_center()
                        .gap(px(tokens::SPACE_4))
                        .pr(px(tokens::SPACE_3))
                        .child(
                            div()
                                .flex_none()
                                .w(px(TITLE_BAR_INSET + 120.))
                                .child(Logo::new().size(px(22.)).wordmark()),
                        )
                        .child(h_flex().flex_1().justify_center().when_some(
                            search,
                            |bar, search| {
                                bar.child(
                                    div()
                                        .w(px(440.))
                                        // The title bar is a window-drag area; on
                                        // Windows a click anywhere over it drags the
                                        // window unless something on top blocks it.
                                        .occlude()
                                        .child(SearchBar::new(&search).shortcut("Ctrl F")),
                                )
                            },
                        ))
                        .child(
                            h_flex()
                                .flex_none()
                                .w(px(120.))
                                .justify_end()
                                .gap(px(tokens::SPACE_1))
                                .when(unlocked, |bar| {
                                    // Lock and Settings must take their own clicks,
                                    // not drag the window (see the search field).
                                    bar.occlude()
                                        .child(
                                            IconButton::new(
                                                "title-lock",
                                                IconName::Lock,
                                                "Lock now",
                                            )
                                            .on_click(
                                                cx.listener(|this, _, window, cx| {
                                                    this.lock_or_settings(true, window, cx)
                                                }),
                                            ),
                                        )
                                        .child(
                                            IconButton::new(
                                                "title-settings",
                                                IconName::Settings,
                                                "Settings",
                                            )
                                            .on_click(
                                                cx.listener(|this, _, window, cx| {
                                                    this.lock_or_settings(false, window, cx)
                                                }),
                                            ),
                                        )
                                }),
                        ),
                ),
            )
            .when_some(banner, |window, banner| window.child(banner))
            .child(v_flex().flex_1().min_h_0().child(screen))
    }
}

/// Applies the theme setting, now and whenever Windows switches between light
/// and dark while the setting is "System".
pub fn follow_theme(services: &Services, window: &mut Window, cx: &mut gpui_kit::App) {
    let kind = theme::kind_for(services.app.settings().theme, cx);
    theme::apply(kind, Some(window), cx);
}
