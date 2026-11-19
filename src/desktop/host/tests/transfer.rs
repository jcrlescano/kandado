//! The import and export screens, headless (D-05, D-47, D-48): the first-run
//! offer, every state of the import page, and the export page for each
//! format. Each frame is drawn in both themes at the design's two window
//! sizes. The system file dialogs cannot be driven here, so files are handed
//! to the pages the way the dialogs would.

use std::{sync::Arc, time::Duration};

use gpui_kit::{
    AppContext as _, Bounds, Entity, Point, TestAppContext, WindowBounds, WindowOptions, px, size,
    test::TestWindowExt as _,
};
use kandado_app::{
    ExportFormat, KandadoApp, KdfParams, Paths, Source,
    clipboard::{Clipboard, MemoryClipboard},
    device_key::MemoryDeviceKey,
    secrets::MemorySecrets,
    tasks::Tasks,
    testing::seed_account,
};
use kandado_host::{
    shell::VaultShell,
    theme::{self, KandadoTheme, ThemeKind},
    views::{
        Services,
        settings::{ExportPage, ImportPage, Page, SettingsView},
    },
};

const PASSWORD: &str = "tubig at langis sa kusina";
const EMAIL: &str = "juan.delacruz@gmail.com";
const PROTON: &str = include_str!("../../crates/kandado-vault/tests/fixtures/proton-data.json");
const SIZES: [(f32, f32); 2] = [(1200., 740.), (960., 600.)];

fn unlocked(name: &str) -> Services {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "kandado-host-transfer-{}-{name}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let paths = Paths {
        data_dir: dir.clone(),
    };
    seed_account(&paths, EMAIL, PASSWORD);
    let app = KandadoApp::with_clipboard(
        paths,
        Arc::new(MemorySecrets::default()),
        Arc::new(MemoryDeviceKey::new()),
        Clipboard::new(Arc::new(MemoryClipboard::default())),
        KdfParams::MIN,
        Some("http://127.0.0.1:9"),
    )
    .expect("the app starts");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(app.restore()).unwrap();
    runtime
        .block_on(app.unlock_with_password(PASSWORD.into()))
        .expect("unlocks offline");
    Services::new(
        app,
        Tasks::new().expect("the background runtime starts"),
        dir,
    )
}

fn options(window_size: (f32, f32)) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: Point::default(),
            size: size(px(window_size.0), px(window_size.1)),
        })),
        ..Default::default()
    }
}

fn start(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        theme::fonts::register(cx).expect("bundled fonts load");
        theme::apply(ThemeKind::Graphite, None, cx);
    });
}

/// Draws the window in both themes.
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

fn open_settings(
    cx: &mut TestAppContext,
    services: &Services,
    window_size: (f32, f32),
) -> (gpui_kit::AnyWindowHandle, Entity<SettingsView>) {
    start(cx);
    let services = services.clone();
    cx.update(|cx| {
        gpui_kit::open_window(options(window_size), cx, |window, cx| {
            cx.new(|cx| SettingsView::new(services, window, cx))
        })
        .expect("open test window")
    })
}

/// Waits (up to five seconds) for background work to show on a page.
fn wait_until(
    cx: &mut TestAppContext,
    what: &str,
    mut ready: impl FnMut(&mut TestAppContext) -> bool,
) {
    for _ in 0..250 {
        cx.run_until_parked();
        if ready(cx) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {what}");
}

fn file(name: &str, text: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kandado-host-files-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

fn showing(cx: &mut TestAppContext, page: &Entity<ImportPage>) -> &'static str {
    cx.update(|cx| page.read(cx).showing())
}

#[gpui_kit::test]
fn the_first_run_offer_shows_in_an_empty_vault_until_skipped(cx: &mut TestAppContext) {
    for window_size in SIZES {
        let services = unlocked("first-run");
        start(cx);
        let handle = {
            let services = services.clone();
            cx.update(|cx| {
                gpui_kit::open_window(options(window_size), cx, |window, cx| {
                    cx.new(|cx| VaultShell::new(services, EMAIL.into(), window, cx))
                })
                .expect("open test window")
            })
        };
        let (window, shell) = handle;
        // D-05.
        assert!(services.app.should_offer_import());
        draw(cx, window);
        // Skip for now: the offer is over and D-06 shows.
        services.app.import_offer_done().unwrap();
        cx.update(|cx| shell.update(cx, |shell, cx| shell.refresh(cx)));
        assert!(!services.app.should_offer_import());
        draw(cx, window);
        // D-18: the create menu has Import.
        cx.update(|cx| shell.update(cx, |shell, cx| shell.open_create_menu(cx)));
        draw(cx, window);
    }
}

#[gpui_kit::test]
fn importing_a_file_through_the_import_page(cx: &mut TestAppContext) {
    for window_size in SIZES {
        let services = unlocked("import-page");
        let (window, settings) = open_settings(cx, &services, window_size);
        cx.update(|cx| settings.update(cx, |view, cx| view.select(Page::Import, cx)));
        let page = cx.update(|cx| settings.read(cx).import_page().clone());
        assert_eq!(showing(cx, &page), "sources");
        draw(cx, window);

        // A source chosen from the first-run offer.
        cx.update(|cx| {
            settings.update(cx, |view, cx| {
                view.open_import(Some(Source::ProtonPass), cx)
            })
        });
        assert_eq!(showing(cx, &page), "file");
        draw(cx, window);

        // A file that is not Proton's: the page says so.
        let wrong = file("not-proton.csv", "a,b\n1,2\n");
        cx.update(|cx| {
            page.update(cx, |page, cx| {
                page.load(Source::ProtonPass, wrong.clone(), None, cx)
            })
        });
        wait_until(cx, "the file to be read", |cx| {
            showing(cx, &page) != "reading"
        });
        draw(cx, window);
        assert_eq!(showing(cx, &page), "failed");

        // The real one: preview, then import.
        let export = file("proton.json", PROTON);
        cx.update(|cx| {
            page.update(cx, |page, cx| {
                page.load(Source::ProtonPass, export.clone(), None, cx)
            })
        });
        wait_until(cx, "the preview", |cx| showing(cx, &page) == "preview");
        draw(cx, window);
        cx.update(|cx| page.update(cx, |page, cx| page.start_import(cx)));
        wait_until(cx, "the import to finish", |cx| {
            showing(cx, &page) == "done"
        });
        draw(cx, window);
        assert_eq!(services.app.counts(), (8, 1));
        assert_eq!(services.app.last_import().unwrap().imported, 9);
        // The first-run offer is over: the vault has items.
        assert!(!services.app.should_offer_import());

        // Back to the sources, with the last import shown.
        cx.update(|cx| settings.update(cx, |view, cx| view.open_import(None, cx)));
        assert_eq!(showing(cx, &page), "sources");
        draw(cx, window);
    }
}

#[gpui_kit::test]
fn an_encrypted_export_asks_for_its_password_in_the_import_page(cx: &mut TestAppContext) {
    let services = unlocked("import-passphrase");
    // Something to export: import the Proton file first.
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let reading = runtime
        .block_on(services.app.read_import_bytes(
            Source::ProtonPass,
            zeroize::Zeroizing::new(PROTON.as_bytes().to_vec()),
            "p.json".into(),
            None,
        ))
        .unwrap();
    runtime
        .block_on(services.app.run_import(
            reading.plan,
            kandado_app::transfer::ImportChoice::MatchSource,
            true,
            kandado_app::transfer::ImportProgress::new(),
        ))
        .unwrap();
    let path =
        std::env::temp_dir().join(format!("kandado-host-enc-{}.kandado", std::process::id()));
    runtime
        .block_on(
            services
                .app
                .export_to(&path, ExportFormat::Encrypted, PASSWORD.into()),
        )
        .unwrap();

    let (window, settings) = open_settings(cx, &services, (1200., 740.));
    let page = cx.update(|cx| settings.read(cx).import_page().clone());
    cx.update(|cx| settings.update(cx, |view, cx| view.open_import(Some(Source::Kandado), cx)));
    let load = |cx: &mut TestAppContext, passphrase: Option<&str>| {
        let (path, passphrase) = (path.clone(), passphrase.map(str::to_owned));
        cx.update(|cx| {
            page.update(cx, |page, cx| {
                page.load(Source::Kandado, path, passphrase, cx)
            })
        });
    };
    load(cx, None);
    wait_until(cx, "the password prompt", |cx| {
        showing(cx, &page) == "passphrase"
    });
    draw(cx, window);
    load(cx, Some("not it"));
    wait_until(cx, "the wrong password", |cx| {
        showing(cx, &page) == "passphrase"
    });
    draw(cx, window);
    load(cx, Some(PASSWORD));
    wait_until(cx, "the preview", |cx| showing(cx, &page) == "preview");
    draw(cx, window);
    let _ = std::fs::remove_file(&path);
}

fn set_password(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    page: &Entity<ExportPage>,
    value: &str,
) {
    let value = value.to_owned();
    cx.update_window(window, |_, window, cx| {
        let input = page.read(cx).password_input().clone();
        input.update(cx, |input, cx| input.set_value(value, window, cx));
    })
    .unwrap();
}

#[gpui_kit::test]
fn the_export_page_checks_the_password_and_saves_each_format(cx: &mut TestAppContext) {
    for window_size in SIZES {
        let services = unlocked("export-page");
        // Something to export.
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let reading = runtime
            .block_on(services.app.read_import_bytes(
                Source::ProtonPass,
                zeroize::Zeroizing::new(PROTON.as_bytes().to_vec()),
                "p.json".into(),
                None,
            ))
            .unwrap();
        runtime
            .block_on(services.app.run_import(
                reading.plan,
                kandado_app::transfer::ImportChoice::MatchSource,
                true,
                kandado_app::transfer::ImportProgress::new(),
            ))
            .unwrap();

        let (window, settings) = open_settings(cx, &services, window_size);
        cx.update(|cx| settings.update(cx, |view, cx| view.select(Page::Export, cx)));
        let page = cx.update(|cx| settings.read(cx).export_page().clone());
        assert_eq!(cx.update(|cx| page.read(cx).showing()), "ready");
        draw(cx, window);

        for (format, extension) in [
            (ExportFormat::Encrypted, "kandado"),
            (ExportFormat::Json, "json"),
            (ExportFormat::Csv, "csv"),
        ] {
            cx.update(|cx| page.update(cx, |page, cx| page.set_format(format, cx)));
            draw(cx, window);
            let path = std::env::temp_dir().join(format!(
                "kandado-host-export-{}.{extension}",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&path);

            // A wrong password writes nothing and says so.
            set_password(cx, window, &page, "not it");
            cx.update_window(window, |_, window, cx| {
                page.update(cx, |page, cx| page.export_to(path.clone(), window, cx))
            })
            .unwrap();
            wait_until(cx, "the password check", |cx| {
                cx.update(|cx| page.read(cx).showing()) != "exporting"
            });
            draw(cx, window);
            assert_eq!(cx.update(|cx| page.read(cx).showing()), "wrong password");
            assert!(!path.exists());

            // The right one saves the file.
            set_password(cx, window, &page, PASSWORD);
            cx.update_window(window, |_, window, cx| {
                page.update(cx, |page, cx| page.export_to(path.clone(), window, cx))
            })
            .unwrap();
            wait_until(cx, "the file to be saved", |cx| {
                cx.update(|cx| page.read(cx).showing()) == "saved"
            });
            draw(cx, window);
            assert!(path.exists());
            let _ = std::fs::remove_file(&path);
        }
    }
}
