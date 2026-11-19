//! The unlocked session, held inside WebAssembly memory.
//!
//! JavaScript keeps the encrypted rows (IndexedDB) and a handle to a
//! [`Session`]. The user key, the account's private key and every vault key stay
//! in here. A call hands in ciphertext and gets back only what the caller needs
//! right now: a summary for a list, or one item's full content for a fill, a
//! copy or a reveal. Dropping the handle (`free()`) wipes the keys.
//!
//! All associated data comes from [`kandado_model::contexts`], the same
//! functions the desktop app uses, so both sides read each other's blobs.

use std::collections::HashMap;

use kandado_crypto::{
    Context, Purpose, SecretKey,
    account::{self, AccountBundle},
    aead,
    context::NO_ID,
    kdf::{self, KdfParams, SALT_LEN},
    seal::{self, PUBLIC_KEY_LEN, PrivateKey},
    wrap,
};
use kandado_model::{ItemContent, ItemData, Secret, VaultMeta, contexts, webauthn};
use kandado_protocol::AccountKeys;
use serde::Serialize;
use uuid::Uuid;
use wasm_bindgen::prelude::*;

type JsResult<T> = Result<T, JsError>;

/// Version of the PIN blob layout.
const PIN_FORMAT: u8 = 1;

/// The user key wrapped by a PIN-derived key. Purpose 3 is "the user key, wrapped
/// by a device key"; the object is empty, which no device key uses.
fn pin_context(user: [u8; 16]) -> Context {
    Context::new(Purpose::UserKeyByDevice, user, NO_ID, PIN_FORMAT)
}

fn check_pin(pin: &str) -> JsResult<()> {
    if pin.len() == 6 && pin.bytes().all(|byte| byte.is_ascii_digit()) {
        Ok(())
    } else {
        Err(JsError::new("a PIN is six digits"))
    }
}

fn id(text: &str) -> JsResult<[u8; 16]> {
    Uuid::parse_str(text)
        .map(|uuid| *uuid.as_bytes())
        .map_err(|_| JsError::new("an ID must be a UUID"))
}

fn bundle_from(keys_json: &str) -> JsResult<AccountBundle> {
    let keys: AccountKeys = serde_json::from_str(keys_json)
        .map_err(|_| JsError::new("the account keys are malformed"))?;
    let salt: [u8; SALT_LEN] = keys
        .kdf_salt
        .as_bytes()
        .try_into()
        .map_err(|_| JsError::new("the salt must be 16 bytes"))?;
    let public: [u8; PUBLIC_KEY_LEN] = keys
        .public_x25519
        .as_bytes()
        .try_into()
        .map_err(|_| JsError::new("the public key must be 32 bytes"))?;
    Ok(AccountBundle {
        version: keys.version,
        kdf_salt: salt,
        kdf_params: KdfParams {
            memory_kib: keys.kdf.memory_kib,
            iterations: keys.kdf.iterations,
            parallelism: keys.kdf.parallelism,
        },
        user_key_by_master: keys.user_key_by_master.into_bytes(),
        user_key_by_recovery: keys.user_key_by_recovery.into_bytes(),
        public_x25519: public,
        private_keys_enc: keys.private_keys_enc.into_bytes(),
    })
}

/// What a list row needs. No secret is in it.
#[derive(Serialize)]
struct Summary<'a> {
    id: &'a str,
    vault_id: &'a str,
    kind: &'static str,
    title: &'a str,
    /// A username, a masked card number, a name: whatever tells two items apart.
    subtitle: String,
    websites: Vec<WebsiteSummary<'a>>,
    favourite: bool,
    has_totp: bool,
    /// For a passkey: which site it is for and its credential id. Not secret;
    /// both are sent to the site at every sign-in.
    #[serde(skip_serializing_if = "Option::is_none")]
    passkey: Option<PasskeySummary<'a>>,
}

#[derive(Serialize)]
struct PasskeySummary<'a> {
    rp_id: &'a str,
    credential_id: &'a str,
}

#[derive(Serialize)]
struct WebsiteSummary<'a> {
    url: &'a str,
    match_mode: &'static str,
}

fn mode_name(mode: kandado_model::url_match::MatchMode) -> &'static str {
    use kandado_model::url_match::MatchMode;
    match mode {
        MatchMode::Domain => "domain",
        MatchMode::Host => "host",
        MatchMode::Never => "never",
    }
}

fn subtitle(content: &ItemContent) -> String {
    match &content.data {
        ItemData::Login(login) => login.username.clone(),
        ItemData::Alias(alias) => alias.address.clone(),
        ItemData::Card(card) => {
            let number = card.number.expose();
            let last: String = number
                .chars()
                .rev()
                .filter(char::is_ascii_digit)
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            if last.is_empty() {
                card.cardholder.clone()
            } else {
                format!("\u{2022}\u{2022}\u{2022}\u{2022} {last}")
            }
        }
        ItemData::Identity(identity) => format!("{} {}", identity.first_name, identity.last_name)
            .trim()
            .to_owned(),
        ItemData::Passkey(passkey) => passkey.user_name.clone(),
        ItemData::Note(_) | ItemData::Password(_) => String::new(),
    }
}

/// The extension's pairing key pair (`kandado_crypto::pairing`), and the code
/// both sides show while pairing.
#[wasm_bindgen(getter_with_clone)]
pub struct PairingKey {
    /// For `chrome.storage.local` only. It opens only what the paired desktop
    /// app seals to this extension, and only for this account.
    pub private_key: Vec<u8>,
    pub public_key: Vec<u8>,
    /// Six digits.
    pub code: String,
}

/// Makes the extension's pairing key pair.
#[wasm_bindgen(js_name = createPairingKey)]
pub fn create_pairing_key() -> JsResult<PairingKey> {
    let (private, public) = seal::generate_keypair()?;
    Ok(PairingKey {
        private_key: private.expose().to_vec(),
        public_key: public.as_bytes().to_vec(),
        code: kandado_crypto::pairing::code(public.as_bytes()),
    })
}

/// The six-digit code for a pairing public key.
#[wasm_bindgen(js_name = pairingCode)]
pub fn pairing_code(public_key: &[u8]) -> JsResult<String> {
    let public: [u8; PUBLIC_KEY_LEN] = public_key
        .try_into()
        .map_err(|_| JsError::new("a public key is 32 bytes"))?;
    Ok(kandado_crypto::pairing::code(&public))
}

/// An item sealed for storage and upload.
#[wasm_bindgen(getter_with_clone)]
pub struct SealedItem {
    /// The item key, wrapped by the vault key.
    pub key_wrapped: Vec<u8>,
    pub content_enc: Vec<u8>,
    /// `login`, `card`, ... The server files items by it (decision 11).
    pub kind: String,
}

/// A new passkey: the sealed item to store, and the answer for the page.
#[wasm_bindgen(getter_with_clone)]
pub struct SealedPasskey {
    pub key_wrapped: Vec<u8>,
    pub content_enc: Vec<u8>,
    /// `webauthn::Registration` as JSON: no secret, it goes to the site.
    pub response_json: String,
}

/// The unlocked keys of one account.
#[wasm_bindgen]
pub struct Session {
    user: [u8; 16],
    user_key: SecretKey,
    private: PrivateKey,
    #[cfg(feature = "testing")]
    public: kandado_crypto::seal::PublicKey,
    vault_keys: HashMap<[u8; 16], SecretKey>,
}

impl Session {
    fn open(user: [u8; 16], user_key: SecretKey, bundle: &AccountBundle) -> JsResult<Self> {
        let private = account::open_private_key(user, &user_key, bundle)?;
        Ok(Self {
            user,
            user_key,
            private,
            #[cfg(feature = "testing")]
            public: kandado_crypto::seal::PublicKey::from_bytes(bundle.public_x25519),
            vault_keys: HashMap::new(),
        })
    }

    fn vault_key(&self, vault: &[u8; 16]) -> JsResult<&SecretKey> {
        self.vault_keys
            .get(vault)
            .ok_or_else(|| JsError::new("that vault is not open"))
    }

    fn item_key(&self, vault: [u8; 16], item: [u8; 16], key_wrapped: &[u8]) -> JsResult<SecretKey> {
        Ok(wrap::unwrap_key(
            self.vault_key(&vault)?,
            &contexts::item_key(vault, item),
            key_wrapped,
        )?)
    }

    fn content(
        &self,
        vault_id: &str,
        item_id: &str,
        key_wrapped: &[u8],
        content_enc: &[u8],
    ) -> JsResult<ItemContent> {
        let (vault, item) = (id(vault_id)?, id(item_id)?);
        let key = self.item_key(vault, item, key_wrapped)?;
        let bytes = aead::decrypt(&key, &contexts::item_content(item), content_enc)?;
        Ok(ItemContent::decode(&bytes)?)
    }
}

#[wasm_bindgen]
impl Session {
    /// Unlocks with the master password. A wrong password and a tampered
    /// bundle give the same error, on purpose.
    #[wasm_bindgen(js_name = unlockWithPassword)]
    pub fn unlock_with_password(
        user_id: &str,
        keys_json: &str,
        password: &str,
    ) -> JsResult<Session> {
        let user = id(user_id)?;
        let bundle = bundle_from(keys_json)?;
        let user_key = account::unlock_with_password(user, &bundle, password)?;
        Self::open(user, user_key, &bundle)
    }

    /// Unlocks with a user key kept in `chrome.storage.session`, so a restarted
    /// service worker stays unlocked until auto-lock. Fails if the key is not
    /// the one this account's private key is encrypted under.
    #[wasm_bindgen(js_name = fromUserKey)]
    pub fn from_user_key(user_id: &str, keys_json: &str, user_key: &[u8]) -> JsResult<Session> {
        let user = id(user_id)?;
        let bundle = bundle_from(keys_json)?;
        Self::open(user, SecretKey::from_slice(user_key)?, &bundle)
    }

    /// Unlocks with the user key the desktop app sealed to this extension
    /// ("Unlock with desktop app", E-02). Fails unless it opens with the pairing
    /// key and is this account's user key.
    #[wasm_bindgen(js_name = fromDesktop)]
    pub fn from_desktop(
        user_id: &str,
        keys_json: &str,
        pairing_private: &[u8],
        pairing_id: &str,
        sealed: &[u8],
    ) -> JsResult<Session> {
        let user = id(user_id)?;
        let bundle = bundle_from(keys_json)?;
        let private = PrivateKey::from_slice(pairing_private)?;
        let user_key =
            kandado_crypto::pairing::open_user_key(user, id(pairing_id)?, &private, sealed)?;
        Self::open(user, user_key, &bundle)
    }

    /// Wraps the user key under a 6-digit PIN (E-02). The result goes to
    /// `chrome.storage.session` only: six digits cannot resist offline guessing,
    /// so it must never reach the disk. Layout: format byte, salt, wrapped key.
    #[wasm_bindgen(js_name = wrapWithPin)]
    pub fn wrap_with_pin(&self, pin: &str) -> JsResult<Vec<u8>> {
        check_pin(pin)?;
        let salt = kdf::generate_salt()?;
        let key = account::password_key(pin, &salt, &KdfParams::MIN)?;
        let wrapped = wrap::wrap_key(&key, &pin_context(self.user), &self.user_key)?;
        let mut out = Vec::with_capacity(1 + SALT_LEN + wrapped.len());
        out.push(PIN_FORMAT);
        out.extend_from_slice(&salt);
        out.extend_from_slice(&wrapped);
        Ok(out)
    }

    /// Unlocks with the PIN and a blob made by [`Session::wrap_with_pin`]. A
    /// wrong PIN fails; the caller counts attempts and drops the blob after five.
    #[wasm_bindgen(js_name = unlockWithPin)]
    pub fn unlock_with_pin(
        user_id: &str,
        keys_json: &str,
        pin: &str,
        blob: &[u8],
    ) -> JsResult<Session> {
        check_pin(pin)?;
        let user = id(user_id)?;
        let bundle = bundle_from(keys_json)?;
        let (&format, rest) = blob
            .split_first()
            .ok_or_else(|| JsError::new("the PIN blob is malformed"))?;
        if format != PIN_FORMAT || rest.len() <= SALT_LEN {
            return Err(JsError::new("the PIN blob is malformed"));
        }
        let (salt, wrapped) = rest.split_at(SALT_LEN);
        let salt: [u8; SALT_LEN] = salt
            .try_into()
            .map_err(|_| JsError::new("the PIN blob is malformed"))?;
        let key = account::password_key(pin, &salt, &KdfParams::MIN)?;
        let user_key = wrap::unwrap_key(&key, &pin_context(user), wrapped)?;
        Self::open(user, user_key, &bundle)
    }

    /// The user key, for `chrome.storage.session` only (memory-backed, cleared
    /// when the browser closes). Never write it anywhere else.
    #[wasm_bindgen(js_name = exportUserKey)]
    pub fn export_user_key(&self) -> Vec<u8> {
        self.user_key.expose().to_vec()
    }

    /// Opens a vault key sealed to this account. Call it for each vault a pull
    /// brings, and for each stored vault after an unlock.
    #[wasm_bindgen(js_name = openVault)]
    pub fn open_vault(&mut self, vault_id: &str, key_sealed: &[u8]) -> JsResult<()> {
        let vault = id(vault_id)?;
        let bytes = seal::open(
            &self.private,
            &contexts::vault_key(self.user, vault),
            key_sealed,
        )?;
        self.vault_keys
            .insert(vault, SecretKey::from_slice(&bytes)?);
        Ok(())
    }

    #[wasm_bindgen(js_name = hasVault)]
    pub fn has_vault(&self, vault_id: &str) -> JsResult<bool> {
        Ok(self.vault_keys.contains_key(&id(vault_id)?))
    }

    #[wasm_bindgen(js_name = forgetVault)]
    pub fn forget_vault(&mut self, vault_id: &str) -> JsResult<()> {
        self.vault_keys.remove(&id(vault_id)?);
        Ok(())
    }

    /// A vault's name, colour and icon, as JSON.
    #[wasm_bindgen(js_name = vaultMeta)]
    pub fn vault_meta(&self, vault_id: &str, meta_enc: &[u8]) -> JsResult<String> {
        let vault = id(vault_id)?;
        let bytes = aead::decrypt(
            self.vault_key(&vault)?,
            &contexts::vault_meta(vault),
            meta_enc,
        )?;
        let meta = VaultMeta::decode(&bytes)?;
        serde_json::to_string(&meta).map_err(|_| JsError::new("the vault could not be read"))
    }

    /// What a list row shows, as JSON. Holds no secret.
    #[wasm_bindgen(js_name = itemSummary)]
    pub fn item_summary(
        &self,
        vault_id: &str,
        item_id: &str,
        key_wrapped: &[u8],
        content_enc: &[u8],
    ) -> JsResult<String> {
        let content = self.content(vault_id, item_id, key_wrapped, content_enc)?;
        let (websites, has_totp) = match &content.data {
            ItemData::Login(login) => (
                login
                    .websites
                    .iter()
                    .map(|site| WebsiteSummary {
                        url: &site.url,
                        match_mode: mode_name(site.match_mode),
                    })
                    .collect(),
                login.totp.as_ref().is_some_and(|totp| !totp.is_empty()),
            ),
            _ => (Vec::new(), false),
        };
        serde_json::to_string(&Summary {
            id: item_id,
            vault_id,
            kind: content.kind().as_str(),
            title: &content.title,
            subtitle: subtitle(&content),
            websites,
            favourite: content.favourite,
            has_totp,
            passkey: match &content.data {
                ItemData::Passkey(passkey) => Some(PasskeySummary {
                    rp_id: &passkey.rp_id,
                    credential_id: &passkey.credential_id,
                }),
                _ => None,
            },
        })
        .map_err(|_| JsError::new("the item could not be read"))
    }

    /// One item's whole content, secrets included, as JSON. Ask for it only
    /// when the person fills, copies or reveals, and drop it afterwards.
    #[wasm_bindgen(js_name = itemContent)]
    pub fn item_content(
        &self,
        vault_id: &str,
        item_id: &str,
        key_wrapped: &[u8],
        content_enc: &[u8],
    ) -> JsResult<String> {
        let mut content = self.content(vault_id, item_id, key_wrapped, content_enc)?;
        // A passkey's private key never leaves the core: it only signs, here.
        if let ItemData::Passkey(passkey) = &mut content.data {
            passkey.private_key = Secret::default();
        }
        serde_json::to_string(&content).map_err(|_| JsError::new("the item could not be read"))
    }

    /// The two-factor code of a login at a time, or an empty string if it has none.
    #[wasm_bindgen(js_name = itemTotp)]
    pub fn item_totp(
        &self,
        vault_id: &str,
        item_id: &str,
        key_wrapped: &[u8],
        content_enc: &[u8],
        unix_seconds: f64,
    ) -> JsResult<String> {
        let content = self.content(vault_id, item_id, key_wrapped, content_enc)?;
        let ItemData::Login(login) = &content.data else {
            return Ok(String::new());
        };
        let Some(setup) = login.totp.as_ref().filter(|totp| !totp.is_empty()) else {
            return Ok(String::new());
        };
        Ok(kandado_model::totp::Totp::parse(setup.expose())?
            .code_at(unix_seconds as u64)
            .digits)
    }

    /// Encrypts a new item (JSON content) under a fresh item key. `item_id` is
    /// made by the caller (a UUIDv7), so work can start offline.
    #[wasm_bindgen(js_name = sealNewItem)]
    pub fn seal_new_item(
        &self,
        vault_id: &str,
        item_id: &str,
        content_json: &str,
    ) -> JsResult<SealedItem> {
        let (vault, item) = (id(vault_id)?, id(item_id)?);
        let content: ItemContent = serde_json::from_str(content_json)
            .map_err(|_| JsError::new("the item is malformed"))?;
        let key = SecretKey::generate()?;
        Ok(SealedItem {
            key_wrapped: wrap::wrap_key(
                self.vault_key(&vault)?,
                &contexts::item_key(vault, item),
                &key,
            )?,
            content_enc: aead::encrypt(&key, &contexts::item_content(item), &content.encode()?)?,
            kind: content.kind().as_str().to_owned(),
        })
    }

    /// Encrypts an edited item under its existing key.
    #[wasm_bindgen(js_name = sealItemUpdate)]
    pub fn seal_item_update(
        &self,
        vault_id: &str,
        item_id: &str,
        key_wrapped: &[u8],
        content_json: &str,
    ) -> JsResult<SealedItem> {
        let (vault, item) = (id(vault_id)?, id(item_id)?);
        let content: ItemContent = serde_json::from_str(content_json)
            .map_err(|_| JsError::new("the item is malformed"))?;
        // The JavaScript side only ever sees a passkey without its key, so an
        // edit from there would lose it.
        if matches!(content.data, ItemData::Passkey(_)) {
            return Err(JsError::new("a passkey is not edited here"));
        }
        let key = self.item_key(vault, item, key_wrapped)?;
        Ok(SealedItem {
            key_wrapped: key_wrapped.to_vec(),
            content_enc: aead::encrypt(&key, &contexts::item_content(item), &content.encode()?)?,
            kind: content.kind().as_str().to_owned(),
        })
    }

    /// Makes a passkey for `navigator.credentials.create` (E-25) and seals it
    /// as a new item. `request_json` is a `webauthn::CreateRequest`, whose
    /// `origin` the worker took from the browser, not from the page.
    /// `user_verified`: the session was opened with the master password or
    /// the desktop app, so the person is who unlocked the vault.
    #[wasm_bindgen(js_name = createPasskey)]
    pub fn create_passkey(
        &self,
        vault_id: &str,
        item_id: &str,
        request_json: &str,
        user_verified: bool,
    ) -> JsResult<SealedPasskey> {
        let (vault, item) = (id(vault_id)?, id(item_id)?);
        let request: webauthn::CreateRequest = serde_json::from_str(request_json)
            .map_err(|_| JsError::new("the passkey request is malformed"))?;
        let (passkey, registration) = webauthn::register(&request, user_verified)?;
        let title = if passkey.rp_name.trim().is_empty() {
            passkey.rp_id.clone()
        } else {
            passkey.rp_name.clone()
        };
        let content = ItemContent::new(title, ItemData::Passkey(passkey));
        let key = SecretKey::generate()?;
        Ok(SealedPasskey {
            key_wrapped: wrap::wrap_key(
                self.vault_key(&vault)?,
                &contexts::item_key(vault, item),
                &key,
            )?,
            content_enc: aead::encrypt(&key, &contexts::item_content(item), &content.encode()?)?,
            response_json: serde_json::to_string(&registration)
                .map_err(|_| JsError::new("the passkey could not be made"))?,
        })
    }

    /// Signs in with a stored passkey for `navigator.credentials.get` (E-26).
    /// Returns a `webauthn::Assertion` as JSON. Fails if the item is not a
    /// passkey or belongs to another site.
    #[wasm_bindgen(js_name = assertPasskey)]
    pub fn assert_passkey(
        &self,
        vault_id: &str,
        item_id: &str,
        key_wrapped: &[u8],
        content_enc: &[u8],
        request_json: &str,
        user_verified: bool,
    ) -> JsResult<String> {
        let content = self.content(vault_id, item_id, key_wrapped, content_enc)?;
        let ItemData::Passkey(passkey) = &content.data else {
            return Err(JsError::new("that item is not a passkey"));
        };
        let request: webauthn::GetRequest = serde_json::from_str(request_json)
            .map_err(|_| JsError::new("the passkey request is malformed"))?;
        let assertion = webauthn::assert(passkey, &request, user_verified)?;
        serde_json::to_string(&assertion).map_err(|_| JsError::new("the sign-in could not be made"))
    }

    /// The kind of an item (`login`, `card`, ...), for a push.
    #[wasm_bindgen(js_name = itemKind)]
    pub fn item_kind(
        &self,
        vault_id: &str,
        item_id: &str,
        key_wrapped: &[u8],
        content_enc: &[u8],
    ) -> JsResult<String> {
        Ok(self
            .content(vault_id, item_id, key_wrapped, content_enc)?
            .kind()
            .as_str()
            .to_owned())
    }
}

#[cfg(test)]
mod tests {
    //! Native tests: the `JsError` type only works on `wasm32`, so these check
    //! the pieces that do not return one. The whole `Session` is exercised in
    //! `tests/session.rs`, which runs in a browser.
    use kandado_model::item::{Login, Website};

    use super::*;

    #[test]
    fn a_card_subtitle_shows_only_the_last_four_digits() {
        let content = ItemContent::new(
            "Visa",
            ItemData::Card(kandado_model::item::Card {
                number: "4111 1111 1111 1234".into(),
                ..Default::default()
            }),
        );
        let text = subtitle(&content);
        assert!(text.ends_with("1234"), "{text}");
        assert!(!text.contains("4111"), "{text}");
    }

    #[test]
    fn a_login_subtitle_is_the_username_and_holds_no_password() {
        let content = ItemContent::new(
            "GitHub",
            ItemData::Login(Login {
                username: "juan".into(),
                password: "hunter2".into(),
                websites: vec![Website::default()],
                ..Default::default()
            }),
        );
        assert_eq!(subtitle(&content), "juan");
    }
}

#[cfg(feature = "testing")]
mod testing {
    use kandado_model::contexts;

    use super::*;

    /// A new account, for tests: its keys as the server would hold them.
    #[wasm_bindgen]
    pub struct TestAccount {
        keys_json: String,
        session: Session,
    }

    #[wasm_bindgen]
    impl TestAccount {
        #[wasm_bindgen(getter, js_name = keysJson)]
        pub fn keys_json(&self) -> String {
            self.keys_json.clone()
        }

        #[wasm_bindgen(js_name = intoSession)]
        pub fn into_session(self) -> Session {
            self.session
        }
    }

    /// A vault made inside a session: its sealed key and encrypted metadata.
    #[wasm_bindgen(getter_with_clone)]
    pub struct TestVault {
        pub key_sealed: Vec<u8>,
        pub meta_enc: Vec<u8>,
    }

    #[wasm_bindgen]
    impl Session {
        #[wasm_bindgen(js_name = createAccountForTests)]
        pub fn create_account_for_tests(user_id: &str, password: &str) -> JsResult<TestAccount> {
            use kandado_protocol::{KdfSettings, Opaque};
            let user = id(user_id)?;
            let new = account::create(user, password, KdfParams::MIN)?;
            let bundle = &new.bundle;
            let keys = AccountKeys {
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
            };
            let session = Session::open(user, new.user_key, &new.bundle)?;
            Ok(TestAccount {
                keys_json: serde_json::to_string(&keys)
                    .map_err(|_| JsError::new("the keys could not be written"))?,
                session,
            })
        }

        #[wasm_bindgen(js_name = createVaultForTests)]
        pub fn create_vault_for_tests(
            &mut self,
            vault_id: &str,
            name: &str,
        ) -> JsResult<TestVault> {
            let vault = id(vault_id)?;
            let key = SecretKey::generate()?;
            let key_sealed = seal::seal(
                &self.public,
                &contexts::vault_key(self.user, vault),
                key.expose(),
            )?;
            let meta_enc = aead::encrypt(
                &key,
                &contexts::vault_meta(vault),
                &VaultMeta::new(name, "brass", "vault").encode()?,
            )?;
            self.vault_keys.insert(vault, key);
            Ok(TestVault {
                key_sealed,
                meta_enc,
            })
        }
    }
}
