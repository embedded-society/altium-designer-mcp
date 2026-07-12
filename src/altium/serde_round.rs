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

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use serde_json::{json, Value};

    // Test structs wiring each helper into serde via the same attributes the
    // real primitives use, so the tests exercise the actual serialise path.
    #[derive(Serialize)]
    struct Bare {
        #[serde(serialize_with = "super::serialize")]
        v: f64,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Opt {
        #[serde(with = "super::option")]
        v: Option<f64>,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct VecTup {
        #[serde(with = "super::vec_tuple")]
        v: Option<Vec<(f64, f64)>>,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct VecF {
        #[serde(with = "super::vec_f64")]
        v: Option<Vec<f64>>,
    }

    #[test]
    fn round_reduces_to_six_decimal_places() {
        assert!((super::round(1.234_567_89) - 1.234_568).abs() < 1e-12);
        assert!((super::round(9.999_999_9) - 10.0).abs() < 1e-12);
        // A value already within 6 dp is unchanged.
        assert!((super::round(-2.5) + 2.5).abs() < 1e-12);
    }

    #[test]
    fn bare_serialiser_rounds() {
        let j = serde_json::to_value(Bare { v: 1.234_567_89 }).unwrap();
        assert_eq!(j["v"], json!(1.234_568));
    }

    #[test]
    fn option_serialises_some_rounded_and_none_null() {
        let some = serde_json::to_value(Opt {
            v: Some(0.123_456_789),
        })
        .unwrap();
        assert_eq!(some["v"], json!(0.123_457));
        let none = serde_json::to_value(Opt { v: None }).unwrap();
        assert_eq!(none["v"], Value::Null);
    }

    #[test]
    fn option_round_trips_both_variants() {
        for v in [Some(1.5_f64), None] {
            let s = serde_json::to_string(&Opt { v }).unwrap();
            let back: Opt = serde_json::from_str(&s).unwrap();
            assert_eq!(back.v, v);
        }
    }

    #[test]
    fn vec_tuple_serialises_each_component_rounded_and_none_null() {
        let some = serde_json::to_value(VecTup {
            v: Some(vec![(1.111_111_9, 2.222_222_1)]),
        })
        .unwrap();
        assert_eq!(some["v"], json!([[1.111_112, 2.222_222]]));
        let none = serde_json::to_value(VecTup { v: None }).unwrap();
        assert_eq!(none["v"], Value::Null);
    }

    #[test]
    fn vec_tuple_round_trips() {
        let orig = VecTup {
            v: Some(vec![(1.0, 2.0), (3.0, 4.0)]),
        };
        let back: VecTup = serde_json::from_str(&serde_json::to_string(&orig).unwrap()).unwrap();
        assert_eq!(back, orig);
    }

    #[test]
    fn vec_f64_serialises_rounded_and_none_null() {
        let some = serde_json::to_value(VecF {
            v: Some(vec![9.999_999_9, 0.000_000_4]),
        })
        .unwrap();
        assert_eq!(some["v"], json!([10.0, 0.0]));
        let none = serde_json::to_value(VecF { v: None }).unwrap();
        assert_eq!(none["v"], Value::Null);
    }

    #[test]
    fn vec_f64_round_trips() {
        let orig = VecF {
            v: Some(vec![1.0, 2.0, 3.0]),
        };
        let back: VecF = serde_json::from_str(&serde_json::to_string(&orig).unwrap()).unwrap();
        assert_eq!(back, orig);
    }
}
