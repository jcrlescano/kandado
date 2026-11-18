//! Renders every phase 1 screen headlessly, in both themes and at the
//! design's smallest window (960 x 600) as well as 1200 x 740. This catches a
//! view that panics while building or laying out, or reads a missing asset.
//! How the screens look is checked by eye with `cargo run -p kandado`.
//!
//! The server address points at a closed port, so the settings pages that load
//! from the server show their "could not load" state, which is also a state
//! worth rendering.

use std::sync::Arc;

use gpui_kit::{
    AppContext as _, Bounds, Entity, Point, Render, TestAppContext, WindowBounds, WindowOptions,
    px, size, test::TestWindowExt,
};
use kandado_app::{
    KandadoApp, KdfParams, Paths, Phase, device_key::MemoryDeviceKey, secrets::MemorySecrets,
    tasks::Tasks,
};
use kandado_host::{
    app::AppView,
    theme::{self, KandadoTheme, ThemeKind},
    views::{
        Services,
        settings::{Page, SettingsView},
        start::{MasterPasswordView, RecoverView, RecoveryPhraseView, SignInView, UnlockView},
    },
};

fn services(name: &str) -> Services {
    let dir = std::env::temp_dir().join(format!("kandado-screens-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let app = KandadoApp::with_params(
        Paths {
            data_dir: dir.clone(),
        },
        Arc::new(MemorySecrets::default()),
        Arc::new(MemoryDeviceKey::new()),
        KdfParams::MIN,
        // Nothing listens here.
        Some("http://127.0.0.1:9"),
    )
    .expect("the app starts");
    Services::new(
        app,
        Tasks::new().expect("the background runtime starts"),
        dir,
    )
}

/// Opens `view` in a window of `window_size`, renders it in both themes and
/// returns once every frame has drawn.
fn render<V: Render + 'static>(
    cx: &mut TestAppContext,
    window_size: (f32, f32),
    build: impl FnOnce(&mut gpui_kit::Window, &mut gpui_kit::Context<V>) -> V + 'static,
) -> Entity<V> {
    // Background work runs on Tokio, outside the test scheduler.
    cx.executor().allow_parking();
    let (window, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        theme::fonts::register(cx).expect("bundled fonts load");
        theme::apply(ThemeKind::Vault, None, cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(window_size.0), px(window_size.1)),
            })),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| build(window, cx)))
            .expect("open test window")
    });
    for kind in [ThemeKind::Vault, ThemeKind::Paper, ThemeKind::Vault] {
        cx.update_window(window, |_, window, cx| {
            theme::apply(kind, Some(window), cx);
            window.render_frame(cx);
            assert_eq!(cx.theme_kind(), kind);
        })
        .expect("window is open");
    }
    view
}

const SIZES: [(f32, f32); 2] = [(1200., 740.), (960., 600.)];

#[gpui_kit::test]
fn sign_in(cx: &mut TestAppContext) {
    for window_size in SIZES {
        let services = services("sign-in");
        render(cx, window_size, |_, _| SignInView::new(services));
    }
}

#[gpui_kit::test]
fn create_master_password(cx: &mut TestAppContext) {
    for window_size in SIZES {
        let services = services("master");
        render(cx, window_size, |window, cx| {
            MasterPasswordView::new(services, "juan.delacruz@gmail.com".into(), window, cx)
        });
    }
}

#[gpui_kit::test]
fn recovery_phrase(cx: &mut TestAppContext) {
    for window_size in SIZES {
        let services = services("phrase");
        render(cx, window_size, |_, _| {
            RecoveryPhraseView::new(services, "juan.delacruz@gmail.com".into())
        });
    }
}

#[gpui_kit::test]
fn unlock_with_and_without_windows_hello(cx: &mut TestAppContext) {
    for window_size in SIZES {
        for hello in [false, true] {
            let services = services("unlock");
            render(cx, window_size, move |window, cx| {
                UnlockView::new(
                    services,
                    "juan.delacruz@gmail.com".into(),
                    hello,
                    window,
                    cx,
                )
            });
        }
    }
}

#[gpui_kit::test]
fn forgot_password(cx: &mut TestAppContext) {
    for window_size in SIZES {
        let services = services("recover");
        render(cx, window_size, |window, cx| {
            RecoverView::new(services, "juan.delacruz@gmail.com".into(), window, cx)
        });
    }
}

#[gpui_kit::test]
fn every_settings_page(cx: &mut TestAppContext) {
    for window_size in SIZES {
        let services = services("settings");
        let view = render(cx, window_size, |window, cx| {
            SettingsView::new(services, window, cx)
        });
        for page in Page::ALL {
            cx.update(|cx| view.update(cx, |view, cx| view.select(page, cx)));
            cx.run_until_parked();
            cx.update(|cx| {
                for window in cx.windows() {
                    window
                        .update(cx, |_, window, cx| window.render_frame(cx))
                        .ok();
                }
            });
        }
    }
}

#[gpui_kit::test]
fn the_window_starts_on_sign_in_with_no_account(cx: &mut TestAppContext) {
    let services = services("window");
    let app = services.app.clone();
    let view = render(cx, (1200., 740.), |window, cx| {
        AppView::new(services, window, cx)
    });
    // The first thing the window does is ask which screen to open. With no
    // account on this computer and no session, that is sign-in.
    cx.run_until_parked();
    std::thread::sleep(std::time::Duration::from_millis(200));
    cx.run_until_parked();
    cx.update(|_| assert!(!app.is_unlocked()));
    let _ = (view, Phase::SignedOut);
}
