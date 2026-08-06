//! Tolerant decoding for Thunderstore `manifest.json` payloads.
//!
//! Mod authors publish manifests straight out of whatever editor they happen to
//! use, so the bytes in the archive are frequently *not* plain UTF-8. The two
//! shapes seen in the wild are a UTF-8 byte-order mark (Visual Studio and
//! Notepad both write one by default) and, more rarely, UTF-16 with a BOM
//! ("Save as Unicode"). Both make `serde_json` fail with the same unhelpful
//! `expected value at line 1 column 1`, which is what users hit as a blanket
//! "Sync Failed" — see issue #24.
//!
//! Everything here is BOM-driven, so a well-formed UTF-8 manifest takes the
//! fast path and is parsed without any copying or re-encoding.

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const UTF16LE_BOM: &[u8] = &[0xFF, 0xFE];
const UTF16BE_BOM: &[u8] = &[0xFE, 0xFF];

/// Decode manifest bytes into a UTF-8 string, stripping a byte-order mark and
/// transcoding UTF-16 if one is present.
///
/// Returns `None` when the bytes are not valid text in any supported encoding.
pub fn decode_manifest_text(bytes: &[u8]) -> Option<String> {
    if let Some(rest) = bytes.strip_prefix(UTF8_BOM) {
        return String::from_utf8(rest.to_vec()).ok();
    }

    // A UTF-16 BOM is checked before plain UTF-8 because `FF FE` / `FE FF` are
    // never valid UTF-8 lead bytes, so there is no ambiguity to resolve.
    if let Some(rest) = bytes.strip_prefix(UTF16LE_BOM) {
        return decode_utf16(rest, u16::from_le_bytes);
    }
    if let Some(rest) = bytes.strip_prefix(UTF16BE_BOM) {
        return decode_utf16(rest, u16::from_be_bytes);
    }

    String::from_utf8(bytes.to_vec()).ok()
}

fn decode_utf16(bytes: &[u8], to_unit: fn([u8; 2]) -> u16) -> Option<String> {
    if bytes.len() % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| to_unit([pair[0], pair[1]]))
        .collect();
    String::from_utf16(&units).ok()
}

/// Parse manifest bytes into JSON, tolerating the encodings described above.
///
/// `context` is folded into the error message so callers can name the mod that
/// failed instead of surfacing a bare parser error.
pub fn parse_manifest_bytes(bytes: &[u8], context: &str) -> Result<serde_json::Value, String> {
    let text = decode_manifest_text(bytes).ok_or_else(|| {
        format!(
            "Invalid manifest.json in {}: file is not valid UTF-8 or UTF-16 text",
            context
        )
    })?;
    serde_json::from_str(&text)
        .map_err(|error| format!("Invalid manifest.json in {}: {}", context, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{"name":"ReturnsAPI","version_number":"0.1.58"}"#;

    fn assert_parses(bytes: &[u8]) {
        let value = parse_manifest_bytes(bytes, "test mod").expect("manifest should parse");
        assert_eq!(value["name"].as_str(), Some("ReturnsAPI"));
        assert_eq!(value["version_number"].as_str(), Some("0.1.58"));
    }

    #[test]
    fn parses_plain_utf8() {
        assert_parses(MANIFEST.as_bytes());
    }

    #[test]
    fn parses_utf8_with_bom() {
        // The exact byte prefix shipped by ReturnsAPI 0.1.58, which is the
        // archive that triggered the "expected value at line 1 column 1" report.
        let mut bytes = UTF8_BOM.to_vec();
        bytes.extend_from_slice(MANIFEST.as_bytes());
        assert_parses(&bytes);
    }

    #[test]
    fn parses_utf16le_with_bom() {
        let mut bytes = UTF16LE_BOM.to_vec();
        for unit in MANIFEST.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        assert_parses(&bytes);
    }

    #[test]
    fn parses_utf16be_with_bom() {
        let mut bytes = UTF16BE_BOM.to_vec();
        for unit in MANIFEST.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        assert_parses(&bytes);
    }

    #[test]
    fn parses_manifest_with_leading_whitespace() {
        let padded = format!("\n\t {}", MANIFEST);
        assert_parses(padded.as_bytes());
    }

    #[test]
    fn rejects_non_text_payload() {
        let error = parse_manifest_bytes(&[0x00, 0xFF, 0xFE, 0x80], "test mod").unwrap_err();
        assert!(
            error.contains("test mod"),
            "error should name the mod: {error}"
        );
    }

    #[test]
    fn rejects_malformed_json_with_context() {
        let error = parse_manifest_bytes(b"{\"name\": ", "test mod").unwrap_err();
        assert!(error.contains("Invalid manifest.json in test mod"));
    }

    #[test]
    fn utf16_without_bom_is_not_misread_as_utf8() {
        // UTF-16 with no BOM is indistinguishable from binary; it must fail
        // rather than silently decode into mojibake.
        let mut bytes = Vec::new();
        for unit in MANIFEST.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        assert!(parse_manifest_bytes(&bytes, "test mod").is_err());
    }
}
