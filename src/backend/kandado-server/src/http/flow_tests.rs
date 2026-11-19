//! End-to-end tests of sign-in, keys and devices against a real PostgreSQL.
//!
//! Each test gets its own empty database (`#[sqlx::test]`). Run with
//! `DATABASE_URL` pointing at the docker-compose database. Google is replaced
//! by the development sign-in route, which shares every other line of the flow.

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use http_body_util::BodyExt;
use kandado_crypto::{
    account::{self, AccountBundle},
    kdf::KdfParams,
};
use kandado_protocol::{
    AccountKeys, ChangeMaster, DeviceInfo, DeviceList, KdfSettings, Me, Opaque, Problem,
    ProblemCode, TokenRequest, TokenResponse,
};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tower::ServiceExt;
use url::Url;

use super::{AppState, router};
use crate::{config::Config, db::users};

const REDIRECT: &str = "http://127.0.0.1:53211/callback";

pub(super) struct Api {
    pub(super) app: Router,
    pub(super) db: PgPool,
}

pub(super) struct Reply {
    pub(super) status: StatusCode,
    location: Option<String>,
    pub(super) body: Vec<u8>,
}

impl Reply {
    pub(super) fn json<T: DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).unwrap_or_else(|error| {
            panic!(
                "{error}: {} ({})",
                String::from_utf8_lossy(&self.body),
                self.status
            )
        })
    }

    pub(super) fn problem(&self) -> Problem {
        self.json()
    }
}

impl Api {
    pub(super) fn new(db: PgPool) -> Self {
        Self::with_config(db, Config::for_tests())
    }

    pub(super) fn with_config(db: PgPool, config: Config) -> Self {
        let app = router(AppState::new(db.clone(), config));
        Self { app, db }
    }

    pub(super) async fn send(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Reply {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let body = match body {
            Some(bytes) => {
                request = request.header(header::CONTENT_TYPE, "application/json");
                Body::from(bytes)
            }
            None => Body::empty(),
        };
        let response = self
            .app
            .clone()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let location = response
            .headers()
            .get(header::LOCATION)
            .map(|value| value.to_str().unwrap().to_owned());
        let body = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        Reply {
            status,
            location,
            body,
        }
    }

    pub(super) async fn get(&self, path: &str, token: Option<&str>) -> Reply {
        self.send(Method::GET, path, token, None).await
    }

    pub(super) async fn post<T: Serialize>(
        &self,
        path: &str,
        token: Option<&str>,
        body: &T,
    ) -> Reply {
        self.send(
            Method::POST,
            path,
            token,
            Some(serde_json::to_vec(body).unwrap()),
        )
        .await
    }

    pub(super) async fn put<T: Serialize>(
        &self,
        path: &str,
        token: Option<&str>,
        body: &T,
    ) -> Reply {
        self.send(
            Method::PUT,
            path,
            token,
            Some(serde_json::to_vec(body).unwrap()),
        )
        .await
    }

    pub(super) async fn delete(&self, path: &str, token: Option<&str>) -> Reply {
        self.send(Method::DELETE, path, token, None).await
    }

    /// Runs the dev sign-in and returns the redirect the client would get.
    async fn dev_flow(&self, email: &str, verifier: &str, step_up_token: Option<&str>) -> Url {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let mut url = Url::parse("http://x/v1/dev/sign-in").unwrap();
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("email", email)
                .append_pair("client", "desktop")
                .append_pair("challenge", &challenge)
                .append_pair("redirect", REDIRECT);
            if let Some(token) = step_up_token {
                query
                    .append_pair("purpose", "step_up")
                    .append_pair("access_token", token);
            }
        }
        let reply = self
            .get(&format!("{}?{}", url.path(), url.query().unwrap()), None)
            .await;
        assert_eq!(reply.status, StatusCode::SEE_OTHER, "{:?}", reply.body);
        Url::parse(&reply.location.unwrap()).unwrap()
    }

    async fn exchange(&self, code: &str, verifier: &str, device: bool) -> Reply {
        self.post(
            "/v1/auth/token",
            None,
            &TokenRequest {
                code: code.to_owned(),
                verifier: verifier.to_owned(),
                device: device.then(|| DeviceInfo {
                    name: "Test PC".into(),
                    platform: "Windows 11".into(),
                }),
            },
        )
        .await
    }

    /// Signs in a new device for `email`.
    pub(super) async fn sign_in(&self, email: &str) -> TokenResponse {
        let verifier = format!("verifier-{email}-{}", rand_suffix());
        let redirect = self.dev_flow(email, &verifier, None).await;
        let code = redirect
            .query_pairs()
            .find(|(key, _)| key == "code")
            .expect("redirect carries a code")
            .1
            .into_owned();
        let reply = self.exchange(&code, &verifier, true).await;
        assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
        reply.json()
    }

    async fn step_up(&self, email: &str, token: &str) -> TokenResponse {
        let verifier = format!("step-up-{}", rand_suffix());
        let redirect = self.dev_flow(email, &verifier, Some(token)).await;
        let code = redirect
            .query_pairs()
            .find(|(key, _)| key == "code")
            .expect("redirect carries a code")
            .1
            .into_owned();
        let reply = self.exchange(&code, &verifier, false).await;
        assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
        reply.json()
    }
}

fn rand_suffix() -> String {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).unwrap();
    URL_SAFE_NO_PAD.encode(bytes)
}

fn to_keys(bundle: &AccountBundle) -> AccountKeys {
    AccountKeys {
        version: bundle.version,
        kdf_salt: Opaque::new(bundle.kdf_salt.to_vec()),
        kdf: KdfSettings {
            memory_kib: bundle.kdf_params.memory_kib,
            iterations: bundle.kdf_params.iterations,
            parallelism: bundle.kdf_params.parallelism,
        },
        user_key_by_master: Opaque::new(bundle.user_key_by_master.clone()),
        user_key_by_recovery: Opaque::new(bundle.user_key_by_recovery.clone()),
        public_x25519: Opaque::new(bundle.public_x25519.to_vec()),
        private_keys_enc: Opaque::new(bundle.private_keys_enc.clone()),
    }
}

fn from_keys(keys: &AccountKeys) -> AccountBundle {
    AccountBundle {
        version: keys.version,
        kdf_salt: keys.kdf_salt.as_bytes().try_into().unwrap(),
        kdf_params: KdfParams {
            memory_kib: keys.kdf.memory_kib,
            iterations: keys.kdf.iterations,
            parallelism: keys.kdf.parallelism,
        },
        user_key_by_master: keys.user_key_by_master.as_bytes().to_vec(),
        user_key_by_recovery: keys.user_key_by_recovery.as_bytes().to_vec(),
        public_x25519: keys.public_x25519.as_bytes().try_into().unwrap(),
        private_keys_enc: keys.private_keys_enc.as_bytes().to_vec(),
    }
}

/// Makes the device's last Google sign-in older than the step-up window and
/// returns a token that says so.
async fn age_session(api: &Api, session: &TokenResponse) -> String {
    sqlx::query("UPDATE devices SET authenticated_at = now() - interval '1 hour' WHERE id = $1")
        .bind(session.device_id)
        .execute(&api.db)
        .await
        .unwrap();
    let reply = api
        .post(
            "/v1/auth/refresh",
            None,
            &kandado_protocol::RefreshRequest {
                refresh_token: session.refresh_token.clone().unwrap(),
            },
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK);
    reply.json::<TokenResponse>().access_token
}

#[sqlx::test(migrations = "../migrations")]
async fn signing_in_creates_the_account_and_a_device(db: PgPool) {
    let api = Api::new(db);
    let session = api.sign_in("ana@example.test").await;
    assert!(session.refresh_token.is_some());

    let me: Me = api.get("/v1/me", Some(&session.access_token)).await.json();
    assert_eq!(me.email, "ana@example.test");
    assert_eq!(me.id, session.user_id);
    assert_eq!(me.device_id, session.device_id);
    assert!(!me.has_keys);

    // Signing in again finds the same account.
    let again = api.sign_in("ana@example.test").await;
    assert_eq!(again.user_id, session.user_id);
    assert_ne!(again.device_id, session.device_id);
    assert_eq!(
        users::audit_kinds(&api.db, session.user_id).await.unwrap(),
        ["sign_in", "sign_in"]
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn routes_need_a_valid_token(db: PgPool) {
    let api = Api::new(db);
    for token in [None, Some("nonsense"), Some("k1.a.b")] {
        let reply = api.get("/v1/me", token).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
        assert_eq!(reply.problem().code, ProblemCode::Unauthorized);
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn a_code_works_once_and_only_with_its_verifier(db: PgPool) {
    let api = Api::new(db);
    let redirect = api.dev_flow("ana@example.test", "the-verifier", None).await;
    let code = redirect
        .query_pairs()
        .find(|(key, _)| key == "code")
        .unwrap()
        .1
        .into_owned();

    // A wrong verifier is refused, and it burns the code.
    let wrong = api.exchange(&code, "another-verifier", true).await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    let burnt = api.exchange(&code, "the-verifier", true).await;
    assert_eq!(burnt.status, StatusCode::BAD_REQUEST);

    // A fresh code works once.
    let redirect = api.dev_flow("ana@example.test", "the-verifier", None).await;
    let code = redirect
        .query_pairs()
        .find(|(key, _)| key == "code")
        .unwrap()
        .1
        .into_owned();
    assert_eq!(
        api.exchange(&code, "the-verifier", true).await.status,
        StatusCode::OK
    );
    assert_eq!(
        api.exchange(&code, "the-verifier", true).await.status,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn an_expired_code_is_refused(db: PgPool) {
    let api = Api::new(db);
    let redirect = api.dev_flow("ana@example.test", "the-verifier", None).await;
    let code = redirect
        .query_pairs()
        .find(|(key, _)| key == "code")
        .unwrap()
        .1
        .into_owned();
    sqlx::query("UPDATE auth_handoffs SET expires_at = now() - interval '1 second'")
        .execute(&api.db)
        .await
        .unwrap();
    assert_eq!(
        api.exchange(&code, "the-verifier", true).await.status,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn redirects_and_challenges_are_validated(db: PgPool) {
    let api = Api::new(db);
    let good = URL_SAFE_NO_PAD.encode(Sha256::digest(b"v"));
    for (redirect, challenge) in [
        ("https://evil.test/callback", good.as_str()),
        ("http://127.0.0.1:1/other", good.as_str()),
        (REDIRECT, "short"),
    ] {
        let mut url = Url::parse("http://x/v1/dev/sign-in").unwrap();
        url.query_pairs_mut()
            .append_pair("email", "ana@example.test")
            .append_pair("client", "desktop")
            .append_pair("challenge", challenge)
            .append_pair("redirect", redirect);
        let reply = api
            .get(&format!("{}?{}", url.path(), url.query().unwrap()), None)
            .await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{redirect}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn google_is_unavailable_when_not_configured(db: PgPool) {
    let api = Api::new(db);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(b"v"));
    let reply = api
        .get(
            &format!("/v1/auth/google/start?client=desktop&challenge={challenge}&redirect=http%3A%2F%2F127.0.0.1%3A5%2Fcallback"),
            None,
        )
        .await;
    assert_eq!(reply.status, StatusCode::SERVICE_UNAVAILABLE);
}

#[sqlx::test(migrations = "../migrations")]
async fn an_unknown_callback_state_is_refused(db: PgPool) {
    let api = Api::new(db);
    let reply = api
        .get("/v1/auth/google/callback?state=nope&code=abc", None)
        .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../migrations")]
async fn keys_are_stored_once_and_unlock_on_another_device(db: PgPool) {
    let api = Api::new(db);
    let first = api.sign_in("ana@example.test").await;

    // The first device creates the account keys with the real client code.
    let created = account::create(
        *first.user_id.as_bytes(),
        "correct horse battery",
        KdfParams::MIN,
    )
    .unwrap();
    let keys = to_keys(&created.bundle);
    let reply = api
        .post("/v1/account/keys", Some(&first.access_token), &keys)
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.body);

    // Never overwritten.
    let again = api
        .post("/v1/account/keys", Some(&first.access_token), &keys)
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert!(
        api.get("/v1/me", Some(&first.access_token))
            .await
            .json::<Me>()
            .has_keys
    );

    // A second device downloads them and unlocks with the password, or with
    // the recovery phrase.
    let second = api.sign_in("ana@example.test").await;
    let fetched: AccountKeys = api
        .get("/v1/account/keys", Some(&second.access_token))
        .await
        .json();
    assert_eq!(fetched, keys);
    let bundle = from_keys(&fetched);
    let user = *second.user_id.as_bytes();
    let key = account::unlock_with_password(user, &bundle, "correct horse battery").unwrap();
    assert_eq!(key.expose(), created.user_key.expose());
    let by_phrase = account::unlock_with_recovery(user, &bundle, &created.recovery).unwrap();
    assert_eq!(by_phrase.expose(), created.user_key.expose());
}

#[sqlx::test(migrations = "../migrations")]
async fn malformed_keys_are_refused_and_other_users_see_nothing(db: PgPool) {
    let api = Api::new(db);
    let ana = api.sign_in("ana@example.test").await;
    let created = account::create(*ana.user_id.as_bytes(), "pw", KdfParams::MIN).unwrap();

    let mut bad_len = to_keys(&created.bundle);
    bad_len.user_key_by_master = Opaque::new(vec![0; 10]);
    let mut weak = to_keys(&created.bundle);
    weak.kdf.iterations = 1;
    let mut version = to_keys(&created.bundle);
    version.version = 9;
    for keys in [bad_len, weak, version] {
        let reply = api
            .post("/v1/account/keys", Some(&ana.access_token), &keys)
            .await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    }

    let bob = api.sign_in("bob@example.test").await;
    let reply = api.get("/v1/account/keys", Some(&bob.access_token)).await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../migrations")]
async fn refresh_rotates_and_a_reused_token_revokes_the_device(db: PgPool) {
    let api = Api::new(db);
    let session = api.sign_in("ana@example.test").await;
    let first_refresh = session.refresh_token.clone().unwrap();

    let refresh = |token: String| {
        let api = &api;
        async move {
            api.post(
                "/v1/auth/refresh",
                None,
                &kandado_protocol::RefreshRequest {
                    refresh_token: token,
                },
            )
            .await
        }
    };

    let rotated: TokenResponse = refresh(first_refresh.clone()).await.json();
    assert_ne!(
        rotated.refresh_token.as_deref(),
        Some(first_refresh.as_str())
    );
    assert_eq!(rotated.device_id, session.device_id);
    assert_eq!(
        api.get("/v1/me", Some(&rotated.access_token)).await.status,
        StatusCode::OK
    );

    // The old token is used again: someone kept a copy. The device is revoked,
    // including the token that was just issued.
    let replay = refresh(first_refresh).await;
    assert_eq!(replay.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        refresh(rotated.refresh_token.clone().unwrap()).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        api.get("/v1/me", Some(&rotated.access_token)).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert!(
        users::audit_kinds(&api.db, session.user_id)
            .await
            .unwrap()
            .contains(&"refresh_token_reused".to_owned())
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn logout_ends_only_the_calling_device(db: PgPool) {
    let api = Api::new(db);
    let one = api.sign_in("ana@example.test").await;
    let two = api.sign_in("ana@example.test").await;
    assert_eq!(
        api.send(
            Method::POST,
            "/v1/auth/logout",
            Some(&one.access_token),
            None
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        api.get("/v1/me", Some(&one.access_token)).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        api.get("/v1/me", Some(&two.access_token)).await.status,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn a_device_can_be_revoked_from_another_and_only_by_its_owner(db: PgPool) {
    let api = Api::new(db);
    let laptop = api.sign_in("ana@example.test").await;
    let desktop = api.sign_in("ana@example.test").await;

    let list: DeviceList = api
        .get("/v1/devices", Some(&laptop.access_token))
        .await
        .json();
    assert_eq!(list.devices.len(), 2);
    let current: Vec<_> = list.devices.iter().filter(|d| d.current).collect();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].id, laptop.device_id);
    assert_eq!(current[0].name, "Test PC");

    // Someone else cannot touch it.
    let bob = api.sign_in("bob@example.test").await;
    let denied = api
        .delete(
            &format!("/v1/devices/{}", desktop.device_id),
            Some(&bob.access_token),
        )
        .await;
    assert_eq!(denied.status, StatusCode::NOT_FOUND);

    let revoked = api
        .delete(
            &format!("/v1/devices/{}", desktop.device_id),
            Some(&laptop.access_token),
        )
        .await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);
    assert_eq!(
        api.get("/v1/me", Some(&desktop.access_token)).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        api.get("/v1/devices", Some(&laptop.access_token))
            .await
            .json::<DeviceList>()
            .devices
            .len(),
        1
    );
    // Revoking twice finds nothing.
    assert_eq!(
        api.delete(
            &format!("/v1/devices/{}", desktop.device_id),
            Some(&laptop.access_token)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn sensitive_actions_need_a_recent_google_sign_in(db: PgPool) {
    let api = Api::new(db);
    let laptop = api.sign_in("ana@example.test").await;
    let other = api.sign_in("ana@example.test").await;
    let created = account::create(*laptop.user_id.as_bytes(), "old pw", KdfParams::MIN).unwrap();
    api.post(
        "/v1/account/keys",
        Some(&laptop.access_token),
        &to_keys(&created.bundle),
    )
    .await;

    // A session whose Google sign-in is an hour old is asked to confirm.
    let stale = age_session(&api, &laptop).await;
    let rewrap = account::rewrap_master(
        *laptop.user_id.as_bytes(),
        &created.user_key,
        "new pw",
        KdfParams::MIN,
    )
    .unwrap();
    let change = ChangeMaster {
        kdf_salt: Opaque::new(rewrap.kdf_salt.to_vec()),
        kdf: KdfSettings {
            memory_kib: rewrap.kdf_params.memory_kib,
            iterations: rewrap.kdf_params.iterations,
            parallelism: rewrap.kdf_params.parallelism,
        },
        user_key_by_master: Opaque::new(rewrap.user_key_by_master),
    };
    let refused = api
        .put("/v1/account/keys/master", Some(&stale), &change)
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert_eq!(refused.problem().code, ProblemCode::StepUpRequired);
    assert_eq!(
        api.send(
            Method::POST,
            "/v1/devices/revoke-others",
            Some(&stale),
            None
        )
        .await
        .problem()
        .code,
        ProblemCode::StepUpRequired
    );
    assert_eq!(
        api.delete("/v1/account", Some(&stale)).await.problem().code,
        ProblemCode::StepUpRequired
    );

    // Signing in with Google again for the same session makes it fresh,
    // and it stays fresh after a refresh.
    let fresh = api.step_up("ana@example.test", &stale).await;
    assert_eq!(fresh.device_id, laptop.device_id);
    assert!(fresh.refresh_token.is_none());
    let changed = api
        .put(
            "/v1/account/keys/master",
            Some(&fresh.access_token),
            &change,
        )
        .await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT, "{:?}", changed.body);

    let keys: AccountKeys = api
        .get("/v1/account/keys", Some(&fresh.access_token))
        .await
        .json();
    let bundle = from_keys(&keys);
    let user = *laptop.user_id.as_bytes();
    assert!(account::unlock_with_password(user, &bundle, "new pw").is_ok());
    assert!(account::unlock_with_password(user, &bundle, "old pw").is_err());
    assert!(account::unlock_with_recovery(user, &bundle, &created.recovery).is_ok());

    // Sign out other devices, now allowed.
    let revoked = api
        .send(
            Method::POST,
            "/v1/devices/revoke-others",
            Some(&fresh.access_token),
            None,
        )
        .await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);
    assert_eq!(
        api.get("/v1/me", Some(&other.access_token)).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        api.get("/v1/me", Some(&fresh.access_token)).await.status,
        StatusCode::OK
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn a_step_up_must_be_completed_by_the_same_account(db: PgPool) {
    let api = Api::new(db);
    let ana = api.sign_in("ana@example.test").await;
    let redirect = api
        .dev_flow("bob@example.test", "verifier", Some(&ana.access_token))
        .await;
    let pairs: Vec<_> = redirect.query_pairs().collect();
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].0, "error");
    assert_eq!(pairs[0].1, "wrong_account");
}

#[sqlx::test(migrations = "../migrations")]
async fn deleting_the_account_removes_everything(db: PgPool) {
    let api = Api::new(db);
    let ana = api.sign_in("ana@example.test").await;
    let created = account::create(*ana.user_id.as_bytes(), "pw", KdfParams::MIN).unwrap();
    api.post(
        "/v1/account/keys",
        Some(&ana.access_token),
        &to_keys(&created.bundle),
    )
    .await;

    // A fresh sign-in, so no step-up is needed.
    let reply = api.delete("/v1/account", Some(&ana.access_token)).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    let counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM users), (SELECT count(*) FROM oauth_identities),
                (SELECT count(*) FROM user_keys), (SELECT count(*) FROM devices),
                (SELECT count(*) FROM audit_events)",
    )
    .fetch_one(&api.db)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0, 0, 0, 0));
    assert_eq!(
        api.get("/v1/me", Some(&ana.access_token)).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn oversized_bodies_are_refused(db: PgPool) {
    let api = Api::new(db);
    let session = api.sign_in("ana@example.test").await;
    let reply = api
        .send(
            Method::POST,
            "/v1/account/keys",
            Some(&session.access_token),
            Some(vec![b' '; 200 * 1024]),
        )
        .await;
    assert_eq!(reply.status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[test]
fn the_servers_key_sizes_match_the_crypto_crate() {
    // The server checks lengths without depending on kandado-crypto for
    // anything but this test.
    assert_eq!(kandado_crypto::wrap::WRAPPED_LEN, 1 + 24 + 32 + 16);
    assert_eq!(kandado_crypto::kdf::SALT_LEN, 16);
    assert_eq!(kandado_crypto::seal::PUBLIC_KEY_LEN, 32);
    assert_eq!(kandado_crypto::kdf::KdfParams::MIN.memory_kib, 19 * 1024);
    assert_eq!(kandado_crypto::kdf::KdfParams::MAX.memory_kib, 1024 * 1024);
    assert_eq!(kandado_crypto::kdf::KdfParams::MIN.iterations, 2);
    assert_eq!(kandado_crypto::kdf::KdfParams::MAX.iterations, 16);
    assert_eq!(kandado_crypto::kdf::KdfParams::MAX.parallelism, 8);
    // A private key plus the AEAD overhead is the same size as a wrapped key.
    assert_eq!(
        kandado_crypto::seal::PRIVATE_KEY_LEN + kandado_crypto::aead::OVERHEAD,
        kandado_crypto::wrap::WRAPPED_LEN
    );
}
