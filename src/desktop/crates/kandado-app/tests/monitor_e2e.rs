//! Phase 7's exit criterion, end to end: seeded weak, reused, breached-site
//! and exposed-password items all appear in Health with the right status, and
//! leave the lists when fixed. Runs against the real `kandado-server`, whose
//! Monitor sources point at a fake Have I Been Pwned and 2FA Directory here.
//!
//! ```text
//! docker compose -f src/backend/docker-compose.yml up -d
//! cargo build -p kandado-server
//! DATABASE_URL=postgres://kandado:kandado@127.0.0.1:54329/kandado \
//!   cargo test -p kandado-app --test monitor_e2e -- --ignored
//! ```

mod common;

use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
};

use common::{computer, free_port, google, start_server_with, unique_email};
use kandado_app::{
    KandadoApp, Phase,
    monitor::{Health, Unchecked},
};
use kandado_model::{
    ItemContent, ItemData,
    health::PwnedQuery,
    item::{Login, Password, Website},
};
use uuid::Uuid;

/// A strong password that a fake Pwned Passwords says was leaked.
const LEAKED: &str = "Kx9!vQ2#mT7$wL4p";
const BREACH_DATE: &str = "2026-11-02";

/// A tiny HTTP/1.1 server with fixed answers, standing in for Have I Been
/// Pwned and the 2FA Directory. Records the paths it was asked for.
fn fake_sources() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let seen = asked.clone();
    let leaked = PwnedQuery::of(LEAKED);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
            // Skip the headers.
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap() <= 2 {
                    break;
                }
            }
            seen.lock().unwrap().push(path.clone());
            let body = if path == format!("/range/{}", leaked.prefix) {
                format!(
                    "{}:42\r\n0000000000000000000000000000000000A:0\r\n",
                    leaked.suffix
                )
            } else if path.starts_with("/range/") {
                "0000000000000000000000000000000000B:3\r\n".to_owned()
            } else if path == "/api/v3/breaches" {
                format!(
                    r#"[{{"Name":"Lakbay","Title":"Lakbay","Domain":"lakbay.ph","BreachDate":"{BREACH_DATE}",
                        "AddedDate":"2026-11-14T00:00:00Z","DataClasses":["Email addresses","Passwords"],
                        "IsVerified":true}}]"#
                )
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
    (base, asked)
}

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

async fn signed_in(server: &common::Server) -> std::sync::Arc<KandadoApp> {
    let app = computer("monitor", server);
    app.restore().await.unwrap();
    google(&app, server, &unique_email()).await.unwrap();
    app.create_master_password("correct horse battery staple".into())
        .await
        .unwrap();
    assert!(matches!(
        app.confirm_recovery_saved().await.unwrap(),
        Phase::Unlocked { .. }
    ));
    app
}

/// Changes the password of a login or a saved password.
fn change_password(app: &KandadoApp, id: Uuid, password: &str) {
    let item = app.item(id).unwrap();
    let mut content = item.content.clone();
    match &mut content.data {
        ItemData::Login(login) => login.password = password.into(),
        ItemData::Password(saved) => saved.password = password.into(),
        _ => unreachable!(),
    }
    app.update_item(id, item.vault_id, &content).unwrap();
}

#[tokio::test]
#[ignore = "needs PostgreSQL and a built kandado-server"]
async fn seeded_problems_appear_and_leave_when_fixed() {
    let (base, asked) = fake_sources();
    let pwned = base.clone();
    let hibp = format!("{base}/api/v3");
    let directory = format!("{base}/v3/totp.json");
    let server = start_server_with(
        free_port(),
        &[
            ("KANDADO_PWNED_URL", &pwned),
            ("KANDADO_HIBP_URL", &hibp),
            ("KANDADO_TWO_FACTOR_URL", &directory),
        ],
    )
    .await;
    let app = signed_in(&server).await;
    let vault = app.vaults().unwrap()[0].id;

    // A login on a breached site, whose password was set before the breach.
    let mut lakbay = login("Lakbay", "Vr8#nQ2!pL6$kZ4w", "lakbay.ph/login");
    if let ItemData::Login(l) = &mut lakbay.data {
        l.password_changed_at = kandado_model::health::date_to_unix("2026-01-15");
    }
    let lakbay = app.create_item(vault, &lakbay).unwrap();
    let weak = app
        .create_item(vault, &login("Router", "password1", "192.168.1.1"))
        .unwrap();
    let shopee = app
        .create_item(vault, &login("Shopee", "Same-Pass-Twice-99!", "shopee.ph"))
        .unwrap();
    let lazada = app
        .create_item(
            vault,
            &login("Lazada", "Same-Pass-Twice-99!", "lazada.com.ph"),
        )
        .unwrap();
    let leaked = app
        .create_item(
            vault,
            &ItemContent::new(
                "Saved password",
                ItemData::Password(Password {
                    password: LEAKED.into(),
                    ..Password::default()
                }),
            ),
        )
        .unwrap();
    let github = app
        .create_item(vault, &login("GitHub", "Gh-Unique-Pass-77!x", "github.com"))
        .unwrap();

    let health: Health = app.check_health().await.unwrap();
    assert!(health.unchecked.is_empty(), "{:?}", health.unchecked);
    assert_eq!(health.items_checked, 6);
    // Breached site.
    assert_eq!(health.breaches.len(), 1);
    let breach = &health.breaches[0];
    assert_eq!(
        (breach.name.as_str(), breach.domain.as_str()),
        ("Lakbay", "lakbay.ph")
    );
    assert_eq!(
        breach.items.iter().map(|i| i.id).collect::<Vec<_>>(),
        [lakbay]
    );
    assert_eq!(breach.data_classes, ["Email addresses", "Passwords"]);
    // Exposed password, with how often it was seen.
    assert_eq!(health.exposed.len(), 1);
    assert_eq!(
        (health.exposed[0].item.id, health.exposed[0].times_seen),
        (leaked, 42)
    );
    // Reused, as one group of two: Shopee and Lazada.
    assert_eq!(health.reused.len(), 1);
    let mut group: Vec<_> = health.reused[0].iter().map(|i| i.id).collect();
    group.sort();
    let mut expected = vec![shopee, lazada];
    expected.sort();
    assert_eq!(group, expected);
    // Weak.
    assert_eq!(
        health.weak.iter().map(|w| w.item.id).collect::<Vec<_>>(),
        [weak]
    );
    assert!(health.weak[0].score <= 1);
    // Could add two-factor: GitHub only (Lakbay is not in the directory).
    assert_eq!(health.two_factor.len(), 1);
    assert_eq!(health.two_factor[0].item.id, github);
    assert_eq!(
        health.two_factor[0].documentation.as_deref(),
        Some("https://docs.github.com/2fa")
    );
    assert_eq!(health.to_fix(), 5);
    assert_eq!(health.breach_count(), 2);

    // Only five-character prefixes left this computer.
    for path in asked.lock().unwrap().iter() {
        if let Some(prefix) = path.strip_prefix("/range/") {
            assert_eq!(prefix.len(), 5, "{path}");
        }
    }

    // Fixing each removes it.
    change_password(&app, lakbay, "New-Lakbay-Pass-2026!");
    change_password(&app, weak, "Rt7#wQ9!mP2$vL5x");
    change_password(&app, shopee, "Sh0pee-Own-Pass-41!");
    change_password(&app, leaked, "Fresh-Never-Leaked-88?");
    app.dismiss_two_factor(github).unwrap();

    let fixed = app.check_health().await.unwrap();
    assert!(fixed.breaches.is_empty(), "{:?}", fixed.breaches);
    assert!(fixed.exposed.is_empty());
    assert!(fixed.reused.is_empty());
    assert!(fixed.weak.is_empty());
    assert!(fixed.two_factor.is_empty());
    assert_eq!(fixed.to_fix(), 0);
}

#[tokio::test]
#[ignore = "needs PostgreSQL and a built kandado-server"]
async fn a_breach_marked_resolved_moves_to_resolved_and_offline_says_what_was_skipped() {
    let (base, _) = fake_sources();
    let hibp = format!("{base}/api/v3");
    let directory = format!("{base}/v3/totp.json");
    let server = start_server_with(
        free_port(),
        &[
            ("KANDADO_PWNED_URL", &base),
            ("KANDADO_HIBP_URL", &hibp),
            ("KANDADO_TWO_FACTOR_URL", &directory),
        ],
    )
    .await;
    let app = signed_in(&server).await;
    let vault = app.vaults().unwrap()[0].id;
    let mut lakbay = login("Lakbay", "Vr8#nQ2!pL6$kZ4w", "lakbay.ph");
    if let ItemData::Login(l) = &mut lakbay.data {
        l.password_changed_at = kandado_model::health::date_to_unix("2026-01-15");
    }
    let lakbay = app.create_item(vault, &lakbay).unwrap();

    app.resolve_breach(lakbay, "Lakbay").unwrap();
    let health = app.check_health().await.unwrap();
    assert!(health.breaches.is_empty());
    assert_eq!(health.resolved.len(), 1);
    assert_eq!(health.resolved[0].items[0].id, lakbay);

    // Without the server, weak and reused still work; the rest is listed as unchecked.
    app.create_item(vault, &login("Router", "password1", "192.168.1.1"))
        .unwrap();
    drop(server);
    let offline = app.check_health().await.unwrap();
    assert_eq!(offline.weak.len(), 1);
    assert_eq!(
        offline.unchecked,
        [
            Unchecked::Passwords,
            Unchecked::Breaches,
            Unchecked::TwoFactor
        ]
    );
}
