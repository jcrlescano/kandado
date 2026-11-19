//! The phase 6 screens, headless: Share on an item, D-32 create secure link
//! (options, creating, failed, ready) and D-33 secure links (loading, could
//! not load, empty, list), in both themes and at the design's two window
//! sizes. How they look is checked by eye with `cargo run -p kandado`.
//!
//! The offline test needs nothing. The other one needs the real server:
//!
//! ```text
//! docker compose -f src/backend/docker-compose.yml up -d
//! cargo build -p kandado-server
//! DATABASE_URL=postgres://kandado:kandado@127.0.0.1:54329/kandado \
//!   cargo test -p kandado --test links -- --ignored
//! ```

#[path = "../../crates/kandado-app/tests/common/mod.rs"]
mod common;

use std::sync::Arc;

use gpui_kit::{
    AppContext as _, Bounds, Entity, Point, TestAppContext, WindowBounds, WindowOptions, px, size,
    test::TestWindowExt as _,
};
use kandado_app::{KandadoApp, Phase, tasks::Tasks};
use kandado_host::{
    shell::{Place, VaultShell},
    theme::{self, KandadoTheme, ThemeKind},
    views::{
        Services,
        sharing::{LinkDialog, LinksPage},
    },
};
use kandado_model::{
    ItemContent, ItemData,
    item::{Login, Passkey},
};
use kandado_protocol::LinkState;
use uuid::Uuid;

const EMAIL: &str = "juan.delacruz@gmail.com";
const SIZES: [(f32, f32); 2] = [(1200., 740.), (960., 600.)];

fn github() -> ItemContent {
    ItemContent::new(
        "GitHub",
        ItemData::Login(Login {
            username: "juan-dc".into(),
            password: "Tq7#vL2m!Ox0-r9P".into(),
            totp: Some("otpauth://totp/GitHub:juan?secret=JBSWY3DPEHPK3PXP".into()),
            ..Login::default()
        }),
    )
}

fn services_for(app: Arc<KandadoApp>) -> Services {
    Services::new(
        app,
        Tasks::new().expect("the background runtime starts"),
        std::env::temp_dir(),
    )
}

/// An unlocked vault on disk whose server does not answer.
fn offline(name: &str) -> Services {
    let dir =
        std::env::temp_dir().join(format!("kandado-host-links-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let paths = kandado_app::Paths {
        data_dir: dir.clone(),
    };
    kandado_app::testing::seed_account(&paths, EMAIL, "tubig at langis sa kusina");
    let app = KandadoApp::with_params(
        paths,
        Arc::new(kandado_app::secrets::MemorySecrets::default()),
        Arc::new(kandado_app::device_key::MemoryDeviceKey::new()),
        kandado_app::KdfParams::MIN,
        Some("http://127.0.0.1:9"),
    )
    .unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(app.restore()).unwrap();
    runtime
        .block_on(app.unlock_with_password("tubig at langis sa kusina".into()))
        .unwrap();
    services_for(app)
}

fn open_shell(
    cx: &mut TestAppContext,
    services: &Services,
    window_size: (f32, f32),
) -> (gpui_kit::AnyWindowHandle, Entity<VaultShell>) {
    cx.executor().allow_parking();
    let services = services.clone();
    let opened = cx.update(|cx| {
        gpui_kit::init(cx);
        theme::fonts::register(cx).expect("bundled fonts load");
        theme::apply(ThemeKind::Graphite, None, cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(window_size.0), px(window_size.1)),
            })),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| VaultShell::new(services, EMAIL.into(), window, cx))
        })
        .expect("open test window")
    });
    cx.update_window(opened.0, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();
    (opened.0, opened.1)
}

fn draw(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) {
    cx.run_until_parked();
    for kind in [ThemeKind::Enamel, ThemeKind::Graphite] {
        cx.update_window(window, |_, window, cx| {
            theme::apply(kind, Some(window), cx);
            window.render_frame(cx);
            assert_eq!(cx.theme_kind(), kind);
        })
        .expect("window is open");
    }
}

fn with<R>(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    shell: &Entity<VaultShell>,
    work: impl FnOnce(&mut VaultShell, &mut gpui_kit::Window, &mut gpui_kit::Context<VaultShell>) -> R,
) -> R {
    let result = cx
        .update_window(window, |_, window, cx| {
            shell.update(cx, |shell, cx| work(shell, window, cx))
        })
        .expect("window is open");
    draw(cx, window);
    result
}

/// Waits for background work (the server, or its absence) to come back.
fn settle(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    done: impl Fn(&gpui_kit::App) -> bool,
) {
    for _ in 0..200 {
        draw(cx, window);
        if cx.update(|cx| done(cx)) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the screen did not settle");
}

fn link_dialog(cx: &mut TestAppContext, shell: &Entity<VaultShell>) -> Entity<LinkDialog> {
    cx.update(|cx| shell.read(cx).link_dialog().cloned())
        .expect("the secure link dialog is open")
}

fn links_page(cx: &mut TestAppContext, shell: &Entity<VaultShell>) -> Entity<LinksPage> {
    cx.update(|cx| shell.read(cx).links().cloned())
        .expect("the secure links page is open")
}

#[gpui_kit::test]
fn share_opens_the_link_dialog_and_offline_it_says_why(cx: &mut TestAppContext) {
    for window_size in SIZES {
        let services = offline(&format!("offline-{}", window_size.0));
        let personal = services.app.vaults().unwrap()[0].id;
        let login = services.app.create_item(personal, &github()).unwrap();
        let passkey = services
            .app
            .create_item(
                personal,
                &ItemContent::new("Site", ItemData::Passkey(Passkey::default())),
            )
            .unwrap();
        assert!(services.app.can_share(login));
        assert!(services.app.has_totp(login));
        assert!(!services.app.can_share(passkey));

        let (window, shell) = open_shell(cx, &services, window_size);
        // A passkey has no Share; asking anyway is refused with a toast.
        with(cx, window, &shell, |shell, window, cx| {
            shell.select(passkey, window, cx);
            shell.open_link_dialog(passkey, window, cx);
        });
        cx.update(|cx| assert!(!shell.read(cx).has_dialog()));

        // D-32 options, each choice, then Create with no server.
        with(cx, window, &shell, |shell, window, cx| {
            shell.select(login, window, cx);
            shell.open_link_dialog(login, window, cx);
        });
        let dialog = link_dialog(cx, &shell);
        for index in 0..4 {
            cx.update(|cx| {
                dialog.update(cx, |dialog, cx| {
                    dialog.choose_expiry(index, cx);
                    dialog.choose_views(index, cx);
                    dialog.set_include_totp(index % 2 == 0, cx);
                })
            });
            draw(cx, window);
        }
        cx.update(|cx| dialog.update(cx, |dialog, cx| dialog.create(cx)));
        settle(cx, window, |cx| dialog.read(cx).failure().is_some());
        assert!(cx.update(|cx| !dialog.read(cx).is_ready()));

        // Escape closes it; D-33 shows that it could not load.
        cx.simulate_keystrokes(window, "escape");
        draw(cx, window);
        cx.update(|cx| assert!(!shell.read(cx).has_dialog()));
        with(cx, window, &shell, |shell, window, cx| {
            shell.go(Place::Links, window, cx)
        });
        let page = links_page(cx, &shell);
        settle(cx, window, |cx| page.read(cx).failed());

        // Opening an item from search leaves the page.
        with(cx, window, &shell, |shell, window, cx| {
            shell.select(login, window, cx)
        });
        cx.update(|cx| {
            let shell = shell.read(cx);
            assert!(shell.links().is_none());
            assert!(shell.detail().is_some());
        });
    }
}

#[gpui_kit::test]
#[ignore = "needs PostgreSQL and a built kandado-server"]
fn a_link_is_made_listed_and_revoked_with_the_real_server(cx: &mut TestAppContext) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(common::start_server());
    for (round, window_size) in SIZES.into_iter().enumerate() {
        let app = common::computer(&format!("host-links-{round}"), &server);
        runtime.block_on(async {
            app.restore().await.unwrap();
            common::google(&app, &server, &common::unique_email())
                .await
                .unwrap();
            app.create_master_password("correct horse battery staple".into())
                .await
                .unwrap();
            assert!(matches!(
                app.confirm_recovery_saved().await.unwrap(),
                Phase::Unlocked { .. }
            ));
        });
        let services = services_for(app.clone());
        let personal = app.vaults().unwrap()[0].id;
        let item = app.create_item(personal, &github()).unwrap();
        let (window, shell) = open_shell(cx, &services, window_size);

        // D-33 empty.
        with(cx, window, &shell, |shell, window, cx| {
            shell.go(Place::Links, window, cx)
        });
        let page = links_page(cx, &shell);
        settle(cx, window, |cx| {
            page.read(cx).rows().is_some_and(<[_]>::is_empty)
        });

        // D-32 options, then ready; copy; see all links.
        with(cx, window, &shell, |shell, window, cx| {
            shell.select(item, window, cx);
            shell.open_link_dialog(item, window, cx);
        });
        let dialog = link_dialog(cx, &shell);
        cx.update(|cx| {
            dialog.update(cx, |dialog, cx| {
                dialog.choose_views(1, cx);
                dialog.create(cx);
            })
        });
        settle(cx, window, |cx| dialog.read(cx).is_ready());
        cx.update(|cx| dialog.update(cx, |dialog, cx| dialog.copy(cx)));
        draw(cx, window);
        cx.update(|cx| {
            dialog.update(cx, |dialog, cx| dialog.see_all(cx));
        });
        draw(cx, window);
        let page = links_page(cx, &shell);
        settle(cx, window, |cx| {
            page.read(cx).rows().is_some_and(|rows| rows.len() == 1)
        });

        // A second link, used up from outside; then revoke the first.
        let other = runtime
            .block_on(app.create_link(item, kandado_app::links::LinkOptions::default()))
            .unwrap();
        let (address, _) = other.url.split_once('#').unwrap();
        let id = address.rsplit('/').next().unwrap();
        let opened = runtime.block_on(async {
            reqwest::Client::new()
                .post(format!("{}/v1/public/links/{id}/open", server.url))
                .send()
                .await
                .unwrap()
                .status()
        });
        assert!(opened.is_success());
        cx.update(|cx| page.update(cx, |page, cx| page.reload(cx)));
        settle(cx, window, |cx| {
            page.read(cx).rows().is_some_and(|rows| rows.len() == 2)
        });
        let first: Uuid = cx.update(|cx| {
            page.read(cx)
                .rows()
                .unwrap()
                .iter()
                .find(|row| row.info.state == LinkState::Active)
                .unwrap()
                .info
                .id
        });
        cx.update(|cx| page.update(cx, |page, cx| page.revoke(first, cx)));
        settle(cx, window, |cx| {
            page.read(cx)
                .rows()
                .is_some_and(|rows| rows.iter().all(|row| row.info.state != LinkState::Active))
        });
        let states: Vec<_> = cx.update(|cx| {
            page.read(cx)
                .rows()
                .unwrap()
                .iter()
                .map(|row| row.info.state)
                .collect()
        });
        assert_eq!(states, [LinkState::UsedUp, LinkState::Revoked]);
    }
}
