//! The associated data of every blob a vault is made of, in one place.
//!
//! The desktop app and the browser extension both seal vault keys, wrap item
//! keys and encrypt content. If they built these contexts separately they could
//! drift apart and neither would open the other's data, so both call these
//! functions. The tests freeze the encoding.

use kandado_crypto::{Context, Purpose, context::NO_ID};

use crate::{item, link, vault};

/// Version of the plaintext inside a sealed vault key or a wrapped item key:
/// one raw 256-bit key.
pub const KEY_FORMAT: u8 = 1;

/// A vault key sealed to an account's public key. Owner: the account.
pub fn vault_key(user: [u8; 16], vault_id: [u8; 16]) -> Context {
    Context::new(Purpose::VaultKey, user, vault_id, KEY_FORMAT)
}

/// A vault's name, colour and icon, encrypted with the vault key.
pub fn vault_meta(vault_id: [u8; 16]) -> Context {
    Context::new(Purpose::VaultMeta, NO_ID, vault_id, vault::FORMAT_VERSION)
}

/// An item key, wrapped by its vault's key. Owner: the vault.
pub fn item_key(vault_id: [u8; 16], item_id: [u8; 16]) -> Context {
    Context::new(Purpose::ItemKey, vault_id, item_id, KEY_FORMAT)
}

/// An item's content, encrypted with its item key.
pub fn item_content(item_id: [u8; 16]) -> Context {
    Context::new(Purpose::ItemContent, NO_ID, item_id, item::FORMAT_VERSION)
}

/// A secure link's snapshot of an item, encrypted with the link key. Object: the link.
pub fn secure_link(link_id: [u8; 16]) -> Context {
    Context::new(Purpose::SecureLink, NO_ID, link_id, link::FORMAT_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_encoding_is_frozen() {
        // purpose, format, owner, object
        let bytes = item_key([1; 16], [2; 16]).to_bytes();
        assert_eq!(&bytes[..7], b"kandado");
        assert_eq!(bytes[7], 7);
        assert_eq!(bytes[8], 1);
        assert_eq!(&bytes[9..25], &[1; 16]);
        assert_eq!(&bytes[25..41], &[2; 16]);

        let bytes = item_content([3; 16]).to_bytes();
        assert_eq!((bytes[7], bytes[8]), (8, item::FORMAT_VERSION));
        assert_eq!(&bytes[9..25], &[0; 16]);
        assert_eq!(&bytes[25..41], &[3; 16]);

        let bytes = vault_key([4; 16], [5; 16]).to_bytes();
        assert_eq!((bytes[7], bytes[8]), (5, 1));
        let bytes = vault_meta([6; 16]).to_bytes();
        assert_eq!((bytes[7], bytes[8]), (6, vault::FORMAT_VERSION));
        let bytes = secure_link([7; 16]).to_bytes();
        assert_eq!((bytes[7], bytes[8]), (10, link::FORMAT_VERSION));
        assert_eq!(&bytes[9..25], &[0; 16]);
        assert_eq!(&bytes[25..41], &[7; 16]);
    }
}
