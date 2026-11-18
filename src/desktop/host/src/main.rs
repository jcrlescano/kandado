//! The Kandado desktop app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{path::PathBuf, sync::Arc, time::Duration};

use gpui_kit::{
    AppContext as _, Bounds, WindowBounds, WindowOptions, component::TitleBar, px, size,
};
use kandado_app::{KandadoApp, Paths, tasks::Tasks};
use kandado_host::{
    app::{self, AppView, Hotkey, Request},
    assets::Assets,
    theme,
    tray::Tray,
    views::Services,
};

fn data_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Kandado")
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let tasks = Tasks::new().expect("the background runtime starts");

    #[cfg(windows)]
    let (secrets, device_key): (
        Arc<dyn kandado_app::secrets::SecretStore>,
        Arc<dyn kandado_app::device_key::DeviceKey>,
    ) = (
        Arc::new(kandado_app::secrets::windows_store::CredentialManager::new(
            "Kandado",
        )),
        Arc::new(kandado_app::device_key::windows_hello::WindowsDeviceKey::new()),
    );
    #[cfg(not(windows))]
    let (secrets, device_key): (
        Arc<dyn kandado_app::secrets::SecretStore>,
        Arc<dyn kandado_app::device_key::DeviceKey>,
    ) = (
        Arc::new(kandado_app::secrets::MemorySecrets::default()),
        Arc::new(kandado_app::device_key::MemoryDeviceKey::new()),
    );

    // KANDADO_SERVER_URL points a development build at another server.
    let server = std::env::var("KANDADO_SERVER_URL").ok();
    let app = match KandadoApp::with_params(
        Paths {
            data_dir: data_dir(),
        },
        secrets,
        device_key,
        kandado_crypto_default_kdf(),
        server.as_deref(),
    ) {
        Ok(app) => app,
        Err(error) => {
            tracing::error!(%error, "Kandado could not start");
            std::process::exit(1);
        }
    };
    let services = Services::new(app, tasks, data_dir());
    // The browser extension link, if "Unlock the browser extension" is on (D-45).
    if let Err(error) = services.browser.start() {
        tracing::warn!(%error, "the browser link did not start");
    }
    // Pulls and pushes in the background, for as long as the app runs.
    services.app.start_sync(&services.tasks);

    // Started with Windows: open in the tray, without showing the window.
    let background = std::env::args().any(|arg| arg == kandado_app::platform::BACKGROUND_ARG);

    gpui_kit::application().with_assets(Assets).run(move |cx| {
        gpui_kit::init(cx);
        theme::fonts::register(cx).expect("bundled fonts load");
        let kind = theme::kind_for(services.app.settings().theme, cx);
        theme::apply(kind, None, cx);
        app::bind_keys(cx);

        let bounds = Bounds::centered(None, size(px(1200.), px(740.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            // The design's minimum window.
            window_min_size: Some(size(px(960.), px(600.))),
            show: !background,
            focus: !background,
            ..TitleBar::window_options()
        };
        let window_services = services.clone();
        let (window, view) = gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| AppView::new(window_services, window, cx))
        })
        .expect("failed to open the window");

        // The tray icon (D-53) and the global shortcut both send requests.
        match Tray::install() {
            Ok((tray, requests)) => {
                let hotkey_requests = tray.sender();
                match kandado_app::platform::watch_global_hotkey(
                    services.app.settings().global_shortcut,
                    move || {
                        let _ = hotkey_requests.send(Request::Show);
                    },
                ) {
                    Ok(control) => cx.set_global(Hotkey(control)),
                    Err(error) => tracing::warn!(%error, "no global shortcut"),
                }
                view.update(cx, |view, _| view.listen_for(requests));
                cx.spawn(async move |cx| {
                    // Keeps the icon alive for as long as the app runs.
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(100))
                            .await;
                        tray.poll();
                    }
                })
                .detach();
            }
            Err(error) => tracing::warn!(%error, "no tray icon"),
        }

        // Follow Windows' light and dark mode while the setting is "System".
        let theme_services = services.clone();
        let _ = window.update(cx, |_, window, _cx| {
            window
                .observe_window_appearance(move |window, cx| {
                    app::follow_theme(&theme_services, window, cx);
                })
                .detach();
        });

        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
    });
}

/// Argon2id cost for new accounts.
fn kandado_crypto_default_kdf() -> kandado_app::KdfParams {
    kandado_app::KdfParams::DEFAULT
}
