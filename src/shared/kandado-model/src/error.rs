#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("the item is malformed")]
    Format,
    #[error("the item uses format version {0}, which this version of Kandado does not support")]
    UnsupportedVersion(u8),
    #[error("the generator settings are outside the allowed range")]
    GeneratorOptions,
    #[error("the two-factor setup key is not valid")]
    TotpSetup,
    #[error("the system random source failed")]
    Random,
    #[error("this kind of item cannot be shared by link")]
    NotShareable,
    #[error("the link is incomplete or its key is wrong")]
    LinkKey,
}

pub type Result<T> = core::result::Result<T, Error>;
