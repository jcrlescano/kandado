//! WebAuthn for passkeys stored in Kandado: what an authenticator produces
//! for `navigator.credentials.create` (a registration) and `.get` (an
//! assertion), built byte for byte as the specification lays it out.
//!
//! Kandado is the authenticator here, so it also builds `clientDataJSON` from
//! the origin the browser reported for the calling frame. Only ES256 (-7) is
//! offered; a site that accepts nothing else is left to the browser.
//!
//! Every binary value crossing to JavaScript is base64url without padding,
//! as in the WebAuthn JSON forms.
//!
//! References: WebAuthn Level 3 §5.8.1 (client data), §6.1 (authenticator
//! data), §6.5 (attestation object, "none" format); RFC 8152 / 9053 (COSE
//! EC2 keys); RFC 8949 (CBOR), in the CTAP2 canonical key order.

use data_encoding::BASE64URL_NOPAD;
use kandado_crypto::{
    passkey::{ES256, PasskeyKey},
    random,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{Error, Result, item::Passkey};

/// Kandado's authenticator model, as relying parties see it in registrations.
/// `9d4e1a07-6c55-4b2e-8f3a-2b8c71e0d5a4`.
pub const AAGUID: [u8; 16] = [
    0x9d, 0x4e, 0x1a, 0x07, 0x6c, 0x55, 0x4b, 0x2e, 0x8f, 0x3a, 0x2b, 0x8c, 0x71, 0xe0, 0xd5, 0xa4,
];

/// Authenticator data flags (§6.1).
pub mod flags {
    /// User present: the person answered the prompt.
    pub const UP: u8 = 0x01;
    /// User verified: Kandado was unlocked with the master password.
    pub const UV: u8 = 0x04;
    /// Backup eligible: the passkey can be synced.
    pub const BE: u8 = 0x08;
    /// Backed up: it is synced.
    pub const BS: u8 = 0x10;
    /// Attested credential data is included.
    pub const AT: u8 = 0x40;
}

/// Bytes in a credential id Kandado makes.
const CREDENTIAL_ID_LEN: usize = 16;

/// What the page asked `navigator.credentials.create` for, plus the origin the
/// browser reported for the calling frame.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CreateRequest {
    pub origin: String,
    /// `rp.id`, or the origin's host when the page left it out.
    #[serde(default)]
    pub rp_id: Option<String>,
    #[serde(default)]
    pub rp_name: String,
    /// `user.id`, base64url.
    pub user_id: String,
    #[serde(default)]
    pub user_name: String,
    #[serde(default)]
    pub user_display_name: String,
    /// base64url.
    pub challenge: String,
    /// `pubKeyCredParams[].alg`.
    #[serde(default)]
    pub algorithms: Vec<i64>,
}

/// What the page asked `navigator.credentials.get` for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GetRequest {
    pub origin: String,
    #[serde(default)]
    pub rp_id: Option<String>,
    /// base64url.
    pub challenge: String,
}

/// The authenticator's answer to `create`, for `AuthenticatorAttestationResponse`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Registration {
    pub credential_id: String,
    pub client_data_json: String,
    pub attestation_object: String,
    pub authenticator_data: String,
    /// DER SubjectPublicKeyInfo, for `getPublicKey()`.
    pub public_key: String,
    pub public_key_algorithm: i64,
}

/// The authenticator's answer to `get`, for `AuthenticatorAssertionResponse`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Assertion {
    pub credential_id: String,
    pub client_data_json: String,
    pub authenticator_data: String,
    pub signature: String,
    pub user_handle: String,
}

fn b64(bytes: &[u8]) -> String {
    BASE64URL_NOPAD.encode(bytes)
}

fn from_b64(text: &str) -> Result<Vec<u8>> {
    BASE64URL_NOPAD
        .decode(text.trim_end_matches('=').as_bytes())
        .map_err(|_| Error::WebAuthn("a value is not base64url"))
}

/// The relying party id to use for a request from `origin`, checked as a
/// browser would (§5.1.3 step 8, §5.1.4 step 7): the page must be HTTPS (or
/// HTTP on the local machine), and the id must be its host or a registrable
/// domain the host is under, never a bare public suffix such as `co.uk`.
pub fn rp_id_for(origin: &str, requested: Option<&str>) -> Result<String> {
    let url = url::Url::parse(origin).map_err(|_| Error::WebAuthn("the origin is not a URL"))?;
    let host = url
        .host_str()
        .ok_or(Error::WebAuthn("the origin has no host"))?
        .to_ascii_lowercase();
    let local = matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]")
        || host.ends_with(".localhost");
    match url.scheme() {
        "https" => {}
        "http" if local => {}
        _ => return Err(Error::WebAuthn("passkeys need a secure page")),
    }
    let rp = match requested.map(str::trim).filter(|rp| !rp.is_empty()) {
        None => return Ok(host),
        Some(rp) => rp.trim_end_matches('.').to_ascii_lowercase(),
    };
    if rp == host {
        return Ok(rp);
    }
    let is_ip = |name: &str| name.parse::<std::net::IpAddr>().is_ok() || name.starts_with('[');
    let under = host
        .strip_suffix(&rp)
        .is_some_and(|rest| rest.ends_with('.'));
    let registrable = psl::domain_str(&rp).is_some();
    if under && registrable && !is_ip(&host) && !is_ip(&rp) {
        Ok(rp)
    } else {
        Err(Error::WebAuthn(
            "the relying party id does not belong to this site",
        ))
    }
}

/// `clientDataJSON` (§5.8.1), with its members in the order the
/// specification's limited verification algorithm expects.
fn client_data_json(kind: &str, challenge: &str, origin: &str) -> Vec<u8> {
    #[derive(Serialize)]
    struct ClientData<'a> {
        #[serde(rename = "type")]
        kind: &'a str,
        challenge: &'a str,
        origin: &'a str,
        #[serde(rename = "crossOrigin")]
        cross_origin: bool,
    }
    serde_json::to_vec(&ClientData {
        kind,
        challenge,
        origin,
        cross_origin: false,
    })
    .expect("client data serialises")
}

/// The origin as the client data writes it: scheme, host and a port only
/// when it is not the default.
fn serialised_origin(origin: &str) -> Result<String> {
    let url = url::Url::parse(origin).map_err(|_| Error::WebAuthn("the origin is not a URL"))?;
    Ok(url.origin().ascii_serialization())
}

fn challenge_of(text: &str) -> Result<String> {
    let bytes = from_b64(text)?;
    if bytes.is_empty() {
        return Err(Error::WebAuthn("the challenge is empty"));
    }
    // Re-encoded, so the client data always holds canonical base64url.
    Ok(b64(&bytes))
}

/// Authenticator data (§6.1): rpIdHash, flags, a zero signature counter (a
/// synced passkey cannot keep one honestly), then attested credential data.
fn authenticator_data(rp_id: &str, flags: u8, attested: Option<&[u8]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(37 + attested.map_or(0, <[u8]>::len));
    out.extend_from_slice(&Sha256::digest(rp_id.as_bytes()));
    out.push(flags);
    out.extend_from_slice(&0u32.to_be_bytes());
    if let Some(attested) = attested {
        out.extend_from_slice(attested);
    }
    out
}

// ---- minimal CBOR, enough for a COSE key and a "none" attestation --------

fn head(out: &mut Vec<u8>, major: u8, value: u64) {
    let major = major << 5;
    match value {
        0..=23 => out.push(major | value as u8),
        24..=0xff => out.extend_from_slice(&[major | 24, value as u8]),
        0x100..=0xffff => {
            out.push(major | 25);
            out.extend_from_slice(&(value as u16).to_be_bytes());
        }
        _ => {
            out.push(major | 26);
            out.extend_from_slice(&(value as u32).to_be_bytes());
        }
    }
}

fn int(out: &mut Vec<u8>, value: i64) {
    if value >= 0 {
        head(out, 0, value as u64);
    } else {
        head(out, 1, (-1 - value) as u64);
    }
}

fn bytes(out: &mut Vec<u8>, value: &[u8]) {
    head(out, 2, value.len() as u64);
    out.extend_from_slice(value);
}

fn text(out: &mut Vec<u8>, value: &str) {
    head(out, 3, value.len() as u64);
    out.extend_from_slice(value.as_bytes());
}

/// The public key as a COSE EC2 key: kty 2, alg -7, crv 1 (P-256), x, y.
/// Keys in CTAP2 canonical order: 1, 3, -1, -2, -3.
fn cose_es256(x: &[u8; 32], y: &[u8; 32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(77);
    head(&mut out, 5, 5);
    int(&mut out, 1);
    int(&mut out, 2);
    int(&mut out, 3);
    int(&mut out, ES256);
    int(&mut out, -1);
    int(&mut out, 1);
    int(&mut out, -2);
    bytes(&mut out, x);
    int(&mut out, -3);
    bytes(&mut out, y);
    out
}

/// The attestation object with "none" attestation (§6.5, §8.7).
fn attestation_none(auth_data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(auth_data.len() + 32);
    head(&mut out, 5, 3);
    text(&mut out, "fmt");
    text(&mut out, "none");
    text(&mut out, "attStmt");
    head(&mut out, 5, 0);
    text(&mut out, "authData");
    bytes(&mut out, auth_data);
    out
}

/// Makes a new passkey for `request` (§6.3.2). Returns the passkey to store
/// as an item and the answer for the page. `user_verified` says Kandado is
/// unlocked with the master password, so the UV flag is honest.
pub fn register(request: &CreateRequest, user_verified: bool) -> Result<(Passkey, Registration)> {
    if !request.algorithms.is_empty() && !request.algorithms.contains(&ES256) {
        return Err(Error::WebAuthn(
            "the site accepts no algorithm Kandado offers",
        ));
    }
    let rp_id = rp_id_for(&request.origin, request.rp_id.as_deref())?;
    let user_id = from_b64(&request.user_id)?;
    if user_id.is_empty() || user_id.len() > 64 {
        return Err(Error::WebAuthn("the user id must be 1 to 64 bytes"));
    }
    let challenge = challenge_of(&request.challenge)?;
    let origin = serialised_origin(&request.origin)?;

    let key = PasskeyKey::generate().map_err(|_| Error::Random)?;
    let mut credential_id = [0u8; CREDENTIAL_ID_LEN];
    random::fill(&mut credential_id).map_err(|_| Error::Random)?;
    let (x, y) = key.public_xy();
    let cose = cose_es256(&x, &y);

    let mut attested = Vec::with_capacity(16 + 2 + CREDENTIAL_ID_LEN + cose.len());
    attested.extend_from_slice(&AAGUID);
    attested.extend_from_slice(&(CREDENTIAL_ID_LEN as u16).to_be_bytes());
    attested.extend_from_slice(&credential_id);
    attested.extend_from_slice(&cose);
    let mut flag_bits = flags::UP | flags::BE | flags::BS | flags::AT;
    if user_verified {
        flag_bits |= flags::UV;
    }
    let auth_data = authenticator_data(&rp_id, flag_bits, Some(&attested));
    let client_data = client_data_json("webauthn.create", &challenge, &origin);

    let private = key.to_pkcs8_der().map_err(|_| Error::Format)?;
    let passkey = Passkey {
        rp_id: rp_id.clone(),
        rp_name: request.rp_name.clone(),
        credential_id: b64(&credential_id),
        user_handle: b64(&user_id),
        user_name: request.user_name.clone(),
        user_display_name: request.user_display_name.clone(),
        algorithm: ES256 as i32,
        private_key: Zeroizing::new(b64(&private)).as_str().into(),
        sign_count: 0,
        ..Passkey::default()
    };
    let registration = Registration {
        credential_id: b64(&credential_id),
        client_data_json: b64(&client_data),
        attestation_object: b64(&attestation_none(&auth_data)),
        authenticator_data: b64(&auth_data),
        public_key: b64(&key.public_spki_der().map_err(|_| Error::Format)?),
        public_key_algorithm: ES256,
    };
    Ok((passkey, registration))
}

/// Signs in with a stored passkey (§6.3.3). The request must come from a
/// page the passkey's relying party covers.
pub fn assert(passkey: &Passkey, request: &GetRequest, user_verified: bool) -> Result<Assertion> {
    let rp_id = rp_id_for(&request.origin, request.rp_id.as_deref())?;
    if rp_id != passkey.rp_id {
        return Err(Error::WebAuthn("this passkey belongs to another site"));
    }
    if i64::from(passkey.algorithm) != ES256 {
        return Err(Error::WebAuthn(
            "the passkey uses an algorithm Kandado cannot sign with",
        ));
    }
    let challenge = challenge_of(&request.challenge)?;
    let origin = serialised_origin(&request.origin)?;
    let der = Zeroizing::new(from_b64(passkey.private_key.expose())?);
    let key =
        PasskeyKey::from_pkcs8_der(&der).map_err(|_| Error::WebAuthn("the passkey is damaged"))?;

    let mut flag_bits = flags::UP | flags::BE | flags::BS;
    if user_verified {
        flag_bits |= flags::UV;
    }
    let auth_data = authenticator_data(&rp_id, flag_bits, None);
    let client_data = client_data_json("webauthn.get", &challenge, &origin);
    let mut signed = auth_data.clone();
    signed.extend_from_slice(&Sha256::digest(&client_data));
    Ok(Assertion {
        credential_id: passkey.credential_id.clone(),
        client_data_json: b64(&client_data),
        authenticator_data: b64(&auth_data),
        signature: b64(&key.sign(&signed)),
        user_handle: passkey.user_handle.clone(),
    })
}

#[cfg(test)]
mod tests {
    use p256_verify::verify;

    use super::*;

    /// Verification the way a relying party does it, with the same curve
    /// library but none of the code above: parse the COSE key out of the
    /// attestation object, then check the assertion's signature.
    mod p256_verify {
        use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier as _};

        pub fn verify(x: &[u8], y: &[u8], message: &[u8], der: &[u8]) -> bool {
            // Uncompressed SEC1: 0x04 || x || y.
            let point = [&[0x04][..], x, y].concat();
            let key = VerifyingKey::from_sec1_bytes(&point).unwrap();
            key.verify(message, &Signature::from_der(der).unwrap())
                .is_ok()
        }
    }

    fn create() -> CreateRequest {
        CreateRequest {
            origin: "https://login.example.com".into(),
            rp_id: Some("example.com".into()),
            rp_name: "Example".into(),
            user_id: b64(b"user-1234"),
            user_name: "juan@example.com".into(),
            user_display_name: "Juan".into(),
            challenge: b64(&[7; 32]),
            algorithms: vec![-8, -7, -257],
        }
    }

    fn get() -> GetRequest {
        GetRequest {
            origin: "https://example.com".into(),
            rp_id: None,
            challenge: b64(&[9; 32]),
        }
    }

    /// Reads the parts of a registration a relying party checks.
    struct Parsed {
        rp_id_hash: [u8; 32],
        flags: u8,
        counter: u32,
        aaguid: [u8; 16],
        credential_id: Vec<u8>,
        x: Vec<u8>,
        y: Vec<u8>,
    }

    fn parse(registration: &Registration) -> Parsed {
        let object = from_b64(&registration.attestation_object).unwrap();
        // {"fmt":"none","attStmt":{},"authData":h'…'}
        let prefix = b"\xa3\x63fmt\x64none\x67attStmt\xa0\x68authData";
        assert!(object.starts_with(prefix), "{object:02x?}");
        let rest = &object[prefix.len()..];
        // A byte string with a 1-byte (0x58) length.
        assert_eq!(rest[0], 0x58);
        let auth = &rest[2..2 + rest[1] as usize];
        assert_eq!(auth, from_b64(&registration.authenticator_data).unwrap());
        let len = u16::from_be_bytes([auth[53], auth[54]]) as usize;
        let cose = &auth[55 + len..];
        // kty 2, alg -7, crv 1, then -2: x and -3: y as 32-byte strings.
        assert_eq!(&cose[..9], b"\xa5\x01\x02\x03\x26\x20\x01\x21\x58");
        assert_eq!(cose[9], 32);
        assert_eq!(&cose[42..45], b"\x22\x58\x20");
        Parsed {
            rp_id_hash: auth[..32].try_into().unwrap(),
            flags: auth[32],
            counter: u32::from_be_bytes(auth[33..37].try_into().unwrap()),
            aaguid: auth[37..53].try_into().unwrap(),
            credential_id: auth[55..55 + len].to_vec(),
            x: cose[10..42].to_vec(),
            y: cose[45..77].to_vec(),
        }
    }

    #[test]
    fn a_registration_is_what_a_relying_party_expects() {
        let (passkey, registration) = register(&create(), true).unwrap();
        let parsed = parse(&registration);
        assert_eq!(
            parsed.rp_id_hash,
            <[u8; 32]>::from(Sha256::digest(b"example.com"))
        );
        assert_eq!(
            parsed.flags,
            flags::UP | flags::UV | flags::BE | flags::BS | flags::AT
        );
        assert_eq!(parsed.counter, 0);
        assert_eq!(parsed.aaguid, AAGUID);
        assert_eq!(b64(&parsed.credential_id), registration.credential_id);
        assert_eq!(passkey.credential_id, registration.credential_id);
        assert_eq!(passkey.rp_id, "example.com");
        assert_eq!(passkey.user_handle, b64(b"user-1234"));
        assert_eq!(passkey.algorithm, -7);

        let client: serde_json::Value =
            serde_json::from_slice(&from_b64(&registration.client_data_json).unwrap()).unwrap();
        assert_eq!(client["type"], "webauthn.create");
        assert_eq!(client["challenge"], b64(&[7; 32]));
        assert_eq!(client["origin"], "https://login.example.com");
        assert_eq!(client["crossOrigin"], false);
        assert!(
            String::from_utf8(from_b64(&registration.client_data_json).unwrap())
                .unwrap()
                .starts_with(r#"{"type":"webauthn.create","challenge":"#)
        );

        // getPublicKey() is the same point as the COSE key.
        let spki = from_b64(&registration.public_key).unwrap();
        assert_eq!(&spki[spki.len() - 64..spki.len() - 32], parsed.x.as_slice());
        assert_eq!(&spki[spki.len() - 32..], parsed.y.as_slice());
    }

    #[test]
    fn an_assertion_verifies_with_the_registered_key() {
        let (passkey, registration) = register(&create(), false).unwrap();
        let parsed = parse(&registration);
        assert_eq!(parsed.flags & flags::UV, 0);

        let assertion = assert(&passkey, &get(), true).unwrap();
        let auth = from_b64(&assertion.authenticator_data).unwrap();
        assert_eq!(auth.len(), 37);
        assert_eq!(&auth[..32], &parsed.rp_id_hash);
        assert_eq!(auth[32], flags::UP | flags::UV | flags::BE | flags::BS);
        let client = from_b64(&assertion.client_data_json).unwrap();
        let mut signed = auth.clone();
        signed.extend_from_slice(&Sha256::digest(&client));
        let signature = from_b64(&assertion.signature).unwrap();
        assert!(verify(&parsed.x, &parsed.y, &signed, &signature));
        // A different challenge is a different signed message.
        signed[40] ^= 1;
        assert!(!verify(&parsed.x, &parsed.y, &signed, &signature));
        assert_eq!(assertion.user_handle, b64(b"user-1234"));
        assert_eq!(assertion.credential_id, registration.credential_id);
        let client: serde_json::Value = serde_json::from_slice(&client).unwrap();
        assert_eq!(client["type"], "webauthn.get");
        assert_eq!(client["origin"], "https://example.com");
    }

    #[test]
    fn a_passkey_only_works_on_its_own_site() {
        let (passkey, _) = register(&create(), true).unwrap();
        for origin in ["https://sub.example.com", "https://example.com:8443"] {
            let request = GetRequest {
                origin: origin.into(),
                rp_id: Some("example.com".into()),
                ..get()
            };
            assert!(assert(&passkey, &request, true).is_ok(), "{origin}");
        }
        for (origin, rp) in [
            ("https://example.org", None),
            ("https://example.org", Some("example.com")),
            ("https://evil-example.com", Some("example.com")),
            ("https://sub.example.com", None),
        ] {
            let request = GetRequest {
                origin: origin.into(),
                rp_id: rp.map(Into::into),
                ..get()
            };
            assert!(assert(&passkey, &request, true).is_err(), "{origin} {rp:?}");
        }
    }

    #[test]
    fn the_relying_party_id_is_checked_as_a_browser_does() {
        let ok = |origin: &str, rp: Option<&str>| rp_id_for(origin, rp).unwrap();
        assert_eq!(ok("https://www.github.com", None), "www.github.com");
        assert_eq!(
            ok("https://www.github.com", Some("github.com")),
            "github.com"
        );
        assert_eq!(
            ok("https://a.b.example.co.uk", Some("example.co.uk")),
            "example.co.uk"
        );
        assert_eq!(ok("http://localhost:5173", None), "localhost");
        assert_eq!(ok("http://127.0.0.1:8080", None), "127.0.0.1");
        for (origin, rp) in [
            ("http://example.com", None),
            ("https://example.co.uk", Some("co.uk")),
            ("https://github.com", Some("www.github.com")),
            ("https://notgithub.com", Some("github.com")),
            ("https://127.0.0.1", Some("0.0.1")),
            ("file:///tmp/x.html", None),
            ("not a url", None),
        ] {
            assert!(rp_id_for(origin, rp).is_err(), "{origin} {rp:?}");
        }
    }

    #[test]
    fn refuses_what_kandado_cannot_do() {
        let only_rsa = CreateRequest {
            algorithms: vec![-257],
            ..create()
        };
        assert!(register(&only_rsa, true).is_err());
        let no_user = CreateRequest {
            user_id: String::new(),
            ..create()
        };
        assert!(register(&no_user, true).is_err());
        let long_user = CreateRequest {
            user_id: b64(&[1; 65]),
            ..create()
        };
        assert!(register(&long_user, true).is_err());
        let insecure = CreateRequest {
            origin: "http://example.com".into(),
            rp_id: None,
            ..create()
        };
        assert!(register(&insecure, true).is_err());
        // An empty list means the browser's default, which includes ES256.
        let defaults = CreateRequest {
            algorithms: vec![],
            ..create()
        };
        assert!(register(&defaults, true).is_ok());
    }

    #[test]
    fn cbor_lengths_use_the_shortest_head() {
        let mut out = Vec::new();
        head(&mut out, 2, 23);
        head(&mut out, 2, 24);
        head(&mut out, 2, 255);
        head(&mut out, 2, 256);
        int(&mut out, -7);
        int(&mut out, -25);
        assert_eq!(out, [0x57, 0x58, 24, 0x58, 255, 0x59, 1, 0, 0x26, 0x38, 24]);
    }
}
