//! Serde helper that (de)serialises an `Option<Vec<u8>>` as a standard base64
//! string.
//!
//! Raw binary payloads (embedded image bytes in the `SchLib` `/Storage`
//! stream) cannot travel through JSON as-is, so the tool surface carries them
//! base64-encoded. Used via `#[serde(with = "crate::altium::base64_opt")]`
//! alongside `default` + `skip_serializing_if = "Option::is_none"` so an
//! absent payload is omitted entirely.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::{Deserialize, Deserializer, Serializer};

/// Serialises `Some(bytes)` as a base64 string; `None` as JSON null (callers
/// pair this with `skip_serializing_if` so `None` is omitted instead).
#[allow(clippy::ref_option)] // serde requires &Option<T> signature
pub fn serialize<S: Serializer>(value: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        Some(bytes) => serializer.serialize_some(&STANDARD.encode(bytes)),
        None => serializer.serialize_none(),
    }
}

/// Deserialises an optional base64 string back into bytes. Invalid base64 is a
/// deserialisation error (the strict struct path); lenient JSON entry points
/// decode by hand and downgrade the error instead.
pub fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<u8>>, D::Error> {
    Option::<String>::deserialize(deserializer)?.map_or_else(
        || Ok(None),
        |s| {
            STANDARD
                .decode(s.as_bytes())
                .map(Some)
                .map_err(serde::de::Error::custom)
        },
    )
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug, Default)]
    struct Holder {
        #[serde(
            default,
            with = "crate::altium::base64_opt",
            skip_serializing_if = "Option::is_none"
        )]
        data: Option<Vec<u8>>,
    }

    #[test]
    fn round_trips_bytes_as_base64() {
        let holder = Holder {
            data: Some(vec![0x42, 0x4D, 0x00, 0xFF]),
        };
        let json = serde_json::to_string(&holder).expect("serialise");
        assert_eq!(json, r#"{"data":"Qk0A/w=="}"#, "standard base64 encoding");
        let back: Holder = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back, holder);
    }

    #[test]
    fn none_is_omitted_and_absent_reads_none() {
        let json = serde_json::to_string(&Holder { data: None }).expect("serialise");
        assert_eq!(json, "{}", "None is omitted entirely");
        let back: Holder = serde_json::from_str("{}").expect("deserialise");
        assert_eq!(back.data, None);
    }

    #[test]
    fn invalid_base64_is_a_deserialise_error() {
        let err = serde_json::from_str::<Holder>(r#"{"data":"@@not-base64@@"}"#)
            .expect_err("invalid base64 must fail strict deserialisation");
        assert!(
            err.to_string().contains("Invalid"),
            "error mentions the decode failure: {err}"
        );
    }
}
