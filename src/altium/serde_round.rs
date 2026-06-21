//! Shared serde helper that rounds `f64` values to 6 decimal places on
//! serialization.
//!
//! Both `PcbLib` (mm coordinates) and `SchLib` (rotation/angle fields) round
//! float JSON output identically; this is the single implementation they share
//! (it previously lived as duplicate `coord_serde`/`float_serde` modules). The
//! rounding is unchanged, so serialized output is bit-identical.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Rounds a value to 6 decimal places.
#[inline]
fn round(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

/// Serialises an f64 with rounding.
#[allow(clippy::trivially_copy_pass_by_ref)] // serde requires &T signature
pub fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64(round(*value))
}

/// Serialises an optional f64 with rounding.
pub mod option {
    use super::{round, Deserialize, Deserializer, Serializer};

    #[allow(clippy::ref_option)] // serde requires &Option<T> signature
    pub fn serialize<S: Serializer>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(v) => serializer.serialize_some(&round(*v)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<f64>, D::Error> {
        Option::<f64>::deserialize(deserializer)
    }
}

/// Serialises a `Vec` of (f64, f64) tuples with rounding.
pub mod vec_tuple {
    use super::{round, Deserialize, Deserializer, Serialize, Serializer};

    #[allow(clippy::ref_option)] // serde requires &Option<T> signature
    pub fn serialize<S: Serializer>(
        value: &Option<Vec<(f64, f64)>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(v) => {
                let rounded: Vec<(f64, f64)> =
                    v.iter().map(|(a, b)| (round(*a), round(*b))).collect();
                rounded.serialize(serializer)
            }
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Vec<(f64, f64)>>, D::Error> {
        Option::<Vec<(f64, f64)>>::deserialize(deserializer)
    }
}

/// Serialises a `Vec` of f64 with rounding.
pub mod vec_f64 {
    use super::{round, Deserialize, Deserializer, Serialize, Serializer};

    #[allow(clippy::ref_option)] // serde requires &Option<T> signature
    pub fn serialize<S: Serializer>(
        value: &Option<Vec<f64>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(v) => {
                let rounded: Vec<f64> = v.iter().map(|x| round(*x)).collect();
                rounded.serialize(serializer)
            }
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Vec<f64>>, D::Error> {
        Option::<Vec<f64>>::deserialize(deserializer)
    }
}
