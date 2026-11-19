//! The Monitor relay against a fake Have I Been Pwned and 2FA Directory on
//! the loopback: padding asked for, answers kept, lists filtered, stale
//! lists served when a source is down, and only signed-in devices served.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use kandado_protocol::{BreachList, ProblemCode, TwoFactorSites};
use sqlx::PgPool;

use super::flow_tests::Api;
use crate::{config::Config, monitor::Sources};

#[derive(Default)]
struct Upstream {
    range_hits: AtomicUsize,
    breach_hits: AtomicUsize,
    padded: AtomicBool,
    down: AtomicBool,
}

const RANGE: &str = "1E4C9B93F3F0682250B6CF8331B7EE68FD8:9659365\r\n\
                     0018A45C4D1DEF81644B54AB7F969B88D65:0\r\n";

const BREACHES: &str = r#"[
  {"Name":"Lakbay","Title":"Lakbay","Domain":"lakbay.ph","BreachDate":"2026-11-02",
   "AddedDate":"2026-11-14T08:00:00Z","DataClasses":["Email addresses","Passwords"],
   "IsVerified":true,"IsFabricated":false,"IsSpamList":false,"IsMalware":false},
  {"Name":"SpamList","Title":"A spam list","Domain":"spam.example","BreachDate":"2025-01-01",
   "AddedDate":"2025-02-01T00:00:00Z","DataClasses":["Email addresses"],
   "IsVerified":true,"IsSpamList":true},
  {"Name":"Unverified","Domain":"maybe.example","BreachDate":"2025-01-01","IsVerified":false},
  {"Name":"NoDomain","Domain":"","BreachDate":"2025-01-01","IsVerified":true},
  {"Name":"Fake","Domain":"fake.example","BreachDate":"2025-01-01","IsVerified":true,"IsFabricated":true}
]"#;

const DIRECTORY: &str = r#"[
  ["GitHub", {"domain":"github.com","tfa":["totp"],"documentation":"https://docs.github.com/2fa",
              "additional-domains":["github.io"],"keywords":["developer"]}],
  ["Example", {"domain":"Example.COM","tfa":["totp"],"documentation":"javascript:alert(1)","keywords":[]}]
]"#;

async fn range(
    State(up): State<Arc<Upstream>>,
    Path(prefix): Path<String>,
    headers: HeaderMap,
) -> Response {
    if up.down.load(Ordering::SeqCst) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    up.range_hits.fetch_add(1, Ordering::SeqCst);
    up.padded.store(
        headers.get("add-padding").and_then(|v| v.to_str().ok()) == Some("true"),
        Ordering::SeqCst,
    );
    if prefix == "BAD00" {
        return "<html>not a range answer</html>".into_response();
    }
    RANGE.into_response()
}

async fn breaches(State(up): State<Arc<Upstream>>) -> Response {
    if up.down.load(Ordering::SeqCst) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    up.breach_hits.fetch_add(1, Ordering::SeqCst);
    BREACHES.into_response()
}

async fn directory(State(up): State<Arc<Upstream>>) -> Response {
    if up.down.load(Ordering::SeqCst) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    DIRECTORY.into_response()
}

async fn fake_sources() -> (Arc<Upstream>, Sources) {
    let up = Arc::new(Upstream::default());
    let app = Router::new()
        .route("/range/{prefix}", get(range))
        .route("/api/v3/breaches", get(breaches))
        .route("/v3/totp.json", get(directory))
        .with_state(up.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (
        up,
        Sources {
            pwned: base.clone(),
            breaches: format!("{base}/api/v3"),
            two_factor: format!("{base}/v3/totp.json"),
        },
    )
}

async fn setup(db: PgPool) -> (Api, Arc<Upstream>, String) {
    let (up, sources) = fake_sources().await;
    let mut config = Config::for_tests();
    config.monitor = sources;
    let api = Api::with_config(db, config);
    let token = api.sign_in("ana@example.test").await.access_token;
    (api, up, token)
}

#[sqlx::test(migrations = "../migrations")]
async fn a_prefix_is_relayed_padded_and_kept(db: PgPool) {
    let (api, up, token) = setup(db).await;
    let reply = api.get("/v1/monitor/pwned/5baa6", Some(&token)).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(String::from_utf8(reply.body).unwrap(), RANGE);
    assert!(
        up.padded.load(Ordering::SeqCst),
        "padding is always asked for"
    );
    // Kept: the source is asked once, whatever the case of the prefix.
    let again = api.get("/v1/monitor/pwned/5BAA6", Some(&token)).await;
    assert_eq!(again.status, StatusCode::OK);
    assert_eq!(up.range_hits.load(Ordering::SeqCst), 1);

    for bad in ["5BAA", "5BAA61", "ZZZZZ", "5BA%2F"] {
        let reply = api
            .get(&format!("/v1/monitor/pwned/{bad}"), Some(&token))
            .await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{bad}");
    }
    // A source answering something else is not passed on, nor kept.
    let odd = api.get("/v1/monitor/pwned/BAD00", Some(&token)).await;
    assert_eq!(odd.status, StatusCode::SERVICE_UNAVAILABLE);
    let hits = up.range_hits.load(Ordering::SeqCst);
    api.get("/v1/monitor/pwned/BAD00", Some(&token)).await;
    assert_eq!(up.range_hits.load(Ordering::SeqCst), hits + 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn only_signed_in_devices_are_served(db: PgPool) {
    let (api, up, _) = setup(db).await;
    for path in [
        "/v1/monitor/pwned/5BAA6",
        "/v1/monitor/breaches",
        "/v1/monitor/2fa-sites",
    ] {
        assert_eq!(
            api.get(path, None).await.status,
            StatusCode::UNAUTHORIZED,
            "{path}"
        );
    }
    assert_eq!(up.range_hits.load(Ordering::SeqCst), 0);
}

#[sqlx::test(migrations = "../migrations")]
async fn the_breach_list_keeps_only_real_breaches_of_real_sites(db: PgPool) {
    let (api, up, token) = setup(db).await;
    let reply = api.get("/v1/monitor/breaches", Some(&token)).await;
    assert_eq!(reply.status, StatusCode::OK);
    let list: BreachList = reply.json();
    assert_eq!(list.breaches.len(), 1);
    let breach = &list.breaches[0];
    assert_eq!(
        (
            breach.name.as_str(),
            breach.domain.as_str(),
            breach.breach_date.as_str()
        ),
        ("Lakbay", "lakbay.ph", "2026-11-02")
    );
    assert_eq!(breach.data_classes, ["Email addresses", "Passwords"]);

    // Kept, and still served while the source is down.
    up.down.store(true, Ordering::SeqCst);
    let again: BreachList = api.get("/v1/monitor/breaches", Some(&token)).await.json();
    assert_eq!(again, list);
    assert_eq!(up.breach_hits.load(Ordering::SeqCst), 1);
}

#[sqlx::test(migrations = "../migrations")]
async fn with_no_list_yet_and_the_source_down_it_says_unavailable(db: PgPool) {
    let (api, up, token) = setup(db).await;
    up.down.store(true, Ordering::SeqCst);
    for path in [
        "/v1/monitor/breaches",
        "/v1/monitor/2fa-sites",
        "/v1/monitor/pwned/5BAA6",
    ] {
        let reply = api.get(path, Some(&token)).await;
        assert_eq!(reply.status, StatusCode::SERVICE_UNAVAILABLE, "{path}");
        assert_eq!(reply.problem().code, ProblemCode::Unavailable);
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn the_two_factor_list_names_every_domain_once_lower_case(db: PgPool) {
    let (api, _, token) = setup(db).await;
    let reply = api.get("/v1/monitor/2fa-sites", Some(&token)).await;
    assert_eq!(reply.status, StatusCode::OK);
    let list: TwoFactorSites = reply.json();
    let domains: Vec<_> = list.sites.iter().map(|site| site.domain.as_str()).collect();
    assert_eq!(domains, ["github.com", "github.io", "example.com"]);
    assert_eq!(
        list.sites[0].documentation.as_deref(),
        Some("https://docs.github.com/2fa")
    );
    // Only https links are passed on.
    assert_eq!(list.sites[2].documentation, None);
}
