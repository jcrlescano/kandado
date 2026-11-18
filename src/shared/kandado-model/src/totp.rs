//! Two-factor codes: time-based one-time passwords (RFC 6238).
//!
//! A login stores its setup key as an `otpauth://totp/…` URI. A bare base32
//! key, as sites show under "can't scan the code?", is accepted too and
//! stored as a URI with the standard settings.
//!
//! Nothing here reads the clock; the caller passes the time.

use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use url::Url;
use zeroize::Zeroizing;

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Sha1,
    Sha256,
    Sha512,
}

impl Algorithm {
    fn name(self) -> &'static str {
        match self {
            Self::Sha1 => "SHA1",
            Self::Sha256 => "SHA256",
            Self::Sha512 => "SHA512",
        }
    }
}

/// A two-factor setup: the shared key and how codes are made from it.
#[derive(Clone, PartialEq, Eq)]
pub struct Totp {
    key: Zeroizing<Vec<u8>>,
    pub algorithm: Algorithm,
    /// Number of digits in a code: 6 to 8.
    pub digits: u8,
    /// How long a code lasts, in seconds.
    pub period: u32,
    /// Who issued the key, such as "GitHub". May be empty.
    pub issuer: String,
    /// The account the key is for. May be empty.
    pub account: String,
}

impl core::fmt::Debug for Totp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Totp")
            .field("algorithm", &self.algorithm)
            .field("digits", &self.digits)
            .field("period", &self.period)
            .field("issuer", &self.issuer)
            .field("account", &self.account)
            .finish_non_exhaustive()
    }
}

/// A code and how long it stays valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Code {
    /// The digits, with leading zeros kept.
    pub digits: String,
    /// Seconds until the next code, from 1 to the period.
    pub remaining: u32,
    pub period: u32,
}

impl Totp {
    /// Reads what a person pasted or a QR code contained: an `otpauth://totp/`
    /// URI, or a bare base32 key.
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();
        if input.len() >= 10 && input[..10].eq_ignore_ascii_case("otpauth://") {
            Self::from_uri(input)
        } else {
            Self::from_key(input)
        }
    }

    fn from_key(base32: &str) -> Result<Self> {
        Ok(Self {
            key: decode_key(base32)?,
            algorithm: Algorithm::Sha1,
            digits: 6,
            period: 30,
            issuer: String::new(),
            account: String::new(),
        })
    }

    fn from_uri(uri: &str) -> Result<Self> {
        let url = Url::parse(uri).map_err(|_| Error::TotpSetup)?;
        // Counter-based codes (hotp) are not supported.
        if url.host_str() != Some("totp") {
            return Err(Error::TotpSetup);
        }

        let label = url.path().trim_start_matches('/');
        let label = url::form_urlencoded::parse(label.replace('+', "%2B").as_bytes())
            .next()
            .map(|(text, _)| text.into_owned())
            .unwrap_or_default();
        let (label_issuer, account) = match label.split_once(':') {
            Some((issuer, account)) => (issuer.trim().to_owned(), account.trim().to_owned()),
            None => (String::new(), label.trim().to_owned()),
        };

        let mut totp = Self {
            key: Zeroizing::new(Vec::new()),
            algorithm: Algorithm::Sha1,
            digits: 6,
            period: 30,
            issuer: label_issuer,
            account,
        };
        for (name, value) in url.query_pairs() {
            match name.as_ref() {
                "secret" => totp.key = decode_key(&value)?,
                "issuer" => totp.issuer = value.into_owned(),
                "algorithm" => {
                    totp.algorithm = match value.to_ascii_uppercase().as_str() {
                        "SHA1" => Algorithm::Sha1,
                        "SHA256" => Algorithm::Sha256,
                        "SHA512" => Algorithm::Sha512,
                        _ => return Err(Error::TotpSetup),
                    }
                }
                "digits" => totp.digits = value.parse().map_err(|_| Error::TotpSetup)?,
                "period" => totp.period = value.parse().map_err(|_| Error::TotpSetup)?,
                _ => {}
            }
        }
        if totp.key.is_empty() || !(6..=8).contains(&totp.digits) || totp.period == 0 {
            return Err(Error::TotpSetup);
        }
        Ok(totp)
    }

    /// The `otpauth://` URI to store on the login.
    pub fn to_uri(&self) -> Zeroizing<String> {
        let mut url = Url::parse("otpauth://totp/").expect("constant URI");
        let label = if self.issuer.is_empty() {
            self.account.clone()
        } else {
            format!("{}:{}", self.issuer, self.account)
        };
        url.set_path(&label);
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("secret", &Zeroizing::new(BASE32_NOPAD.encode(&self.key)));
            if !self.issuer.is_empty() {
                query.append_pair("issuer", &self.issuer);
            }
            query.append_pair("algorithm", self.algorithm.name());
            query.append_pair("digits", &self.digits.to_string());
            query.append_pair("period", &self.period.to_string());
        }
        Zeroizing::new(url.into())
    }

    /// The code at `unix_seconds` and how long it has left.
    pub fn code_at(&self, unix_seconds: u64) -> Code {
        let period = u64::from(self.period);
        let counter = (unix_seconds / period).to_be_bytes();
        let hash = match self.algorithm {
            Algorithm::Sha1 => mac::<Hmac<Sha1>>(&self.key, &counter),
            Algorithm::Sha256 => mac::<Hmac<Sha256>>(&self.key, &counter),
            Algorithm::Sha512 => mac::<Hmac<Sha512>>(&self.key, &counter),
        };

        // RFC 4226 dynamic truncation.
        let offset = usize::from(hash[hash.len() - 1] & 0x0F);
        let binary = u32::from_be_bytes([
            hash[offset] & 0x7F,
            hash[offset + 1],
            hash[offset + 2],
            hash[offset + 3],
        ]);
        let value = binary % 10u32.pow(self.digits.into());

        Code {
            digits: format!("{value:0width$}", width = usize::from(self.digits)),
            remaining: (period - unix_seconds % period) as u32,
            period: self.period,
        }
    }
}

fn mac<M: Mac + KeyInit>(key: &[u8], message: &[u8]) -> Zeroizing<Vec<u8>> {
    let mut mac = <M as KeyInit>::new_from_slice(key).expect("HMAC accepts a key of any length");
    mac.update(message);
    Zeroizing::new(mac.finalize().into_bytes().to_vec())
}

/// Decodes a base32 key the way people paste them: any case, with spaces,
/// hyphens or padding.
fn decode_key(base32: &str) -> Result<Zeroizing<Vec<u8>>> {
    let cleaned: Zeroizing<String> = Zeroizing::new(
        base32
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-' && *c != '=')
            .map(|c| c.to_ascii_uppercase())
            .collect(),
    );
    let key = BASE32_NOPAD
        .decode(cleaned.as_bytes())
        .map(Zeroizing::new)
        .map_err(|_| Error::TotpSetup)?;
    if key.is_empty() {
        return Err(Error::TotpSetup);
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238 appendix B. The keys are the ASCII strings "1234567890…"
    /// repeated to 20, 32 and 64 bytes.
    fn rfc_totp(algorithm: Algorithm) -> Totp {
        let length = match algorithm {
            Algorithm::Sha1 => 20,
            Algorithm::Sha256 => 32,
            Algorithm::Sha512 => 64,
        };
        let key: Vec<u8> = b"1234567890".iter().copied().cycle().take(length).collect();
        Totp {
            key: Zeroizing::new(key),
            algorithm,
            digits: 8,
            period: 30,
            issuer: String::new(),
            account: String::new(),
        }
    }

    #[test]
    fn matches_the_rfc_6238_test_vectors() {
        let vectors: [(u64, &str, &str, &str); 6] = [
            (59, "94287082", "46119246", "90693936"),
            (1111111109, "07081804", "68084774", "25091201"),
            (1111111111, "14050471", "67062674", "99943326"),
            (1234567890, "89005924", "91819424", "93441116"),
            (2000000000, "69279037", "90698825", "38618901"),
            (20000000000, "65353130", "77737706", "47863826"),
        ];
        for (time, sha1, sha256, sha512) in vectors {
            assert_eq!(rfc_totp(Algorithm::Sha1).code_at(time).digits, sha1);
            assert_eq!(rfc_totp(Algorithm::Sha256).code_at(time).digits, sha256);
            assert_eq!(rfc_totp(Algorithm::Sha512).code_at(time).digits, sha512);
        }
    }

    #[test]
    fn the_countdown_runs_from_the_period_down_to_one() {
        let totp = Totp::parse("JBSWY3DPEHPK3PXP").unwrap();
        assert_eq!(totp.code_at(60).remaining, 30);
        assert_eq!(totp.code_at(61).remaining, 29);
        assert_eq!(totp.code_at(89).remaining, 1);
        assert_eq!(totp.code_at(90).remaining, 30);
        assert_eq!(totp.code_at(60).digits, totp.code_at(89).digits);
        assert_ne!(totp.code_at(89).digits, totp.code_at(90).digits);
    }

    #[test]
    fn a_bare_key_is_accepted_however_it_was_pasted() {
        let tidy = Totp::parse("JBSWY3DPEHPK3PXP").unwrap();
        for pasted in [
            "jbswy3dpehpk3pxp",
            "JBSW Y3DP EHPK 3PXP",
            "jbsw-y3dp-ehpk-3pxp",
            "  JBSWY3DPEHPK3PXP==\n",
        ] {
            assert_eq!(Totp::parse(pasted).unwrap(), tidy, "{pasted:?}");
        }
        assert_eq!(
            (tidy.algorithm, tidy.digits, tidy.period),
            (Algorithm::Sha1, 6, 30)
        );
        assert_eq!(tidy.code_at(0).digits.len(), 6);
    }

    #[test]
    fn reads_every_part_of_an_otpauth_uri() {
        let totp = Totp::parse(
            "otpauth://totp/ACME%20Co:juan@example.com?secret=JBSWY3DPEHPK3PXP&issuer=ACME%20Co&algorithm=SHA256&digits=8&period=60",
        )
        .unwrap();
        assert_eq!(totp.issuer, "ACME Co");
        assert_eq!(totp.account, "juan@example.com");
        assert_eq!(totp.algorithm, Algorithm::Sha256);
        assert_eq!(totp.digits, 8);
        assert_eq!(totp.period, 60);
        assert_eq!(totp.code_at(0).digits.len(), 8);
    }

    #[test]
    fn a_uri_with_only_a_secret_gets_the_standard_settings() {
        let totp = Totp::parse("otpauth://totp/juan?secret=JBSWY3DPEHPK3PXP").unwrap();
        assert_eq!(totp, {
            let mut expected = Totp::parse("JBSWY3DPEHPK3PXP").unwrap();
            expected.account = "juan".into();
            expected
        });
    }

    #[test]
    fn writing_a_uri_and_reading_it_back_changes_nothing() {
        let inputs = [
            "JBSWY3DPEHPK3PXP",
            "otpauth://totp/ACME%20Co:juan+work@example.com?secret=JBSWY3DPEHPK3PXP&issuer=ACME%20Co&algorithm=SHA512&digits=7&period=45",
        ];
        for input in inputs {
            let totp = Totp::parse(input).unwrap();
            assert_eq!(Totp::parse(&totp.to_uri()).unwrap(), totp);
        }
    }

    #[test]
    fn rejects_setups_that_cannot_produce_codes() {
        for bad in [
            "",
            "not base32 !!",
            "otpauth://hotp/juan?secret=JBSWY3DPEHPK3PXP&counter=1",
            "otpauth://totp/juan",
            "otpauth://totp/juan?secret=",
            "otpauth://totp/juan?secret=JBSWY3DPEHPK3PXP&digits=5",
            "otpauth://totp/juan?secret=JBSWY3DPEHPK3PXP&digits=9",
            "otpauth://totp/juan?secret=JBSWY3DPEHPK3PXP&period=0",
            "otpauth://totp/juan?secret=JBSWY3DPEHPK3PXP&algorithm=MD5",
        ] {
            assert_eq!(Totp::parse(bad).unwrap_err(), Error::TotpSetup, "{bad:?}");
        }
    }

    #[test]
    fn debug_output_never_shows_the_key() {
        let debug = format!("{:?}", rfc_totp(Algorithm::Sha1));
        assert!(!debug.contains("49") && !debug.contains("key"), "{debug}");
    }
}
