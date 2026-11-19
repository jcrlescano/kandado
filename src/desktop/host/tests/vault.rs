//! The phase 2 screens, headless. Each test unlocks a real local vault (an
//! account seeded on disk, no server), drives the shell the way the keyboard
//! and buttons do, and renders every frame in both themes at the design's
//! two window sizes, so a view that panics while building or laying out, or
//! reads a missing asset, fails here. How the screens look is checked by eye
//! with `cargo run -p kandado`.

use std::{sync::Arc, time::Duration};

use gpui_kit::{
    AppContext as _, Bounds, Entity, Modifiers, Point, TestAppContext, WindowBounds, WindowOptions,
    px, size, test::TestWindowExt as _,
};
use kandado_app::{
    Filter, KandadoApp, KdfParams, Paths,
    clipboard::{Clipboard, MemoryClipboard},
    device_key::MemoryDeviceKey,
    secrets::MemorySecrets,
    tasks::Tasks,
    testing::seed_account,
};
use kandado_host::{
    shell::{Place, RAIL_BREAKPOINT, VaultShell},
    theme::{self, KandadoTheme, ThemeKind},
    views::{
        Services,
        settings::{Page, SettingsView},
    },
};
use kandado_model::{
    ItemContent, ItemData, ItemKind, VaultMeta,
    item::{Card, Identity, Login, Password, Website},
};
use uuid::Uuid;

const PASSWORD: &str = "tubig at langis sa kusina";
const EMAIL: &str = "juan.delacruz@gmail.com";

struct Vault {
    services: Services,
    clipboard: Arc<MemoryClipboard>,
}

/// An unlocked vault on disk, with no server.
fn unlocked(name: &str) -> Vault {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "kandado-host-vault-{}-{name}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let paths = Paths {
        data_dir: dir.clone(),
    };
    seed_account(&paths, EMAIL, PASSWORD);
    let clipboard = Arc::new(MemoryClipboard::default());
    let app = KandadoApp::with_clipboard(
        paths,
        Arc::new(MemorySecrets::default()),
        Arc::new(MemoryDeviceKey::new()),
        Clipboard::new(clipboard.clone()),
        KdfParams::MIN,
        Some("http://127.0.0.1:9"),
    )
    .expect("the app starts");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(app.restore()).unwrap();
    runtime
        .block_on(app.unlock_with_password(PASSWORD.into()))
        .expect("unlocks offline");
    Vault {
        services: Services::new(
            app,
            Tasks::new().expect("the background runtime starts"),
            std::env::temp_dir(),
        ),
        clipboard,
    }
}

fn login(title: &str, username: &str) -> ItemContent {
    ItemContent::new(
        title,
        ItemData::Login(Login {
            username: username.into(),
            password: "Tq7#vL2m!Ox0-r9P".into(),
            websites: vec![Website {
                url: format!("https://{}.example.test", title.to_lowercase()),
                ..Website::default()
            }],
            totp: Some("otpauth://totp/GitHub:juan?secret=JBSWY3DPEHPK3PXP".into()),
            ..Login::default()
        }),
    )
}

/// One item of every phase 2 kind, in the first vault. Returns their ids.
fn fill(vault: &Vault) -> Vec<Uuid> {
    let app = &vault.services.app;
    let personal = app.vaults().unwrap()[0].id;
    let mut note = ItemContent::new("Wi-Fi at home", ItemData::default());
    note.notes = "Network: Bahay-5G\nPassword: mangga-tide-42".into();
    [
        login("GitHub", "juan-dc"),
        ItemContent::new(
            "BPI Visa",
            ItemData::Card(Card {
                cardholder: "JUAN DELA CRUZ".into(),
                number: "4821009433104821".into(),
                cvv: "214".into(),
                ..Card::default()
            }),
        ),
        note,
        ItemContent::new(
            "Juan Dela Cruz",
            ItemData::Identity(Box::new(Identity {
                first_name: "Juan".into(),
                last_name: "Dela Cruz".into(),
                email: "juan@example.test".into(),
                ..Identity::default()
            })),
        ),
        ItemContent::new(
            "Generated password",
            ItemData::Password(Password {
                password: "cobalt-Meadow-anchor-7".into(),
                ..Password::default()
            }),
        ),
    ]
    .iter()
    .map(|content| app.create_item(personal, content).unwrap())
    .collect()
}

const SIZES: [(f32, f32); 2] = [(1200., 740.), (960., 600.)];

fn open_shell(
    cx: &mut TestAppContext,
    vault: &Vault,
    window_size: (f32, f32),
) -> (gpui_kit::AnyWindowHandle, Entity<VaultShell>) {
    cx.executor().allow_parking();
    let services = vault.services.clone();
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
    // A real window is in front when the person uses it.
    cx.update_window(opened.0, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();
    opened
}

/// Draws the window in both themes, ending in the dark one.
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

/// Runs `work` on the shell with its window, then draws.
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

fn press(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, keys: &str) {
    cx.simulate_keystrokes(window, keys);
    draw(cx, window);
}

#[gpui_kit::test]
fn an_empty_vault_and_every_detail_in_both_sizes(cx: &mut TestAppContext) {
    for window_size in SIZES {
        let vault = unlocked("details");
        // D-06: nothing yet.
        let (window, shell) = open_shell(cx, &vault, window_size);
        draw(cx, window);
        cx.update(|cx| assert_eq!(shell.read(cx).place(), Place::AllVaults));

        // D-07, D-09 – D-12: every kind's detail.
        let ids = fill(&vault);
        with(cx, window, &shell, |shell, _, cx| shell.refresh(cx));
        for id in &ids {
            with(cx, window, &shell, |shell, window, cx| {
                shell.select(*id, window, cx)
            });
            cx.update(|cx| {
                let shell = shell.read(cx);
                assert_eq!(shell.selected(), Some(*id));
                assert!(shell.detail().is_some());
            });
        }
        // Each list filter, and an empty one.
        for kind in [
            Some(ItemKind::Login),
            Some(ItemKind::Card),
            Some(ItemKind::Identity),
            None,
        ] {
            with(cx, window, &shell, |shell, _, cx| shell.set_kind(kind, cx));
        }
        // D-41 empty trash list, Monitor placeholder, back.
        for place in [Place::Trash, Place::Monitor, Place::AllVaults] {
            with(cx, window, &shell, |shell, window, cx| {
                shell.go(place, window, cx)
            });
        }
    }
}

#[gpui_kit::test]
fn the_sidebar_becomes_a_rail_in_a_narrow_window(cx: &mut TestAppContext) {
    let vault = unlocked("rail");
    fill(&vault);
    let (window, _) = open_shell(cx, &vault, (1280., 800.));
    draw(cx, window);
    // D-17: just below the breakpoint, and the smallest window.
    for width in [RAIL_BREAKPOINT - px(1.), px(960.), RAIL_BREAKPOINT] {
        cx.simulate_window_resize(window, size(width, px(700.)));
        draw(cx, window);
    }
}

#[gpui_kit::test]
fn reveal_copy_and_remask_when_the_window_loses_focus(cx: &mut TestAppContext) {
    let vault = unlocked("reveal");
    let ids = fill(&vault);
    let (window, shell) = open_shell(cx, &vault, (1200., 740.));
    with(cx, window, &shell, |shell, window, cx| {
        shell.select(ids[0], window, cx)
    });
    let detail = cx.update(|cx| shell.read(cx).detail().cloned().unwrap());

    // The password is masked until revealed.
    cx.update(|cx| {
        let detail = detail.read(cx);
        assert!(!detail.has_revealed());
        assert!(!format!("{:?}", detail.detail()).contains("Tq7#vL2m"));
    });
    let password = cx.update(|cx| detail.read(cx).detail().unwrap().password_field.unwrap());
    cx.update(|cx| detail.update(cx, |detail, cx| detail.toggle_reveal(password, cx)));
    draw(cx, window);
    cx.update(|cx| assert!(detail.read(cx).has_revealed()));

    // The window loses focus: masked again.
    gpui_kit::VisualTestContext::from_window(window, cx).deactivate_window();
    draw(cx, window);
    cx.update(|cx| assert!(!detail.read(cx).has_revealed()));

    // Ctrl C copies the password and says when it clears; Ctrl B the username.
    cx.update_window(window, |_, window, _| window.activate_window())
        .unwrap();
    with(cx, window, &shell, |shell, window, cx| {
        shell.select(ids[0], window, cx)
    });
    press(cx, window, "ctrl-c");
    assert_eq!(vault.clipboard.text().as_deref(), Some("Tq7#vL2m!Ox0-r9P"));
    cx.update(|cx| {
        assert_eq!(
            shell.read(cx).toast_message().as_deref(),
            Some("Password copied. Clears in 60 s.")
        )
    });
    press(cx, window, "ctrl-b");
    assert_eq!(vault.clipboard.text().as_deref(), Some("juan-dc"));
    press(cx, window, "ctrl-t");
    let code = vault.clipboard.text().unwrap();
    assert!(code.len() == 6 && code.chars().all(|c| c.is_ascii_digit()));

    // ↓ and ↑ move through the list.
    press(cx, window, "down");
    cx.update(|cx| assert_ne!(shell.read(cx).selected(), Some(ids[0])));
    press(cx, window, "up");
}

#[gpui_kit::test]
fn create_each_kind_with_the_menu_and_the_form(cx: &mut TestAppContext) {
    for window_size in SIZES {
        let vault = unlocked("create");
        let (window, shell) = open_shell(cx, &vault, window_size);
        // D-18: the create menu.
        with(cx, window, &shell, |shell, _, cx| {
            shell.open_create_menu(cx)
        });
        cx.update(|cx| assert!(shell.read(cx).has_dialog()));
        press(cx, window, "escape");
        cx.update(|cx| assert!(!shell.read(cx).has_dialog()));

        // D-19, D-22, D-23, D-24 and the password form.
        for kind in [
            ItemKind::Login,
            ItemKind::Card,
            ItemKind::Note,
            ItemKind::Identity,
            ItemKind::Password,
        ] {
            with(cx, window, &shell, |shell, window, cx| {
                shell.create(kind, window, cx)
            });
            let form = cx.update(|cx| shell.read(cx).form().cloned().expect("form open"));
            // Saving with no title is refused with a reason.
            with(cx, window, &shell, |_, _, cx| {
                form.update(cx, |form, cx| form.save(cx))
            });
            cx.update(|cx| assert!(shell.read(cx).form().is_some()));

            cx.update_window(window, |_, window, cx| {
                let title = form.read(cx).title_input().clone();
                title.update(cx, |input, cx| {
                    input.set_value(format!("A {kind:?}"), window, cx)
                });
            })
            .unwrap();
            if kind == ItemKind::Login {
                // The generator popover, and Use password.
                with(cx, window, &shell, |_, window, cx| {
                    form.update(cx, |form, cx| form.toggle_generator(window, cx))
                });
                with(cx, window, &shell, |_, window, cx| {
                    form.update(cx, |form, cx| {
                        form.use_password("generated-Pw-123!".into(), window, cx)
                    })
                });
                // A website and a custom field.
                with(cx, window, &shell, |_, window, cx| {
                    form.update(cx, |form, cx| {
                        form.add_website(window, cx);
                        form.add_custom_field(window, cx);
                    })
                });
                // D-20: the add two-factor dialog.
                with(cx, window, &shell, |_, window, cx| {
                    form.update(cx, |form, cx| form.open_add_totp(window, cx))
                });
                press(cx, window, "escape");
            }
            press(cx, window, "ctrl-s");
            cx.update(|cx| {
                let shell = shell.read(cx);
                assert!(shell.form().is_none(), "{kind:?} saved");
                let id = shell.selected().expect("the new item opens");
                let entry = vault.services.app.entry(id).unwrap();
                assert_eq!(entry.kind, kind);
                assert_eq!(entry.title, format!("A {kind:?}"));
            });
        }
        let login = cx.update(|_| {
            vault.services.app.entries(Filter {
                kind: Some(ItemKind::Login),
                ..Filter::default()
            })[0]
                .id
        });
        assert_eq!(
            vault
                .services
                .app
                .reveal(
                    login,
                    None,
                    vault
                        .services
                        .app
                        .item_detail(login)
                        .unwrap()
                        .password_field
                        .unwrap()
                )
                .unwrap()
                .as_str(),
            "generated-Pw-123!"
        );
    }
}

#[gpui_kit::test]
fn editing_with_changes_asks_before_discarding(cx: &mut TestAppContext) {
    let vault = unlocked("discard");
    let ids = fill(&vault);
    let (window, shell) = open_shell(cx, &vault, (1200., 740.));
    with(cx, window, &shell, |shell, window, cx| {
        shell.select(ids[0], window, cx)
    });
    press(cx, window, "ctrl-e");
    let form = cx.update(|cx| shell.read(cx).form().cloned().expect("edit form"));

    // No changes: Esc leaves at once.
    press(cx, window, "escape");
    cx.update(|cx| assert!(shell.read(cx).form().is_none() && !shell.read(cx).has_dialog()));

    // Changes: Esc asks (D-26); Keep editing stays; Discard leaves.
    press(cx, window, "ctrl-e");
    let form2 = cx.update(|cx| shell.read(cx).form().cloned().unwrap());
    assert_ne!(form.entity_id(), form2.entity_id());
    cx.update_window(window, |_, window, cx| {
        let title = form2.read(cx).title_input().clone();
        title.update(cx, |input, cx| {
            input.set_value("GitHub (edited)", window, cx)
        });
    })
    .unwrap();
    press(cx, window, "escape");
    cx.update(|cx| assert!(shell.read(cx).has_dialog()));
    press(cx, window, "escape");
    cx.update(|cx| assert!(shell.read(cx).form().is_some() && !shell.read(cx).has_dialog()));
    // Picking another item also asks.
    with(cx, window, &shell, |shell, window, cx| {
        shell.select(ids[1], window, cx)
    });
    cx.update(|cx| assert!(shell.read(cx).has_dialog()));
    with(cx, window, &shell, |shell, window, cx| {
        shell.confirm_dialog(window, cx)
    });
    cx.update(|cx| {
        assert_eq!(shell.read(cx).selected(), Some(ids[1]));
        assert_eq!(vault.services.app.entry(ids[0]).unwrap().title, "GitHub");
    });
}

#[gpui_kit::test]
fn search_finds_across_vaults_and_opens_with_the_keyboard(cx: &mut TestAppContext) {
    let vault = unlocked("search");
    let ids = fill(&vault);
    let work = vault
        .services
        .app
        .create_vault(&VaultMeta::new("Work", "sky", "bolt"))
        .unwrap();
    let work_login = vault
        .services
        .app
        .create_item(work, &login("GitHub work", "jdelacruz-acme"))
        .unwrap();
    let (window, shell) = open_shell(cx, &vault, (1200., 740.));
    draw(cx, window);

    // D-14: Ctrl F, type, results grouped by kind.
    press(cx, window, "ctrl-f");
    type_search(cx, window, &shell, "git");
    draw(cx, window);
    cx.update(|cx| {
        let found: Vec<Uuid> = shell
            .read(cx)
            .search_results()
            .iter()
            .map(|e| e.id)
            .collect();
        assert!(
            found.contains(&ids[0]) && found.contains(&work_login),
            "{found:?}"
        );
    });
    // Ctrl C copies without opening.
    search_key(cx, window, &shell, "ctrl-c");
    assert_eq!(vault.clipboard.text().as_deref(), Some("Tq7#vL2m!Ox0-r9P"));
    cx.update(|cx| assert_eq!(shell.read(cx).selected(), None));
    // ↓ then Enter opens the second result.
    search_key(cx, window, &shell, "down");
    let second = cx.update(|cx| shell.read(cx).search_results()[1].id);
    search_key(cx, window, &shell, "enter");
    cx.update(|cx| {
        assert_eq!(shell.read(cx).selected(), Some(second));
        assert!(shell.read(cx).search_results().is_empty());
    });

    // Nothing found: "Create login for …".
    press(cx, window, "ctrl-f");
    type_search(cx, window, &shell, "zzz-nothing");
    draw(cx, window);
    cx.update(|cx| assert!(shell.read(cx).search_results().is_empty()));
    search_key(cx, window, &shell, "enter");
    cx.update(|cx| {
        let form = shell.read(cx).form().cloned().expect("create login form");
        assert_eq!(form.read(cx).title_input().read(cx).value(), "zzz-nothing");
    });
}

#[gpui_kit::test]
fn multi_select_move_favourite_and_trash_with_undo(cx: &mut TestAppContext) {
    let vault = unlocked("bulk");
    let ids = fill(&vault);
    let family = vault
        .services
        .app
        .create_vault(&VaultMeta::new("Family", "coral", "user"))
        .unwrap();
    let (window, shell) = open_shell(cx, &vault, (1200., 740.));
    draw(cx, window);

    // D-15: Ctrl-click and Shift-click tick rows.
    with(cx, window, &shell, |shell, window, cx| {
        shell.click_row(ids[0], Modifiers::control(), window, cx);
        shell.click_row(ids[1], Modifiers::control(), window, cx);
    });
    cx.update(|cx| assert_eq!(shell.read(cx).checked().len(), 2));
    // D-34: move them to Family.
    with(cx, window, &shell, |shell, _, cx| {
        let ids = shell.checked().to_vec();
        shell.ask_move(ids, cx)
    });
    cx.update(|cx| assert!(shell.read(cx).has_dialog()));
    // The dialog starts on their vault; pick Family and move.
    with(cx, window, &shell, |shell, _, cx| {
        shell.set_move_target(family, cx)
    });
    with(cx, window, &shell, |shell, window, cx| {
        shell.confirm_move(window, cx)
    });
    assert_eq!(vault.services.app.entry(ids[0]).unwrap().vault_id, family);
    assert_eq!(vault.services.app.entry(ids[1]).unwrap().vault_id, family);

    // Select all, favourite, then Delete moves them to the trash at once.
    press(cx, window, "ctrl-a");
    cx.update(|cx| assert_eq!(shell.read(cx).checked().len(), ids.len()));
    with(cx, window, &shell, |shell, _, cx| {
        let ids = shell.checked().to_vec();
        shell.favourite(ids, cx)
    });
    assert!(vault.services.app.entry(ids[2]).unwrap().favourite);
    press(cx, window, "ctrl-a");
    press(cx, window, "delete");
    assert_eq!(vault.services.app.counts(), (0, ids.len()));
    cx.update(|cx| {
        assert_eq!(
            shell.read(cx).toast_message().as_deref(),
            Some("5 items moved to trash.")
        )
    });
    // D-25's sibling: the Undo toast brings them back.
    with(cx, window, &shell, |shell, window, cx| {
        shell.undo_trash(window, cx)
    });
    assert_eq!(vault.services.app.counts(), (ids.len(), 0));
}

#[gpui_kit::test]
fn trash_restore_delete_forever_and_empty_trash(cx: &mut TestAppContext) {
    for window_size in SIZES {
        let vault = unlocked("trash");
        let ids = fill(&vault);
        let (window, shell) = open_shell(cx, &vault, window_size);
        with(cx, window, &shell, |shell, window, cx| {
            shell.trash(ids.clone(), window, cx);
            shell.go(Place::Trash, window, cx);
        });
        // D-41: an item in the trash, with its days left.
        with(cx, window, &shell, |shell, window, cx| {
            shell.select(ids[0], window, cx)
        });
        cx.update(|cx| {
            let detail = shell.read(cx).detail().cloned().unwrap();
            assert_eq!(detail.read(cx).detail().unwrap().days_left, Some(30));
        });
        with(cx, window, &shell, |shell, _, cx| {
            shell.restore(vec![ids[0]], cx)
        });
        assert_eq!(vault.services.app.counts().0, 1);

        // D-25: Delete asks, then deletes forever.
        with(cx, window, &shell, |shell, window, cx| {
            shell.select(ids[1], window, cx)
        });
        press(cx, window, "delete");
        cx.update(|cx| assert!(shell.read(cx).has_dialog()));
        with(cx, window, &shell, |shell, window, cx| {
            shell.confirm_dialog(window, cx)
        });
        assert!(vault.services.app.entry(ids[1]).is_none());

        // D-42: Empty trash asks, then empties.
        with(cx, window, &shell, |shell, _, cx| shell.ask_empty_trash(cx));
        cx.update(|cx| assert!(shell.read(cx).has_dialog()));
        with(cx, window, &shell, |shell, window, cx| {
            shell.confirm_dialog(window, cx)
        });
        assert_eq!(vault.services.app.counts(), (1, 0));
    }
}

#[gpui_kit::test]
fn deleting_a_vault_asks_then_shows_another(cx: &mut TestAppContext) {
    for window_size in SIZES {
        let vault = unlocked("delete-vault");
        let app = vault.services.app.clone();
        let personal = app.vaults().unwrap()[0].id;
        let kept = app.create_item(personal, &login("GitHub", "juan")).unwrap();
        let (window, shell) = open_shell(cx, &vault, window_size);

        // The only vault: its edit dialog offers no Delete, and nothing asks.
        with(cx, window, &shell, |shell, window, cx| {
            shell.open_vault_dialog(Some(personal), window, cx)
        });
        press(cx, window, "escape");
        with(cx, window, &shell, |shell, _, cx| {
            shell.ask_delete_vault(personal, cx)
        });
        cx.update(|cx| assert!(!shell.read(cx).has_dialog()));

        let work = app
            .create_vault(&VaultMeta::new("Work", "sky", "bolt"))
            .unwrap();
        let jira = app.create_item(work, &login("Jira", "juan")).unwrap();
        with(cx, window, &shell, |shell, window, cx| {
            shell.go(Place::Vault(work), window, cx);
            shell.select(jira, window, cx);
        });
        // D-27 editing, with Delete vault, then the confirmation.
        with(cx, window, &shell, |shell, window, cx| {
            shell.open_vault_dialog(Some(work), window, cx)
        });
        press(cx, window, "escape");
        with(cx, window, &shell, |shell, _, cx| {
            shell.ask_delete_vault(work, cx)
        });
        cx.update(|cx| assert!(shell.read(cx).has_dialog()));
        // Cancel keeps everything.
        with(cx, window, &shell, |shell, window, cx| {
            shell.close_dialog(window, cx)
        });
        assert_eq!(app.vaults().unwrap().len(), 2);
        assert!(app.entry(jira).is_some());

        with(cx, window, &shell, |shell, _, cx| {
            shell.ask_delete_vault(work, cx)
        });
        with(cx, window, &shell, |shell, window, cx| {
            shell.confirm_dialog(window, cx)
        });
        let left: Vec<Uuid> = app.vaults().unwrap().iter().map(|v| v.id).collect();
        assert_eq!(left, [personal]);
        assert!(app.entry(jira).is_none());
        assert!(app.entry(kept).is_some());
        cx.update(|cx| {
            let shell = shell.read(cx);
            assert!(!shell.has_dialog());
            assert_eq!(
                shell.place(),
                Place::Vault(personal),
                "moved to another vault"
            );
            assert_eq!(shell.selected(), None);
            assert!(shell.detail().is_none());
            assert_eq!(
                shell.toast_message().as_ref().map(|t| t.as_ref()),
                Some("Work deleted.")
            );
        });
    }
}

#[gpui_kit::test]
fn history_restores_an_earlier_version(cx: &mut TestAppContext) {
    let vault = unlocked("history");
    let ids = fill(&vault);
    let app = vault.services.app.clone();
    let personal = app.vaults().unwrap()[0].id;
    let mut edited = login("GitHub", "juan-dc");
    if let ItemData::Login(login) = &mut edited.data {
        login.password = "second".into();
    }
    app.update_item(ids[0], personal, &edited).unwrap();
    let (window, shell) = open_shell(cx, &vault, (1200., 740.));
    with(cx, window, &shell, |shell, window, cx| {
        shell.select(ids[0], window, cx)
    });
    // D-16 from the detail's "View history".
    let detail = cx.update(|cx| shell.read(cx).detail().cloned().unwrap());
    cx.update(|cx| {
        detail.update(cx, |_, cx| {
            cx.emit(kandado_host::views::items::detail::DetailEvent::History)
        })
    });
    draw(cx, window);
    assert_eq!(app.history(ids[0]).unwrap().len(), 2);
    app.restore_version(ids[0], 1).unwrap();
    assert_eq!(
        app.reveal(ids[0], None, 1).unwrap().as_str(),
        "Tq7#vL2m!Ox0-r9P"
    );
    draw(cx, window);
}

#[gpui_kit::test]
fn vault_dialog_shortcuts_and_settings(cx: &mut TestAppContext) {
    for window_size in SIZES {
        let vault = unlocked("vault-dialog");
        let (window, shell) = open_shell(cx, &vault, window_size);
        // D-27 new, then edit.
        with(cx, window, &shell, |shell, window, cx| {
            shell.open_vault_dialog(None, window, cx)
        });
        cx.update(|cx| assert!(shell.read(cx).has_dialog()));
        cx.simulate_input(window, "Family");
        draw(cx, window);
        press(cx, window, "enter");
        let family = vault
            .services
            .app
            .vaults()
            .unwrap()
            .into_iter()
            .find(|vault| vault.meta.name == "Family")
            .expect("created")
            .id;
        assert_eq!(
            vault.services.app.vaults().unwrap().len(),
            2,
            "one vault created"
        );
        cx.update(|cx| assert_eq!(shell.read(cx).place(), Place::Vault(family)));
        with(cx, window, &shell, |shell, window, cx| {
            shell.open_vault_dialog(Some(family), window, cx)
        });
        press(cx, window, "escape");
        // D-51 with Ctrl /.
        press(cx, window, "ctrl-/");
        cx.update(|cx| assert!(shell.read(cx).has_dialog()));
        press(cx, window, "escape");

        // D-43 and the Security page with its new clipboard row.
        let services = vault.services.clone();
        let settings = cx.update(|cx| {
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: size(px(window_size.0), px(window_size.1)),
                })),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| SettingsView::new(services, window, cx))
            })
            .unwrap()
        });
        for page in Page::ALL {
            cx.update(|cx| settings.1.update(cx, |view, cx| view.select(page, cx)));
            draw(cx, settings.0);
        }
    }
}

#[gpui_kit::test]
fn locking_drops_the_vault_from_view(cx: &mut TestAppContext) {
    let vault = unlocked("lock");
    let ids = fill(&vault);
    let (window, shell) = open_shell(cx, &vault, (1200., 740.));
    with(cx, window, &shell, |shell, window, cx| {
        shell.select(ids[0], window, cx)
    });
    press(cx, window, "ctrl-c");
    assert!(vault.clipboard.text().is_some());
    vault
        .services
        .app
        .lock(kandado_app::lock::LockReason::Manual);
    // The clipboard is cleared and nothing can be read any more.
    assert_eq!(vault.clipboard.text(), None);
    assert!(vault.services.app.entries(Filter::default()).is_empty());
    assert!(vault.services.app.item_detail(ids[0]).is_err());
    std::thread::sleep(Duration::from_millis(10));
}

/// Types into the search field. It is drawn by the window's title bar, which
/// these tests do not open, so the text is set on the field itself.
fn type_search(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    shell: &Entity<VaultShell>,
    text: &str,
) {
    cx.update_window(window, |_, window, cx| {
        let search = shell.read(cx).search_input().clone();
        search.update(cx, |input, cx| input.set_value(text.to_owned(), window, cx));
        shell.update(cx, |shell, cx| shell.search_changed(cx));
    })
    .unwrap();
}

/// A key pressed in the search field. The window passes these to the shell
/// (`AppView` forwards them from its title bar); here that is done directly.
fn search_key(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    shell: &Entity<VaultShell>,
    key: &'static str,
) {
    let keystroke = gpui_kit::Keystroke::parse(key).unwrap();
    let handled = with(cx, window, shell, move |shell, window, cx| {
        assert!(shell.search_focused(window, cx), "search has the keyboard");
        shell.handle_keystroke(&keystroke, window, cx)
    });
    assert!(handled, "{key} is a search key");
}

/// D-54: the offline banner shows under the title bar while sync cannot
/// reach the server, offers Retry, and goes away on its own with the next
/// sync that works.
#[gpui_kit::test]
fn the_offline_banner_shows_and_clears_itself(cx: &mut TestAppContext) {
    use kandado_app::{
        Phase,
        sync::{SyncFailure, SyncState},
    };
    use kandado_host::app::AppView;

    for window_size in SIZES {
        let vault = unlocked("banner");
        let services = vault.services.clone();
        cx.executor().allow_parking();
        let (window, view) = cx.update(|cx| {
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
                cx.new(|cx| AppView::new(services, window, cx))
            })
            .expect("open test window")
        });
        // The window first asks which screen to open; then show the vault.
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(200));
        cx.run_until_parked();
        with_view(cx, window, &view, |view, window, cx| {
            view.show_phase(
                Phase::Unlocked {
                    email: EMAIL.into(),
                },
                window,
                cx,
            )
        });

        let retry_shown = |cx: &mut TestAppContext| {
            cx.update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window.try_find("sync-banner").is_some()
            })
            .unwrap()
        };
        assert!(!retry_shown(cx), "no banner while all is well");

        for state in [
            SyncState::Offline,
            SyncState::Failed(SyncFailure::Other),
            SyncState::Failed(SyncFailure::SignedOut),
        ] {
            vault.services.app.pretend_sync_state(state.clone());
            draw(cx, window);
            let shown = retry_shown(cx);
            // Every failure says so; only the first two offer Retry (below).
            assert!(shown, "{state:?}");
        }
        // A sync that works clears it.
        vault.services.app.pretend_sync_state(SyncState::Idle);
        draw(cx, window);
        assert!(!retry_shown(cx));
    }
}

fn with_view<V: 'static, R>(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    view: &Entity<V>,
    work: impl FnOnce(&mut V, &mut gpui_kit::Window, &mut gpui_kit::Context<V>) -> R,
) -> R {
    let result = cx
        .update_window(window, |_, window, cx| {
            view.update(cx, |v, cx| work(v, window, cx))
        })
        .expect("window is open");
    draw(cx, window);
    result
}

#[gpui_kit::test]
fn a_passkey_is_shown_and_cannot_be_edited(cx: &mut TestAppContext) {
    use kandado_model::webauthn::{self, CreateRequest};
    for window_size in SIZES {
        let vault = unlocked("passkey");
        let app = &vault.services.app;
        // A passkey as the extension makes one.
        let (passkey, registration) = webauthn::register(
            &CreateRequest {
                origin: "https://accounts.google.com".into(),
                rp_id: Some("google.com".into()),
                rp_name: "Google".into(),
                user_id: "dXNlci0x".into(),
                user_name: EMAIL.into(),
                user_display_name: "Juan".into(),
                challenge: "AAECAwQFBgcICQoLDA0ODw".into(),
                algorithms: vec![-7],
            },
            true,
        )
        .unwrap();
        let personal = app.vaults().unwrap()[0].id;
        let id = app
            .create_item(
                personal,
                &ItemContent::new("Google", ItemData::Passkey(passkey)),
            )
            .unwrap();
        let detail = app.item_detail(id).unwrap();
        assert_eq!(detail.passkey_site.as_deref(), Some("google.com"));
        let values: Vec<_> = detail
            .sections
            .iter()
            .flat_map(|section| &section.fields)
            .map(|field| {
                (
                    field.label.as_str(),
                    field.value.clone().unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(values[0], ("Username", EMAIL.to_owned()));
        assert_eq!(values[1], ("Website", "google.com".to_owned()));
        let key_id = &values[2];
        assert_eq!(key_id.0, "Key ID");
        assert_eq!(
            key_id.1,
            kandado_app::vault::key_id(&registration.credential_id)
        );
        assert_eq!(
            key_id.1.chars().count(),
            19,
            "four groups of four, joined by three dots"
        );
        assert!(!app.can_share(id), "a passkey is never shared");

        let (window, shell) = open_shell(cx, &vault, window_size);
        with(cx, window, &shell, |shell, window, cx| {
            shell.select(id, window, cx)
        });
        cx.update(|cx| assert!(shell.read(cx).detail().is_some()));
        // Ctrl E does nothing: a passkey has no form.
        press(cx, window, "ctrl-e");
        cx.update(|cx| {
            let shell = shell.read(cx);
            assert!(shell.form().is_none());
            assert!(shell.detail().is_some());
        });
        // Moving it to the trash, then it shows the trash state.
        with(cx, window, &shell, |shell, window, cx| {
            shell.trash(vec![id], window, cx)
        });
        with(cx, window, &shell, |shell, window, cx| {
            shell.go(Place::Trash, window, cx)
        });
        with(cx, window, &shell, |shell, window, cx| {
            shell.select(id, window, cx)
        });
        cx.update(|cx| assert!(shell.read(cx).detail().is_some()));
    }
}
