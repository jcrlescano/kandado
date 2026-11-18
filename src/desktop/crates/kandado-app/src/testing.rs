//! Help for tests, here and in the host: an account on this computer without
//! a server. Not used by the app itself.

use kandado_crypto::{account, kdf::KdfParams};
use kandado_vault::{Store, store::LocalAccount};
use uuid::Uuid;

use crate::service::Paths;

/// Writes an account with this master password into the data directory, as
/// if the person had signed in and set it up earlier. Call before creating
/// the [`crate::KandadoApp`]; it then starts locked and unlocks offline.
pub fn seed_account(paths: &Paths, email: &str, password: &str) {
    let user = Uuid::new_v4();
    let created =
        account::create(*user.as_bytes(), password, KdfParams::MIN).expect("test account keys");
    let store = Store::open(&paths.vault_db()).expect("test vault file");
    store
        .save_account(&LocalAccount {
            user_id: user,
            device_id: Uuid::new_v4(),
            email: email.to_owned(),
            display_name: "Juan Dela Cruz".to_owned(),
            bundle: created.bundle,
        })
        .expect("test account saved");
}
