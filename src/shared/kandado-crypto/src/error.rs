/// Why a cryptographic operation failed.
///
/// Decryption failures are deliberately not broken down further: a wrong key,
/// wrong associated data and a modified blob are indistinguishable to callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("the data could not be decrypted")]
    Decrypt,
    #[error("the encrypted data is malformed")]
    Format,
    #[error("the data uses format version {0}, which this version of Kandado does not support")]
    UnsupportedVersion(u8),
    #[error("the key derivation settings are outside the allowed range")]
    KdfParams,
    #[error("the key is not valid")]
    InvalidKey,
    #[error("the recovery phrase is not valid")]
    InvalidPhrase,
    #[error("the system random source failed")]
    Random,
}

pub type Result<T> = core::result::Result<T, Error>;
