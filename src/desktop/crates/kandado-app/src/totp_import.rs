//! Reading a two-factor setup key (D-20): from what a person pasted, or from
//! a QR code that is visible on screen. Parsing the key itself is
//! `kandado_model::totp`; this module finds the QR code.

use kandado_model::{Secret, totp::Totp};

use crate::platform::{self, Screenshot};

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum TotpImportError {
    #[error("that is not a setup key")]
    NotASetupKey,
    #[error("no QR code with a setup key is on screen")]
    NoQrCode,
    #[error("the screen could not be read")]
    Capture,
}

/// A setup key ready to save on a login, with what it says about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupKey {
    /// The `otpauth://` URI, as the login stores it.
    pub uri: Secret,
    pub issuer: String,
    pub account: String,
}

/// Reads a pasted setup key: an `otpauth://` URI or a bare base32 key, with
/// or without spaces.
pub fn parse(input: &str) -> Result<SetupKey, TotpImportError> {
    let totp = Totp::parse(input.trim()).map_err(|_| TotpImportError::NotASetupKey)?;
    Ok(SetupKey {
        uri: Secret::new(totp.to_uri().as_str()),
        issuer: totp.issuer.clone(),
        account: totp.account.clone(),
    })
}

/// Every QR code in the picture, decoded to text.
fn decode_qr_codes(screenshot: &Screenshot) -> Vec<String> {
    let mut image =
        rqrr::PreparedImage::prepare_from_greyscale(screenshot.width, screenshot.height, |x, y| {
            screenshot.luma[y * screenshot.width + x]
        });
    image
        .detect_grids()
        .into_iter()
        .filter_map(|grid| grid.decode().ok().map(|(_, text)| text))
        .collect()
}

/// Finds a setup key among the QR codes in a picture.
pub fn find_in(screenshot: &Screenshot) -> Result<SetupKey, TotpImportError> {
    decode_qr_codes(screenshot)
        .iter()
        // Only real setup codes count, not any QR code that happens to be base32.
        .filter(|text| text.starts_with("otpauth://"))
        .find_map(|text| parse(text).ok())
        .ok_or(TotpImportError::NoQrCode)
}

/// Looks at everything on screen for a setup QR code. The capture is dropped
/// before this returns.
pub fn scan_screen() -> Result<SetupKey, TotpImportError> {
    let screenshot = platform::capture_screen().map_err(|_| TotpImportError::Capture)?;
    find_in(&screenshot)
}

/// [`scan_screen`] off the async threads, for the views to await.
pub async fn scan_screen_in_background() -> Result<SetupKey, TotpImportError> {
    tokio::task::spawn_blocking(scan_screen)
        .await
        .unwrap_or(Err(TotpImportError::Capture))
}

#[cfg(test)]
mod tests {
    use super::*;

    const URI: &str = "otpauth://totp/GitHub:juan-dc?secret=JBSWY3DPEHPK3PXP&issuer=GitHub";

    /// Draws `text` as a QR code somewhere on a white "screen".
    fn screen_with(text: &str) -> Screenshot {
        let code = qrcode::QrCode::new(text.as_bytes()).unwrap();
        let modules = code.width();
        let colors = code.to_colors();
        let (scale, quiet, left, top) = (4, 16, 300, 120);
        let (width, height) = (1280, 720);
        let mut luma = vec![255u8; width * height];
        // A grey window behind the code, like a real page.
        for y in 60..600 {
            for x in 200..900 {
                luma[y * width + x] = 236;
            }
        }
        let side = modules * scale + quiet * 2;
        for y in 0..side {
            for x in 0..side {
                let dark = x >= quiet
                    && y >= quiet
                    && (x - quiet) / scale < modules
                    && (y - quiet) / scale < modules
                    && colors[(y - quiet) / scale * modules + (x - quiet) / scale]
                        == qrcode::Color::Dark;
                luma[(top + y) * width + left + x] = if dark { 20 } else { 255 };
            }
        }
        Screenshot {
            width,
            height,
            luma,
        }
    }

    #[test]
    fn a_pasted_key_or_uri_is_read() {
        let key = parse("  JBSW Y3DP EHPK 3PXP ").unwrap();
        assert!(key.uri.expose().starts_with("otpauth://totp"));
        assert!(key.uri.expose().contains("JBSWY3DPEHPK3PXP"));

        let key = parse(URI).unwrap();
        assert_eq!(
            (key.issuer.as_str(), key.account.as_str()),
            ("GitHub", "juan-dc")
        );

        assert_eq!(
            parse("not a key!").unwrap_err(),
            TotpImportError::NotASetupKey
        );
        assert_eq!(parse("").unwrap_err(), TotpImportError::NotASetupKey);
    }

    #[test]
    fn a_qr_code_on_screen_is_found_and_read() {
        let key = find_in(&screen_with(URI)).unwrap();
        assert_eq!(key.issuer, "GitHub");
        assert_eq!(key, parse(URI).unwrap());
    }

    #[test]
    fn other_qr_codes_and_empty_screens_are_not_setup_keys() {
        assert_eq!(
            find_in(&screen_with("https://example.test/menu")).unwrap_err(),
            TotpImportError::NoQrCode
        );
        // A QR code that is only a base32 string is not taken for a key.
        assert_eq!(
            find_in(&screen_with("JBSWY3DPEHPK3PXP")).unwrap_err(),
            TotpImportError::NoQrCode
        );
        let blank = Screenshot {
            width: 640,
            height: 480,
            luma: vec![255; 640 * 480],
        };
        assert_eq!(find_in(&blank).unwrap_err(), TotpImportError::NoQrCode);
    }

    #[test]
    fn the_setup_key_is_not_printed() {
        let debug = format!("{:?}", parse(URI).unwrap());
        assert!(!debug.contains("JBSWY3DPEHPK3PXP"), "{debug}");
    }
}
