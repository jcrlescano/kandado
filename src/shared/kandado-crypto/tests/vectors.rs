//! Known-answer tests against `vectors/v1.json`.
//!
//! The same file is checked by the WebAssembly build in a browser, so every
//! client is proven to read the same bytes. The vectors are frozen: a change
//! to this crate that makes them fail has broken compatibility with stored data.
//!
//! To add vectors for a new format, extend `write_vectors` and run
//! `cargo test -p kandado-crypto --test vectors -- --ignored write_vectors`.
//! Never regenerate existing entries.

use kandado_crypto::{
    Context, Purpose, SecretKey, aead,
    kdf::{self, KdfParams},
    recovery::RecoveryPhrase,
    seal, wrap,
};
use serde::{Deserialize, Serialize};

const VECTORS: &str = include_str!("../vectors/v1.json");

#[derive(Serialize, Deserialize)]
struct Vectors {
    context: Vec<ContextVector>,
    aead: Vec<AeadVector>,
    kdf: Vec<KdfVector>,
    wrap: Vec<WrapVector>,
    seal: Vec<SealVector>,
    recovery: Vec<RecoveryVector>,
}

#[derive(Serialize, Deserialize, Clone)]
struct ContextFields {
    purpose: u8,
    owner: String,
    object: String,
    format: u8,
}

impl ContextFields {
    fn new(purpose: Purpose, owner: u8, object: u8, format: u8) -> Self {
        Self {
            purpose: purpose as u8,
            owner: hex::encode([owner; 16]),
            object: hex::encode([object; 16]),
            format,
        }
    }

    fn context(&self) -> Context {
        Context::new(
            Purpose::try_from(self.purpose).unwrap(),
            bytes(&self.owner),
            bytes(&self.object),
            self.format,
        )
    }
}

#[derive(Serialize, Deserialize)]
struct ContextVector {
    #[serde(flatten)]
    fields: ContextFields,
    bytes: String,
}

#[derive(Serialize, Deserialize)]
struct AeadVector {
    key: String,
    context: ContextFields,
    plaintext: String,
    blob: String,
}

#[derive(Serialize, Deserialize)]
struct KdfVector {
    password: String,
    salt: String,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    key: String,
}

#[derive(Serialize, Deserialize)]
struct WrapVector {
    wrapping_key: String,
    context: ContextFields,
    key: String,
    blob: String,
}

#[derive(Serialize, Deserialize)]
struct SealVector {
    private_key: String,
    public_key: String,
    context: ContextFields,
    plaintext: String,
    blob: String,
}

#[derive(Serialize, Deserialize)]
struct RecoveryVector {
    phrase: String,
    key: String,
}

fn bytes<const N: usize>(hex_text: &str) -> [u8; N] {
    hex::decode(hex_text).unwrap().try_into().unwrap()
}

fn key(hex_text: &str) -> SecretKey {
    SecretKey::from_bytes(bytes(hex_text))
}

fn load() -> Vectors {
    serde_json::from_str(VECTORS).unwrap()
}

#[test]
fn context_encoding() {
    let vectors = load().context;
    assert!(!vectors.is_empty());
    for vector in vectors {
        assert_eq!(
            hex::encode(vector.fields.context().to_bytes()),
            vector.bytes
        );
    }
}

#[test]
fn aead_decrypts_known_blobs() {
    let vectors = load().aead;
    assert!(!vectors.is_empty());
    for vector in vectors {
        let plaintext = aead::decrypt(
            &key(&vector.key),
            &vector.context.context(),
            &hex::decode(&vector.blob).unwrap(),
        )
        .unwrap();
        assert_eq!(hex::encode(plaintext.as_slice()), vector.plaintext);
    }
}

#[test]
fn kdf_derives_known_keys() {
    let vectors = load().kdf;
    assert!(!vectors.is_empty());
    for vector in vectors {
        let params = KdfParams {
            memory_kib: vector.memory_kib,
            iterations: vector.iterations,
            parallelism: vector.parallelism,
        };
        let derived =
            kdf::derive_master_key(vector.password.as_bytes(), &bytes(&vector.salt), &params)
                .unwrap();
        assert_eq!(hex::encode(derived.expose()), vector.key);
    }
}

#[test]
fn wrap_unwraps_known_blobs() {
    let vectors = load().wrap;
    assert!(!vectors.is_empty());
    for vector in vectors {
        let unwrapped = wrap::unwrap_key(
            &key(&vector.wrapping_key),
            &vector.context.context(),
            &hex::decode(&vector.blob).unwrap(),
        )
        .unwrap();
        assert_eq!(hex::encode(unwrapped.expose()), vector.key);
    }
}

#[test]
fn seal_opens_known_blobs() {
    let vectors = load().seal;
    assert!(!vectors.is_empty());
    for vector in vectors {
        let private = seal::PrivateKey::from_bytes(bytes(&vector.private_key));
        assert_eq!(
            hex::encode(private.public_key().unwrap().as_bytes()),
            vector.public_key
        );
        let plaintext = seal::open(
            &private,
            &vector.context.context(),
            &hex::decode(&vector.blob).unwrap(),
        )
        .unwrap();
        assert_eq!(hex::encode(plaintext.as_slice()), vector.plaintext);
    }
}

#[test]
fn recovery_phrases_give_known_keys() {
    let vectors = load().recovery;
    assert!(!vectors.is_empty());
    for vector in vectors {
        let phrase = RecoveryPhrase::parse(&vector.phrase).unwrap();
        assert_eq!(
            hex::encode(phrase.recovery_key().unwrap().expose()),
            vector.key
        );
    }
}

/// Writes `vectors/v1.json`. Run once per format; see the module comment.
#[test]
#[ignore = "writes vectors/v1.json"]
fn write_vectors() {
    let pattern = |start: u8| -> [u8; 32] { core::array::from_fn(|i| start.wrapping_add(i as u8)) };

    let contexts = [
        ContextFields::new(Purpose::UserKeyByMaster, 0x11, 0x00, 1),
        ContextFields::new(Purpose::ItemContent, 0x00, 0x22, 1),
        ContextFields::new(Purpose::SecureLink, 0xAA, 0xBB, 255),
        ContextFields::new(Purpose::Export, 0x00, 0x00, 1),
    ];
    let context = contexts
        .iter()
        .map(|fields| ContextVector {
            fields: fields.clone(),
            bytes: hex::encode(fields.context().to_bytes()),
        })
        .collect();

    let aead_cases: [(u8, &ContextFields, &[u8]); 3] = [
        (0x00, &contexts[1], b""),
        (
            0x40,
            &contexts[1],
            b"{\"title\":\"Example\",\"password\":\"hunter2\"}",
        ),
        (0x80, &contexts[2], &[0xFF; 300]),
    ];
    let aead = aead_cases
        .into_iter()
        .map(|(start, fields, plaintext)| {
            let key_bytes = pattern(start);
            let blob = aead::encrypt(
                &SecretKey::from_bytes(key_bytes),
                &fields.context(),
                plaintext,
            )
            .unwrap();
            AeadVector {
                key: hex::encode(key_bytes),
                context: fields.clone(),
                plaintext: hex::encode(plaintext),
                blob: hex::encode(blob),
            }
        })
        .collect();

    let kdf_cases = [
        ("correct horse battery staple", 0x01, KdfParams::MIN),
        ("pässwörd ✓ 密码", 0x02, KdfParams::DEFAULT),
    ];
    let kdf = kdf_cases
        .into_iter()
        .map(|(password, salt_byte, params)| {
            let salt = [salt_byte; kdf::SALT_LEN];
            let derived = kdf::derive_master_key(password.as_bytes(), &salt, &params).unwrap();
            KdfVector {
                password: password.to_owned(),
                salt: hex::encode(salt),
                memory_kib: params.memory_kib,
                iterations: params.iterations,
                parallelism: params.parallelism,
                key: hex::encode(derived.expose()),
            }
        })
        .collect();

    let wrap = vec![{
        let wrapping = pattern(0x10);
        let wrapped_key = pattern(0xC0);
        let blob = wrap::wrap_key(
            &SecretKey::from_bytes(wrapping),
            &contexts[0].context(),
            &SecretKey::from_bytes(wrapped_key),
        )
        .unwrap();
        WrapVector {
            wrapping_key: hex::encode(wrapping),
            context: contexts[0].clone(),
            key: hex::encode(wrapped_key),
            blob: hex::encode(blob),
        }
    }];

    let seal = vec![{
        let fields = ContextFields::new(Purpose::VaultKey, 0x33, 0x44, 1);
        let (private, public) = seal::generate_keypair().unwrap();
        let plaintext = pattern(0x60);
        let blob = seal::seal(&public, &fields.context(), &plaintext).unwrap();
        SealVector {
            private_key: hex::encode(private.expose()),
            public_key: hex::encode(public.as_bytes()),
            context: fields,
            plaintext: hex::encode(plaintext),
            blob: hex::encode(blob),
        }
    }];

    let recovery = [
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
        "legal winner thank year wave sausage worth useful legal winner thank yellow",
        "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong",
    ]
    .into_iter()
    .map(|phrase| RecoveryVector {
        phrase: phrase.to_owned(),
        key: hex::encode(RecoveryPhrase::parse(phrase).unwrap().recovery_key().unwrap().expose()),
    })
    .collect();

    let vectors = Vectors {
        context,
        aead,
        kdf,
        wrap,
        seal,
        recovery,
    };
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/vectors/v1.json");
    let json = serde_json::to_string_pretty(&vectors).unwrap() + "\n";
    std::fs::write(path, json).unwrap();
}
