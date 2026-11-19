//! Makes and signs the update manifest the Kandado desktop app trusts (D-54).
//!
//! ```text
//! update-signer keygen <folder>
//!     Writes update-signing.key (the secret; keep it safe, it signs updates)
//!     and update-signing.pub, and prints the public key for the app's
//!     src/desktop/crates/kandado-app/src/update_key.pub.
//!
//! update-signer manifest <version> <installer> <download-base-url> <notes-url>
//!     Prints latest.json: the version, and the installer's address, SHA-256 and size.
//!
//! update-signer sign <file>
//!     Writes <file>.minisig. The secret key comes from UPDATE_SIGNING_KEY
//!     (the whole key file's text), never from the command line.
//! ```
//!
//! The release workflow runs `manifest` and `sign` and attaches latest.json and
//! latest.json.minisig to the GitHub Release. The app verifies the signature
//! with the public key compiled into it, so neither GitHub nor anyone who can
//! change the release can push an update without the secret key.
#![forbid(unsafe_code)]

use std::{
    fs,
    io::{self, Read as _},
    path::{Path, PathBuf},
    process::ExitCode,
};

use minisign::{KeyPair, SecretKeyBox, SignatureBox};
use sha2::{Digest, Sha256};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["keygen", folder] => keygen(Path::new(folder)),
        ["manifest", version, installer, base, notes] => {
            manifest(version, Path::new(installer), base, notes)
        }
        ["sign", file] => sign(Path::new(file)),
        _ => Err(
            "usage: update-signer keygen <folder> | manifest <version> <installer> \
             <download-base-url> <notes-url> | sign <file>"
                .into(),
        ),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("update-signer: {error}");
            ExitCode::FAILURE
        }
    }
}

type Result<T> = std::result::Result<T, String>;

fn keygen(folder: &Path) -> Result<()> {
    let secret = folder.join("update-signing.key");
    let public = folder.join("update-signing.pub");
    if secret.exists() {
        return Err(format!(
            "{} exists: not replacing a signing key",
            secret.display()
        ));
    }
    let pair = KeyPair::generate_unencrypted_keypair().map_err(|e| e.to_string())?;
    let secret_text = pair
        .sk
        .to_box(Some("Kandado update signing key"))
        .map_err(|e| e.to_string())?
        .into_string();
    let public_box = pair.pk.to_box().map_err(|e| e.to_string())?.into_string();
    fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    write_private(&secret, secret_text.as_bytes())?;
    fs::write(&public, public_box).map_err(|e| e.to_string())?;
    println!("{}", pair.pk.to_base64());
    eprintln!(
        "wrote {} (secret: back it up, then add it as UPDATE_SIGNING_KEY) and {}",
        secret.display(),
        public.display()
    );
    Ok(())
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| e.to_string())?;
    io::Write::write_all(&mut file, bytes).map_err(|e| e.to_string())
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    io::Write::write_all(&mut file, bytes).map_err(|e| e.to_string())
}

fn manifest(version: &str, installer: &Path, base: &str, notes: &str) -> Result<()> {
    if !is_version(version) {
        return Err(format!("{version} is not a version like 1.2.3"));
    }
    let bytes = fs::read(installer).map_err(|e| format!("{}: {e}", installer.display()))?;
    let name = installer
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("the installer has no file name")?;
    let sha256: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let manifest = serde_json::json!({
        "version": version,
        "notes_url": notes,
        "windows": {
            "url": format!("{}/{name}", base.trim_end_matches('/')),
            "sha256": sha256,
            "size": bytes.len(),
        },
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn sign(file: &Path) -> Result<()> {
    let text = std::env::var("UPDATE_SIGNING_KEY")
        .map_err(|_| "UPDATE_SIGNING_KEY is not set (the signing key file's text)".to_owned())?;
    let secret = SecretKeyBox::from_string(text.trim())
        .and_then(SecretKeyBox::into_unencrypted_secret_key)
        .map_err(|e| format!("the signing key is not usable: {e}"))?;
    let public = minisign::PublicKey::from_secret_key(&secret).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    fs::File::open(file)
        .and_then(|mut f| f.read_to_end(&mut data))
        .map_err(|e| format!("{}: {e}", file.display()))?;
    let name = file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let signature: SignatureBox = minisign::sign(
        Some(&public),
        &secret,
        io::Cursor::new(&data),
        Some(&format!("kandado update {name}")),
        Some("Kandado update signature"),
    )
    .map_err(|e| e.to_string())?;
    let out = PathBuf::from(format!("{}.minisig", file.display()));
    fs::write(&out, signature.into_string()).map_err(|e| e.to_string())?;
    eprintln!("signed {} with key {}", file.display(), public.to_base64());
    Ok(())
}

/// `1.2.3`: three numbers, nothing else.
fn is_version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}
