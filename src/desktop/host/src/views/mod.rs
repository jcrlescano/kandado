//! The screens. Each view owns only what the screen needs to draw (inputs, a
//! busy flag, an error line) and asks `kandado-app` for everything else.
//! A view never sees a key: it passes what the person typed and shows what
//! comes back.

pub mod items;
pub mod monitor;
pub mod settings;
pub mod sharing;
pub mod start;
pub mod system;
pub mod vaults;

use std::sync::Arc;

use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement, SharedString,
    Styled,
    component::{h_flex, scroll::ScrollableElement as _, v_flex},
    div,
    prelude::FluentBuilder as _,
    px,
};
use kandado_app::{
    ApiError, AppError, KandadoApp, Phase, UnlockError, VaultError, auth::AuthError, tasks::Tasks,
};

use crate::{
    assets::IconName,
    components::{Icon, LcdStatus, LightState, Logo, StatusLight, panel, section_label, well},
    theme::{KandadoTheme, TextStyles as _, color, family, tokens},
};

/// What every view needs to do its work.
#[derive(Clone)]
pub struct Services {
    pub app: Arc<KandadoApp>,
    pub tasks: Tasks,
    /// The browser extension link (D-45).
    pub browser: Arc<kandado_app::browser::BrowserLink>,
    /// Updates (D-54). Created here; `main.rs` starts its checks.
    pub updater: Arc<kandado_app::update::Updater>,
}

impl Services {
    /// The services for an app keeping its files in `data_dir`. The browser
    /// link is created but not started; the app starts it (`main.rs`).
    pub fn new(app: Arc<KandadoApp>, tasks: Tasks, data_dir: std::path::PathBuf) -> Self {
        let updater = kandado_app::update::Updater::new(&data_dir);
        let browser = kandado_app::browser::BrowserLink::new(app.clone(), tasks.handle(), data_dir);
        Self {
            app,
            tasks,
            browser,
            updater,
        }
    }
}

/// Where a view asks the window to go next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nav {
    /// The app reached this phase; show its screen.
    Phase(Phase),
    /// The master password is set; show the recovery phrase (D-03).
    Phrase { email: String },
    /// Open the forgot-password screen.
    Recover,
    /// Back to the unlock screen.
    Unlock,
    /// Open settings.
    Settings,
    /// Open Settings on Import, with a source chosen if the first-run offer did.
    Import(Option<kandado_app::Source>),
    /// Close settings.
    Home,
}

/// The sentence shown when something fails. Plain words, no codes.
pub fn explain(error: &AppError) -> SharedString {
    match error {
        AppError::Api(error) => explain_api(error),
        AppError::Auth(error) => match error {
            AuthError::Denied => "Sign-in was cancelled.".into(),
            AuthError::WrongAccount => {
                "That is a different Google account. Use the one you signed in with.".into()
            }
            AuthError::TimedOut => "Sign-in took too long. Try again.".into(),
            AuthError::Failed => "Google could not sign you in. Try again.".into(),
            AuthError::Io(_) => "Kandado could not listen for Google. Try again.".into(),
        },
        AppError::WeakPassword => {
            "Choose a longer, less guessable password. Aim for a short sentence.".into()
        }
        AppError::WrongPhrase => {
            "Those words are not your recovery phrase. Check them and try again.".into()
        }
        AppError::WrongPassword => "That is not your master password.".into(),
        AppError::DeviceKey(_) => "Windows Hello could not be used. Try again.".into(),
        AppError::Vault(VaultError::Locked) => "Kandado is locked. Unlock it and try again.".into(),
        AppError::Vault(VaultError::NotFound) => "That item is no longer on this computer.".into(),
        AppError::Vault(VaultError::NotInTrash) => {
            "Move the item to the trash before deleting it forever.".into()
        }
        AppError::Clipboard(_) => "Kandado could not use the clipboard. Try again.".into(),
        AppError::Vault(_) | AppError::Store(_) | AppError::Secrets(_) | AppError::Settings(_) => {
            "Kandado could not read or write its data on this computer.".into()
        }
        AppError::Model(kandado_model::Error::NotShareable) => {
            "A passkey can\u{2019}t be shared by link.".into()
        }
        AppError::Crypto(_) | AppError::Model(_) | AppError::WrongState => {
            "Something went wrong. Restart Kandado and try again.".into()
        }
    }
}

fn explain_api(error: &ApiError) -> SharedString {
    match error {
        ApiError::Offline(_) => "Kandado cannot reach the server. Check your connection.".into(),
        ApiError::SignedOut => "You were signed out. Sign in again.".into(),
        ApiError::Problem(_) | ApiError::Unexpected(_) => {
            "The server could not do that. Try again in a moment.".into()
        }
    }
}

pub fn explain_unlock(error: &UnlockError) -> SharedString {
    match error {
        UnlockError::WrongPassword { tries_left } if *tries_left > 0 => {
            let noun = if *tries_left == 1 { "try" } else { "tries" };
            format!("Wrong password. {tries_left} {noun} left.").into()
        }
        UnlockError::WrongPassword { .. } => {
            "Wrong password. Wait a minute before trying again.".into()
        }
        UnlockError::Wait(wait) => format!(
            "Too many wrong passwords. Try again in {} seconds.",
            wait.as_secs().max(1)
        )
        .into(),
        UnlockError::HelloUnavailable => {
            "Windows Hello did not unlock Kandado. Use your password.".into()
        }
        UnlockError::Failed(error) => explain(error),
    }
}

/// The page heading of the start screens and forms: the display style, at
/// 36 px as on the Unlock board.
pub fn heading(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .font_family(family(tokens::DISPLAY.family))
        .text_size(px(36.))
        .line_height(px(38.))
        .font_weight(gpui_kit::FontWeight(f32::from(tokens::DISPLAY.weight)))
        .text_color(color(cx.palette().ink))
        .child(text.into())
}

/// Body text under a heading.
pub fn paragraph(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_token(tokens::BODY)
        .text_color(color(cx.palette().ink_2))
        .child(text.into())
}

pub fn small(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_token(tokens::SMALL)
        .text_color(color(cx.palette().ink_2))
        .child(text.into())
}

/// A line of error text with its icon, for failures that are not tied to one field.
pub fn error_line(text: impl Into<SharedString>, cx: &App) -> Div {
    h_flex()
        .items_start()
        .gap(px(tokens::SPACE_2))
        .text_token(tokens::SMALL)
        .text_color(color(cx.palette().danger))
        .child(
            Icon::new(IconName::Alert)
                .size(px(16.))
                .color(color(cx.palette().danger)),
        )
        .child(div().flex_1().child(text.into()))
}

fn promise(text: &'static str, cx: &App) -> impl IntoElement {
    let p = cx.palette();
    h_flex()
        .items_center()
        .gap(px(tokens::SPACE_2))
        .text_token(tokens::SMALL)
        .text_color(color(p.ink_2))
        .child(StatusLight::new(LightState::Ok).size(8.))
        .child(div().flex_1().child(text))
}

/// The start screens' frame (D-01 to D-04, Unlock board): one panel in the
/// middle of the window, with the padlock and an LCD status sunk into a well
/// on the left and the form on the right.
pub fn auth_shell(form: impl IntoElement, cx: &App) -> AnyElement {
    auth_frame(form, LightState::Ok, "End-to-end encrypted", cx)
}

/// [`auth_shell`] with the LCD saying something about this screen, such as
/// "Locked".
pub fn auth_frame(
    form: impl IntoElement,
    light: LightState,
    status: impl Into<SharedString>,
    cx: &App,
) -> AnyElement {
    // Centred when it fits; scrolls when the window is shorter than the
    // form (a small screen, or the taskbar taking its share), so the button
    // at the bottom can always be reached.
    div()
        .id("auth-scroll")
        .flex_1()
        .min_h_0()
        .overflow_y_scrollbar()
        .child(
            v_flex()
                .min_h_full()
                .items_center()
                .justify_center()
                .p(px(tokens::SPACE_5))
                .child(
                    panel(cx)
                        .flex()
                        .flex_wrap()
                        .w_full()
                        .max_w(px(900.))
                        .gap(px(tokens::SPACE_5))
                        .p(px(tokens::SPACE_5))
                        .child(
                            well(cx)
                                .flex()
                                .flex_col()
                                .flex_grow(1.)
                                .flex_basis(px(340.))
                                // Text measures its unwrapped width as its minimum; without
                                // this the two columns stack and the form falls off the bottom.
                                .min_w_0()
                                .min_h(px(380.))
                                .items_center()
                                .justify_center()
                                .gap(px(tokens::SPACE_6))
                                .p(px(tokens::SPACE_7))
                                .child(Logo::new().size(px(140.)))
                                .child(
                                    div()
                                        .w_full()
                                        .max_w(px(280.))
                                        .child(LcdStatus::new(status).light(light)),
                                )
                                .child(
                                    v_flex()
                                        .gap(px(6.))
                                        .child(promise(
                                            "Encrypted on this computer before it syncs.",
                                            cx,
                                        ))
                                        .child(promise(
                                            "Your master password never leaves it.",
                                            cx,
                                        )),
                                ),
                        )
                        .child(
                            v_flex()
                                .flex_grow(1.)
                                .flex_basis(px(340.))
                                // Text measures its unwrapped width as its minimum; without
                                // this the two columns stack and the form falls off the bottom.
                                .min_w_0()
                                .justify_center()
                                .gap(px(20.))
                                .px(px(tokens::SPACE_5))
                                .py(px(tokens::SPACE_6))
                                .child(Logo::new().size(px(26.)).wordmark())
                                .child(form)
                                .child(small(concat!("Kandado ", env!("CARGO_PKG_VERSION")), cx)),
                        ),
                ),
        )
        .into_any_element()
}

/// A value the person cannot edit, drawn like a field: the Google address
/// at D-02.
pub fn read_only_field(
    label: &'static str,
    value: impl Into<SharedString>,
    icon: IconName,
    cx: &App,
) -> Div {
    let p = cx.palette();
    v_flex().gap(px(6.)).child(section_label(label, cx)).child(
        well(cx)
            .flex()
            .items_center()
            .gap(px(tokens::SPACE_2))
            .h(px(48.))
            .px(px(14.))
            .rounded(px(tokens::RADIUS_KEY))
            .child(Icon::new(icon).size(px(18.)).color(color(p.ink_2)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_token(tokens::BODY)
                    .text_color(color(p.ink))
                    .child(value.into()),
            ),
    )
}

/// Puts `overlay` (a [`crate::components::Dialog`]) over `content`. The
/// overlay fills the area, so the content behind it cannot be pressed.
pub fn with_overlay(content: AnyElement, overlay: Option<AnyElement>) -> Div {
    v_flex()
        .relative()
        .flex_1()
        .min_h_0()
        .child(content)
        .when_some(overlay, |area, overlay| area.child(overlay))
}

/// A date as the design writes it: "12 Oct 2026". `unix` is in seconds.
pub fn date(unix: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let local = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    match time::OffsetDateTime::from_unix_timestamp(unix) {
        Ok(moment) => {
            let moment = moment.to_offset(local);
            format!(
                "{} {} {}",
                moment.day(),
                MONTHS[usize::from(u8::from(moment.month())) - 1],
                moment.year()
            )
        }
        Err(_) => String::new(),
    }
}

/// What the sidebar says about syncing: "Synced just now", "Offline".
pub fn sync_text(status: &kandado_app::sync::SyncStatus, now: i64) -> String {
    use kandado_app::sync::{SyncFailure, SyncState};
    match &status.state {
        SyncState::Syncing => "Syncing\u{2026}".to_owned(),
        SyncState::Offline => "Offline".to_owned(),
        SyncState::Failed(SyncFailure::SignedOut) => "Signed out".to_owned(),
        SyncState::Failed(SyncFailure::Other) => "Couldn\u{2019}t sync".to_owned(),
        SyncState::Idle => match status.last_synced {
            None => "Not synced yet".to_owned(),
            Some(at) => {
                let seconds = (now - at).max(0);
                match seconds {
                    0..=89 => "Synced just now".to_owned(),
                    90..=3_599 => format!("Synced {} min ago", (seconds + 30) / 60),
                    3_600..=86_399 => format!("Synced {} h ago", (seconds + 1_800) / 3_600),
                    _ => format!("Synced {} d ago", (seconds + 43_200) / 86_400),
                }
            }
        },
    }
}

/// The header's status light and its word: lit green once synced, orange
/// while syncing or in trouble, unlit before the first sync.
pub fn sync_light(status: &kandado_app::sync::SyncStatus) -> (LightState, &'static str) {
    use kandado_app::sync::SyncState;
    match (&status.state, status.last_synced) {
        (SyncState::Syncing, _) => (LightState::Warn, "SYNCING"),
        (SyncState::Offline, _) => (LightState::Warn, "OFFLINE"),
        (SyncState::Failed(_), _) => (LightState::Warn, "NOT SYNCED"),
        (SyncState::Idle, None) => (LightState::Off, "NOT SYNCED"),
        (SyncState::Idle, Some(_)) => (LightState::Ok, "SYNCED"),
    }
}

/// The banner under the title bar when sync cannot reach the server (D-54):
/// its words, and whether "Retry" is offered. `None` when there is nothing
/// to say. The offline banner clears itself with the next sync that works.
pub fn sync_banner(status: &kandado_app::sync::SyncStatus) -> Option<(&'static str, bool)> {
    use kandado_app::sync::{SyncFailure, SyncState};
    match &status.state {
        SyncState::Offline => Some((
            "You\u{2019}re offline. Changes are saved on this computer and sync when you reconnect.",
            true,
        )),
        SyncState::Failed(SyncFailure::Other) => Some(("Couldn\u{2019}t sync. Retrying.", true)),
        SyncState::Failed(SyncFailure::SignedOut) => Some((
            "Your session ended on another computer. Sign out and sign in again to keep syncing.",
            false,
        )),
        SyncState::Idle | SyncState::Syncing => None,
    }
}

#[cfg(test)]
mod sync_text_tests {
    use kandado_app::sync::{SyncFailure, SyncState, SyncStatus};

    use super::{sync_banner, sync_light, sync_text};
    use crate::components::LightState;

    fn status(state: SyncState, last: Option<i64>) -> SyncStatus {
        SyncStatus {
            state,
            last_synced: last,
        }
    }

    #[test]
    fn the_sidebar_says_where_syncing_stands() {
        let now = 100_000;
        let idle = |last| sync_text(&status(SyncState::Idle, last), now);
        assert_eq!(idle(None), "Not synced yet");
        assert_eq!(idle(Some(now - 10)), "Synced just now");
        assert_eq!(idle(Some(now - 300)), "Synced 5 min ago");
        assert_eq!(idle(Some(now - 7_200)), "Synced 2 h ago");
        assert_eq!(idle(Some(now - 3 * 86_400)), "Synced 3 d ago");
        assert_eq!(
            sync_text(&status(SyncState::Syncing, None), now),
            "Syncing\u{2026}"
        );
        assert_eq!(
            sync_text(&status(SyncState::Offline, Some(1)), now),
            "Offline"
        );
        assert_eq!(
            sync_text(&status(SyncState::Failed(SyncFailure::Other), None), now),
            "Couldn\u{2019}t sync"
        );
    }

    #[test]
    fn the_header_light_never_says_synced_before_the_first_sync() {
        let light = |state, last| sync_light(&status(state, last));
        assert_eq!(light(SyncState::Idle, Some(1)), (LightState::Ok, "SYNCED"));
        assert_eq!(
            light(SyncState::Idle, None),
            (LightState::Off, "NOT SYNCED")
        );
        assert_eq!(light(SyncState::Syncing, Some(1)).0, LightState::Warn);
        assert_eq!(
            light(SyncState::Offline, Some(1)),
            (LightState::Warn, "OFFLINE")
        );
        assert_eq!(
            light(SyncState::Failed(SyncFailure::Other), Some(1)).0,
            LightState::Warn
        );
    }

    #[test]
    fn only_trouble_gets_a_banner_and_retry_is_offered_when_it_can_help() {
        let banner = |state| sync_banner(&status(state, Some(1)));
        assert_eq!(banner(SyncState::Idle), None);
        assert_eq!(banner(SyncState::Syncing), None);
        let (offline, retry) = banner(SyncState::Offline).unwrap();
        assert!(offline.starts_with("You\u{2019}re offline.") && retry);
        assert!(banner(SyncState::Failed(SyncFailure::Other)).unwrap().1);
        assert!(!banner(SyncState::Failed(SyncFailure::SignedOut)).unwrap().1);
    }
}
