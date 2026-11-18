//! Associated data that ties a ciphertext to where it belongs.
//!
//! Every encryption is bound to a [`Context`]: what the blob is for, who owns
//! it, which object it belongs to and the version of the plaintext format.
//! Decrypting with a different context fails, so the server cannot move a
//! blob to another row, user or column and have a client accept it.

/// An identifier as raw bytes (a UUID).
pub type Id = [u8; 16];

/// The all-zero identifier, for contexts with no owner or object.
pub const NO_ID: Id = [0; 16];

/// What a blob is for. The numbers are part of the format and never change
/// or get reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Purpose {
    /// The user key, wrapped by the master key. Owner: user.
    UserKeyByMaster = 1,
    /// The user key, wrapped by the recovery key. Owner: user.
    UserKeyByRecovery = 2,
    /// The user key, wrapped by a device key. Owner: user. Object: device.
    UserKeyByDevice = 3,
    /// The user's private keys, encrypted with the user key. Owner: user.
    PrivateKeys = 4,
    /// A vault key sealed to a member. Owner: member. Object: vault.
    VaultKey = 5,
    /// A vault's name, colour and icon, encrypted with the vault key. Object: vault.
    VaultMeta = 6,
    /// An item key wrapped by its vault key. Owner: vault. Object: item.
    ItemKey = 7,
    /// An item's content, encrypted with the item key. Object: item.
    ItemContent = 8,
    /// An item key sealed to a share recipient. Owner: recipient. Object: item.
    ItemShare = 9,
    /// An item snapshot encrypted with a secure link key. Object: link.
    SecureLink = 10,
    /// A whole export of the vault, encrypted with a key made from the master
    /// password. Owner and object: none.
    Export = 11,
    /// The user key, sealed to a paired browser extension by the desktop app.
    /// Owner: user. Object: pairing.
    UserKeyToExtension = 12,
}

impl TryFrom<u8> for Purpose {
    type Error = crate::Error;

    fn try_from(value: u8) -> crate::Result<Self> {
        Ok(match value {
            1 => Self::UserKeyByMaster,
            2 => Self::UserKeyByRecovery,
            3 => Self::UserKeyByDevice,
            4 => Self::PrivateKeys,
            5 => Self::VaultKey,
            6 => Self::VaultMeta,
            7 => Self::ItemKey,
            8 => Self::ItemContent,
            9 => Self::ItemShare,
            10 => Self::SecureLink,
            11 => Self::Export,
            12 => Self::UserKeyToExtension,
            _ => return Err(crate::Error::Format),
        })
    }
}

/// The binding for one blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    pub purpose: Purpose,
    pub owner: Id,
    pub object: Id,
    /// Version of the plaintext format inside the blob.
    pub format: u8,
}

const DOMAIN: &[u8; 7] = b"kandado";

/// Length of an encoded context in bytes.
pub const ENCODED_LEN: usize = DOMAIN.len() + 2 + 16 + 16;

impl Context {
    pub fn new(purpose: Purpose, owner: Id, object: Id, format: u8) -> Self {
        Self {
            purpose,
            owner,
            object,
            format,
        }
    }

    /// The bytes authenticated alongside the ciphertext. Fixed length and
    /// fixed layout, so two different contexts never encode the same way.
    pub fn to_bytes(&self) -> [u8; ENCODED_LEN] {
        let mut out = [0; ENCODED_LEN];
        out[..7].copy_from_slice(DOMAIN);
        out[7] = self.purpose as u8;
        out[8] = self.format;
        out[9..25].copy_from_slice(&self.owner);
        out[25..41].copy_from_slice(&self.object);
        out
    }
}
