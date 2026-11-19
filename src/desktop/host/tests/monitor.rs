//! Health (D-35 to D-39), headless: every screen and state in both themes and
//! at the design's two window sizes, offline and against the real server with
//! a fake Have I Been Pwned and 2FA Directory. How they look is checked by eye
//! with `cargo run -p kandado`.
//!
//! The server-backed test:
//!
//! ```text
//! docker compose -f src/backend/docker-compose.yml up -d
//! cargo build -p kandado-server
//! DATABASE_URL=postgres://kandado:kandado@127.0.0.1:54329/kandado \
//!   cargo test -p kandado --test monitor -- --ignored
//! ```

#[path = "../../crates/kandado-app/tests/common/mod.rs"]
mod common;

use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::Arc,
};

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
        monitor::{BreachPick, MonitorPage, Screen},
    },
};
use kandado_model::{
    ItemContent, ItemData,
    health::PwnedQuery,
    item::{Login, Password, Website},
};

const EMAIL: &str = "juan.delacruz@gmail.com";
const SIZES: [(f32, f32); 2] = [(1200., 740.), (960., 600.)];
const LEAKED: &str = "Kx9!vQ2#mT7$wL4p";

fn login(title: &str, password: &str, site: &str) -> ItemContent {
    ItemContent::new(
        title,
        ItemData::Login(Login {
            username: "juan".into(),
            password: password.into(),
            websites: vec![Website {
                url: format!("https://{site}"),
                ..Website::default()
            }],
            ..Login::default()
        }),
    )
}

/// Weak, reused, breached-site, exposed and two-factor items.
fn seed(app: &KandadoApp) {
    let vault = app.vaults().unwrap()[0].id;
    let mut lakbay = login("Lakbay", "Vr8#nQ2!pL6$kZ4w", "lakbay.ph");
    if let ItemData::Login(l) = &mut lakbay.data {
        l.password_changed_at = kandado_model::health::date_to_unix("2026-01-15");
    }
    for content in [
        lakbay,
        login("Router", "password1", "192.168.1.1"),
        login("Shopee", "Same-Pass-Twice-99!", "shopee.ph"),
        login("Lazada", "Same-Pass-Twice-99!", "lazada.com.ph"),
        login("GitHub", "Gh-Unique-Pass-77!x", "github.com"),
        ItemContent::new(
            "Saved password",
            ItemData::Password(Password {
                password: LEAKED.into(),
                ..Password::default()
            }),
        ),
    ] {
        app.create_item(vault, &content).unwrap();
    }
}

fn services_for(app: Arc<KandadoApp>) -> Services {
    Services::new(app, Tasks::new().unwrap(), std::env::temp_dir())
}

fn offline(name: &str) -> Services {
    let dir = std::env::temp_dir().join(format!(
        "kandado-host-monitor-{}-{name}",
        std::process::id()
    ));
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

fn settle(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    done: impl Fn(&gpui_kit::App) -> bool,
) {
    for _ in 0..400 {
        draw(cx, window);
        if cx.update(|cx| done(cx)) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the screen did not settle");
}

/// Opens Tools › Health and waits for the check.
fn open_health(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    shell: &Entity<VaultShell>,
) -> Entity<MonitorPage> {
    cx.update_window(window, |_, window, cx| {
        shell.update(cx, |shell, cx| shell.go(Place::Monitor, window, cx))
    })
    .unwrap();
    draw(cx, window);
    let page = cx
        .update(|cx| shell.read(cx).monitor().cloned())
        .expect("the Health page is open");
    settle(cx, window, |cx| page.read(cx).health().is_some());
    page
}

fn show(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    page: &Entity<MonitorPage>,
    screen: Screen,
) {
    cx.update(|cx| page.update(cx, |page, cx| page.show(screen, cx)));
    draw(cx, window);
}

#[gpui_kit::test]
fn offline_health_shows_weak_and_reused_and_says_what_was_skipped(cx: &mut TestAppContext) {
    for window_size in SIZES {
        let services = offline(&format!("offline-{}", window_size.0));
        seed(&services.app);
        let (window, shell) = open_health_window(cx, &services, window_size);
        let page = open_health(cx, window, &shell);
        cx.update(|cx| {
            let health = page.read(cx).health().unwrap();
            assert_eq!(health.weak.len(), 1);
            assert_eq!(health.reused.len(), 1);
            assert_eq!(health.unchecked.len(), 3);
            assert!(health.breaches.is_empty() && health.exposed.is_empty());
        });
        for screen in [
            Screen::Breaches(None),
            Screen::Weak,
            Screen::Reused,
            Screen::TwoFactor,
            Screen::Overview,
        ] {
            show(cx, window, &page, screen);
        }
        // "Change" opens the item's form and leaves Health.
        let weak = cx.update(|cx| page.read(cx).health().unwrap().weak[0].item.id);
        cx.update_window(window, |_, window, cx| {
            shell.update(cx, |shell, cx| shell.open_monitor_item(weak, window, cx))
        })
        .unwrap();
        draw(cx, window);
        cx.update(|cx| {
            let shell = shell.read(cx);
            assert!(shell.monitor().is_none());
            assert!(shell.form().is_some());
            assert_eq!(shell.selected(), Some(weak));
        });
    }
}

fn open_health_window(
    cx: &mut TestAppContext,
    services: &Services,
    window_size: (f32, f32),
) -> (gpui_kit::AnyWindowHandle, Entity<VaultShell>) {
    open_shell(cx, services, window_size)
}

/// Have I Been Pwned and the 2FA Directory, faked on the loopback.
fn fake_sources() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let leaked = PwnedQuery::of(LEAKED);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap() <= 2 {
                    break;
                }
            }
            let body = if path == format!("/range/{}", leaked.prefix) {
                format!("{}:42\r\n", leaked.suffix)
            } else if path.starts_with("/range/") {
                "0000000000000000000000000000000000B:3\r\n".to_owned()
            } else if path == "/api/v3/breaches" {
                r#"[{"Name":"Lakbay","Title":"Lakbay","Domain":"lakbay.ph","BreachDate":"2026-11-02",
                    "AddedDate":"2026-11-14T00:00:00Z","DataClasses":["Email addresses","Passwords"],
                    "IsVerified":true}]"#
                    .to_owned()
            } else if path == "/v3/totp.json" {
                r#"[["GitHub",{"domain":"github.com","documentation":"https://docs.github.com/2fa"}]]"#
                    .to_owned()
            } else {
                String::new()
            };
            let status = if body.is_empty() {
                "404 Not Found"
            } else {
                "200 OK"
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    base
}

#[gpui_kit::test]
#[ignore = "needs PostgreSQL and a built kandado-server"]
fn every_health_screen_with_the_real_server(cx: &mut TestAppContext) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let base = fake_sources();
    let hibp = format!("{base}/api/v3");
    let directory = format!("{base}/v3/totp.json");
    let server = runtime.block_on(common::start_server_with(
        common::free_port(),
        &[
            ("KANDADO_PWNED_URL", &base),
            ("KANDADO_HIBP_URL", &hibp),
            ("KANDADO_TWO_FACTOR_URL", &directory),
        ],
    ));
    for (round, window_size) in SIZES.into_iter().enumerate() {
        let app = common::computer(&format!("host-monitor-{round}"), &server);
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
        seed(&app);
        let services = services_for(app.clone());
        let (window, shell) = open_shell(cx, &services, window_size);
        let page = open_health(cx, window, &shell);

        // D-35 issues.
        cx.update(|cx| {
            let health = page.read(cx).health().unwrap();
            assert!(health.unchecked.is_empty(), "{:?}", health.unchecked);
            assert_eq!(health.breaches.len(), 1);
            assert_eq!(health.exposed.len(), 1);
            assert_eq!(health.two_factor.len(), 1);
            assert_eq!(health.to_fix(), 5);
        });
        // D-36: the breached site, then exposed passwords; D-37 to D-39.
        show(cx, window, &page, Screen::Breaches(None));
        show(
            cx,
            window,
            &page,
            Screen::Breaches(Some(BreachPick::Exposed)),
        );
        show(cx, window, &page, Screen::Weak);
        show(cx, window, &page, Screen::Reused);
        show(cx, window, &page, Screen::TwoFactor);

        // Mark as resolved: the breach moves to resolved.
        show(cx, window, &page, Screen::Breaches(None));
        cx.update(|cx| page.update(cx, |page, cx| page.resolve("Lakbay", cx)));
        settle(cx, window, |cx| {
            page.read(cx)
                .health()
                .is_some_and(|h| h.breaches.is_empty() && h.resolved.len() == 1)
        });
        show(
            cx,
            window,
            &page,
            Screen::Breaches(Some(BreachPick::Site("Lakbay".into()))),
        );
        // Not for this item: GitHub leaves the two-factor list.
        let github = cx.update(|cx| page.read(cx).health().unwrap().two_factor[0].item.id);
        show(cx, window, &page, Screen::TwoFactor);
        cx.update(|cx| page.update(cx, |page, cx| page.dismiss_two_factor(github, cx)));
        settle(cx, window, |cx| {
            page.read(cx)
                .health()
                .is_some_and(|h| h.two_factor.is_empty())
        });
        show(cx, window, &page, Screen::TwoFactor);
        show(cx, window, &page, Screen::Overview);
    }
}

#[gpui_kit::test]
fn a_vault_with_nothing_wrong_is_all_clear(cx: &mut TestAppContext) {
    let services = offline("clear");
    let vault = services.app.vaults().unwrap()[0].id;
    services
        .app
        .create_item(vault, &login("Bank", "Tq7#vL2m!Ox0-r9P-k", "bank.example"))
        .unwrap();
    let (window, shell) = open_shell(cx, &services, (1200., 740.));
    let page = open_health(cx, window, &shell);
    cx.update(|cx| {
        let health = page.read(cx).health().unwrap();
        assert_eq!(health.to_fix(), 0);
        assert_eq!(health.items_checked, 1);
    });
    draw(cx, window);
}
