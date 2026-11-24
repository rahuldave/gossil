//! Behavior tests of the strict local JSON codec and its cumulative work ledger.
use super::*;
use serde::Deserialize;
use serde_json::json;
use std::error::Error as _;

/// Generous ceilings for tests that do not probe a limit.
fn wide() -> CodecLimits {
    CodecLimits {
        bytes: 1_048_576,
        nodes: 65_536,
        depth: 64,
    }
}

/// Start a ledger with the given ceilings.
fn work(bytes: usize, nodes: usize, depth: usize) -> CodecWork {
    CodecWork::new(CodecLimits {
        bytes,
        nodes,
        depth,
    })
    .unwrap()
}

#[test]
fn new_ledger_refuses_zero_and_unsupported_ceilings() {
    for limits in [
        CodecLimits { bytes: 0, ..wide() },
        CodecLimits { nodes: 0, ..wide() },
        CodecLimits { depth: 0, ..wide() },
        CodecLimits {
            depth: 128,
            ..wide()
        },
    ] {
        assert_eq!(
            CodecWork::new(limits).err(),
            Some(error("invalid aggregate codec limits"))
        );
    }
    let mut deepest = CodecWork::new(CodecLimits {
        depth: 127,
        ..wide()
    })
    .unwrap();
    assert_eq!(deepest.usage().depth_limit, 127);
    // The deepest ledger decodes 127 nested arrays. At 128 the ledger, and not
    // the recursion limit of `serde_json`, refuses first.
    let nested = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
    deepest.document(nested(127).as_bytes()).unwrap();
    let refused = deepest.document(nested(128).as_bytes()).unwrap_err();
    assert!(
        refused.to_string().contains("local JSON node/depth limit"),
        "{refused}"
    );
    let value: Value = serde_json::from_str(&nested(127)).unwrap();
    deepest.admit_serialized(&value).unwrap();
    assert_eq!(
        deepest.encode_record(&value).unwrap(),
        nested(127).into_bytes()
    );
}

#[test]
fn documents_share_one_ledger_and_exhaustion_is_cumulative() {
    let mut work = work(64, 12, 4);
    assert_eq!(
        work.usage(),
        CodecUsage {
            bytes: 0,
            nodes: 0,
            peak_depth: 0,
            depth_limit: 4
        }
    );
    // The object, its key, the array and the number are four nodes.
    assert_eq!(work.document(br#"{"a":[1]}"#).unwrap(), json!({"a":[1]}));
    assert_eq!(
        work.usage(),
        CodecUsage {
            bytes: 9,
            nodes: 4,
            peak_depth: 3,
            depth_limit: 4
        }
    );
    work.document(b"[2]").unwrap();
    work.document(b"[3]").unwrap();
    assert_eq!((work.usage().bytes, work.usage().nodes), (15, 8));
    // Five more nodes do not fit in the four that remain, even though the
    // document alone fits the original ceilings.
    let refused = work.document(br#"{"b":[4,5]}"#).unwrap_err();
    assert!(refused.to_string().contains("local JSON node/depth limit"));
    assert_eq!(work.remaining_nodes(), 0);
    assert!(work.document(b"null").is_err());
}

#[test]
fn byte_ceiling_admits_the_exact_size_and_refuses_one_more() {
    let mut exact = work(3, 10, 4);
    assert_eq!(exact.document(b"[1]").unwrap(), json!([1]));
    assert_eq!(exact.remaining_bytes(), 0);

    let mut short = work(2, 10, 4);
    assert_eq!(
        short.document(b"[1]").unwrap_err(),
        error("aggregate codec byte limit")
    );
    // A refused byte charge consumes neither bytes nor nodes.
    assert_eq!(short.usage().bytes, 0);
    assert_eq!(short.usage().nodes, 0);
    short.charge(2).unwrap();
    assert_eq!(
        short.charge(1).unwrap_err(),
        error("aggregate codec byte limit")
    );
    assert_eq!(short.remaining_bytes(), 0);
}

#[test]
fn depth_ceiling_admits_the_exact_depth_and_refuses_one_more() {
    let mut exact = work(64, 64, 3);
    assert_eq!(exact.document(b"[[1]]").unwrap(), json!([[1]]));
    assert_eq!(exact.usage().peak_depth, 3);

    let mut deeper = work(64, 64, 3);
    let refused = deeper.document(b"[[[1]]]").unwrap_err();
    assert!(refused.to_string().contains("local JSON node/depth limit"));
    // The three values that fit were charged before the refusal.
    assert_eq!(deeper.usage().nodes, 3);
}

#[test]
fn node_ceiling_charges_every_key_and_value() {
    // An object with two keys and two values is five nodes.
    let mut exact = work(64, 5, 4);
    exact.document(br#"{"a":1,"b":2}"#).unwrap();
    assert_eq!(exact.remaining_nodes(), 0);

    // The second key, not its value, finds the ledger empty.
    let mut short = work(64, 3, 4);
    let refused = short.document(br#"{"a":1,"b":2}"#).unwrap_err();
    assert!(refused.to_string().contains("local JSON node limit"));
    assert_eq!(short.remaining_nodes(), 0);
}

#[test]
fn duplicate_keys_are_refused_at_every_depth() {
    for document in [
        &br#"{"a":1,"a":1}"#[..],
        &br#"{"a":1,"a":2}"#[..],
        &br#"{"outer":{"k":[],"k":null}}"#[..],
        &br#"[{"k":true},{"k":true,"k":false}]"#[..],
    ] {
        let mut work = CodecWork::new(wide()).unwrap();
        let refused = work.document(document).unwrap_err();
        assert!(
            refused.to_string().contains("duplicate local JSON key"),
            "{refused}"
        );
        assert!(refused.source().unwrap().is::<serde_json::Error>());
    }
    // The same key in two sibling objects is not a duplicate.
    let mut work = CodecWork::new(wide()).unwrap();
    assert_eq!(
        work.document(br#"[{"k":1},{"k":2}]"#).unwrap(),
        json!([{"k":1},{"k":2}])
    );
}

#[test]
fn malformed_and_trailing_input_keep_the_parser_cause() {
    let expected = serde_json::from_slice::<Value>(b"{").unwrap_err();
    let mut work = CodecWork::new(wide()).unwrap();
    let failure = work.document(b"{").unwrap_err();
    let source = failure
        .source()
        .unwrap()
        .downcast_ref::<serde_json::Error>()
        .unwrap();
    assert_eq!(source.classify(), expected.classify());
    assert_eq!(
        (source.line(), source.column()),
        (expected.line(), expected.column())
    );
    assert_eq!(
        failure.to_string(),
        format!("invalid aggregate local JSON: {expected}")
    );
    // A clone shares the cause, and equality compares only the diagnostic.
    let cloned = failure.clone();
    assert!(std::ptr::eq(
        failure.source().unwrap(),
        cloned.source().unwrap()
    ));
    assert_eq!(cloned, failure);
    assert_eq!(failure, error(&failure.to_string()));
    assert_eq!(
        format!("{failure:?}"),
        format!("CommandError({:?})", failure.to_string())
    );
    assert!(error("plain refusal").source().is_none());

    let trailing = work.document(b"{} false").unwrap_err();
    assert!(trailing.source().unwrap().is::<serde_json::Error>());
    assert!(trailing.to_string().starts_with("trailing local JSON: "));
}

#[test]
fn decoded_arrays_grow_in_amortized_steps() {
    // Growing by one element per push copies the array on every element. The
    // decoder grows by doubling, so the capacity of a long array runs ahead of it.
    let document = format!("[{}]", vec!["0"; 1000].join(","));
    let mut work = CodecWork::new(wide()).unwrap();
    let value = work.document(document.as_bytes()).unwrap();
    let array = value.as_array().unwrap();
    assert_eq!(array.len(), 1000);
    assert!(array.capacity() > array.len());
}

#[test]
fn json_values_keep_their_types() {
    let mut work = CodecWork::new(wide()).unwrap();
    assert_eq!(
        work.document(r#"[true,null,-7,18446744073709551615,1.5,"\u00e9x"]"#.as_bytes())
            .unwrap(),
        json!([true, null, -7, 18_446_744_073_709_551_615_u64, 1.5, "éx"])
    );
}

#[test]
fn the_visitor_refuses_forms_that_are_not_json_values() {
    let mut budget = JsonBudget {
        remaining: 4,
        max_depth: 4,
        peak_depth: 0,
    };
    let bytes = Seed {
        budget: &mut budget,
        depth: 1,
    }
    .deserialize(serde::de::value::BorrowedBytesDeserializer::<
        serde_json::Error,
    >::new(b"raw"))
    .unwrap_err();
    assert!(
        bytes
            .to_string()
            .contains("bounded local JSON with unique object keys")
    );
    let nonfinite = Seed {
        budget: &mut budget,
        depth: 1,
    }
    .deserialize(serde::de::value::F64Deserializer::<serde_json::Error>::new(
        f64::NAN,
    ))
    .unwrap_err();
    assert_eq!(nonfinite.to_string(), "nonfinite JSON number");
}

#[test]
fn saved_ceilings_only_tighten_and_never_replenish_work() {
    let initial = CodecLimits {
        bytes: 128,
        nodes: 32,
        depth: 8,
    };
    let mut work = CodecWork::new(initial).unwrap();
    work.document(br#"{"a":[1]}"#).unwrap();
    work.constrain(CodecLimits {
        bytes: 20,
        nodes: 8,
        depth: 4,
    })
    .unwrap();
    assert_eq!(
        work.usage(),
        CodecUsage {
            bytes: 9,
            nodes: 4,
            peak_depth: 3,
            depth_limit: 4
        }
    );
    assert_eq!((work.remaining_bytes(), work.remaining_nodes()), (11, 4));
    // Larger saved ceilings change nothing.
    work.constrain(CodecLimits {
        bytes: 1000,
        nodes: 1000,
        depth: 64,
    })
    .unwrap();
    assert_eq!((work.remaining_bytes(), work.remaining_nodes()), (11, 4));
    assert_eq!(work.usage().depth_limit, 4);
    work.constrain(CodecLimits {
        bytes: 15,
        nodes: 6,
        depth: 3,
    })
    .unwrap();
    assert_eq!((work.remaining_bytes(), work.remaining_nodes()), (6, 2));
    work.document(b"[2]").unwrap();
    let used = work.usage();
    work.constrain(initial).unwrap();
    assert_eq!(work.usage(), used);
    assert_eq!((work.remaining_bytes(), work.remaining_nodes()), (3, 0));
    assert!(work.document(b"0").is_err());
}

#[test]
fn saved_ceilings_below_consumed_work_are_refused_without_change() {
    let mut work = CodecWork::new(wide()).unwrap();
    work.document(br#"{"a":[1]}"#).unwrap();
    let before = (work.usage(), work.remaining_bytes(), work.remaining_nodes());
    for (limits, message) in [
        (
            CodecLimits { bytes: 8, ..wide() },
            "saved codec limits below consumed work",
        ),
        (
            CodecLimits { nodes: 3, ..wide() },
            "saved codec limits below consumed work",
        ),
        (
            CodecLimits { depth: 2, ..wide() },
            "saved codec limits below consumed work",
        ),
        (
            CodecLimits { nodes: 0, ..wide() },
            "invalid saved codec limits",
        ),
        (
            CodecLimits {
                depth: 128,
                ..wide()
            },
            "invalid saved codec limits",
        ),
    ] {
        assert_eq!(work.constrain(limits).unwrap_err(), error(message));
        assert_eq!(
            (work.usage(), work.remaining_bytes(), work.remaining_nodes()),
            before
        );
    }
    // Saved ceilings equal to the consumed work leave nothing to spend.
    work.constrain(CodecLimits {
        bytes: 9,
        nodes: 4,
        depth: 3,
    })
    .unwrap();
    assert_eq!((work.remaining_bytes(), work.remaining_nodes()), (0, 0));
    assert!(work.document(b"0").is_err());
}

#[test]
fn encode_record_charges_its_output_and_refuses_oversized_output_without_charge() {
    let mut work = work(128, 32, 8);
    work.document(b"[1]").unwrap();
    work.constrain(CodecLimits {
        bytes: 6,
        nodes: 4,
        depth: 2,
    })
    .unwrap();
    assert_eq!(work.encode_record(&json!([2])).unwrap(), b"[2]");
    assert_eq!(
        work.usage(),
        CodecUsage {
            bytes: 6,
            nodes: 4,
            peak_depth: 2,
            depth_limit: 2
        }
    );
    let before = work.usage();
    let refused = work.encode_record(&json!(0)).unwrap_err();
    assert!(refused.to_string().starts_with("bounded local encode: "));
    assert!(refused.source().is_some());
    assert_eq!(work.usage(), before);
}

#[test]
fn encode_record_output_that_fits_the_bytes_still_pays_its_nodes() {
    let mut work = work(128, 2, 8);
    let refused = work.encode_record(&json!([1, 2])).unwrap_err();
    assert!(refused.to_string().contains("local JSON node/depth limit"));
    // The five output bytes and the two nodes that fit stay spent.
    assert_eq!((work.usage().bytes, work.usage().nodes), (5, 2));
}

#[test]
fn bounded_writer_keeps_its_bytes_after_a_refused_write() {
    use std::io::Write;
    let mut writer = BoundedWriter {
        bytes: vec![],
        limit: 3,
    };
    writer.write_all(b"abc").unwrap();
    writer.flush().unwrap();
    assert_eq!(
        writer.write_all(b"d").unwrap_err().to_string(),
        "local JSON output limit"
    );
    writer.flush().unwrap();
    assert_eq!(writer.bytes, b"abc");
    // Many small writes grow the buffer in amortized steps, not one write at a time.
    let mut writer = BoundedWriter {
        bytes: vec![],
        limit: 1000,
    };
    for _ in 0..1000 {
        writer.write_all(b"x").unwrap();
    }
    assert_eq!(writer.bytes.len(), 1000);
    assert!(writer.bytes.capacity() > 1000);
}

/// A closed record with one optional and one nested closed field.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Closed {
    /// A required integer.
    version: u8,
    /// An optional text, which the record must still list as null.
    note: Option<String>,
    /// A nested closed object.
    inner: Inner,
}
/// The nested closed object of [`Closed`].
#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Inner {
    /// A required flag.
    flag: bool,
}
/// The field names of [`Closed`].
const CLOSED: [&str; 3] = ["version", "note", "inner"];

#[test]
fn closed_records_accept_exactly_the_listed_fields() {
    let mut work = CodecWork::new(wide()).unwrap();
    let record: Closed = work
        .closed_record(
            br#"{"version":1,"note":null,"inner":{"flag":true}}"#,
            &CLOSED,
        )
        .unwrap();
    assert_eq!(
        record,
        Closed {
            version: 1,
            note: None,
            inner: Inner { flag: true }
        }
    );
    // The outer object, three keys and three values, then the inner key and value.
    assert_eq!(work.usage().nodes, 9);
}

#[test]
fn closed_records_refuse_unknown_missing_and_mistyped_fields() {
    for (document, message) in [
        (
            &br#"{"version":1,"note":null,"inner":{"flag":true},"extra":0}"#[..],
            r#"unknown closed local record field "extra""#,
        ),
        (
            &br#"{"version":1,"inner":{"flag":true}}"#[..],
            r#"missing closed local record field "note""#,
        ),
        (&br#"[1]"#[..], "closed local record is not an object"),
    ] {
        let mut work = CodecWork::new(wide()).unwrap();
        assert_eq!(
            work.closed_record::<Closed>(document, &CLOSED).unwrap_err(),
            error(message)
        );
    }
    for document in [
        &br#"{"version":1,"note":null,"inner":{"flag":true,"extra":0}}"#[..],
        &br#"{"version":"1","note":null,"inner":{"flag":true}}"#[..],
        &br#"{"version":256,"note":null,"inner":{"flag":true}}"#[..],
    ] {
        let mut work = CodecWork::new(wide()).unwrap();
        let refused = work.closed_record::<Closed>(document, &CLOSED).unwrap_err();
        assert!(
            refused
                .to_string()
                .starts_with("invalid closed local record: ")
        );
        assert!(refused.source().unwrap().is::<serde_json::Error>());
    }
    // A duplicate key is refused before the field check can see the object.
    let mut work = CodecWork::new(wide()).unwrap();
    let refused = work
        .closed_record::<Closed>(
            br#"{"version":1,"version":1,"note":null,"inner":{"flag":true}}"#,
            &CLOSED,
        )
        .unwrap_err();
    assert!(refused.to_string().contains("duplicate local JSON key"));
}

#[test]
fn closed_records_spend_the_shared_ledger() {
    let document = br#"{"version":1,"note":null,"inner":{"flag":true}}"#;
    let mut work = work(document.len() * 2 - 1, 1000, 8);
    work.closed_record::<Closed>(document, &CLOSED).unwrap();
    // The second copy needs one byte more than remains.
    assert_eq!(
        work.closed_record::<Closed>(document, &CLOSED).unwrap_err(),
        error("aggregate codec byte limit")
    );
}

/// A unit struct, serialized as null.
#[derive(Serialize)]
struct Unit;
/// A tuple struct, serialized as an array.
#[derive(Serialize)]
struct Pair(i16, char);
/// A newtype struct, serialized as its inner value.
#[derive(Serialize)]
struct Wrapper(u32);
/// Raw bytes, serialized as an array of numbers.
struct Bytes;
impl Serialize for Bytes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&[0, 127, 255])
    }
}
/// A value that serializes through `collect_str`.
struct Display;
impl Serialize for Display {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&format_args!("{}-{}", 1, 2))
    }
}
/// Every kind of enum variant.
#[derive(Serialize)]
enum Variant {
    /// Serialized as a string.
    Unit,
    /// Serialized as an object of one key.
    Newtype(u64),
    /// Serialized as an object of one key whose value is an array.
    Tuple(i64, bool),
    /// Serialized as an object of one key whose value is an object.
    Struct {
        /// An unsigned field.
        a: u8,
    },
}
/// A record that uses every serializer method of the preflight.
#[derive(Serialize)]
struct Forms {
    /// Signed integers of each width.
    small: (i8, i16, i32, i64, i128),
    /// Unsigned integers of each width.
    wide: (u8, u16, u32, u64, u128),
    /// Floating-point numbers of each width.
    fraction: (f32, f64),
    /// A character.
    letter: char,
    /// A boolean.
    flag: bool,
    /// Raw bytes.
    bytes: Bytes,
    /// A unit struct.
    unit: Unit,
    /// The unit value.
    empty: (),
    /// A tuple struct.
    pair: Pair,
    /// A newtype struct.
    wrapper: Wrapper,
    /// An absent and a present option.
    options: (Option<u8>, Option<&'static str>),
    /// A display value.
    display: Display,
    /// Each variant kind.
    variants: Vec<Variant>,
    /// A map with string keys.
    map: std::collections::BTreeMap<&'static str, Vec<u8>>,
    /// A map with integer keys.
    numbered: std::collections::BTreeMap<u8, &'static str>,
    /// A map with boolean keys, which `serde_json` writes as strings.
    flags: std::collections::BTreeMap<bool, u8>,
}
/// A record that exercises every serializer method.
fn forms() -> Forms {
    Forms {
        small: (-1, -2, -3, -4, -5),
        wide: (1, 2, 3, 4, 5),
        fraction: (1.5, -0.25),
        letter: 'é',
        flag: true,
        bytes: Bytes,
        unit: Unit,
        empty: (),
        pair: Pair(-7, 'x'),
        wrapper: Wrapper(9),
        options: (None, Some("s")),
        display: Display,
        variants: vec![
            Variant::Unit,
            Variant::Newtype(3),
            Variant::Tuple(-1, false),
            Variant::Struct { a: 2 },
        ],
        map: [("k", vec![1, 2])].into(),
        numbered: [(1, "one")].into(),
        flags: [(true, 1)].into(),
    }
}

#[test]
fn shape_admission_charges_what_a_strict_decode_of_the_output_charges() {
    let mut shaped = CodecWork::new(wide()).unwrap();
    shaped.admit_serialized(&forms()).unwrap();
    let bytes = serde_json::to_vec(&forms()).unwrap();
    let mut decoded = CodecWork::new(wide()).unwrap();
    decoded.document(&bytes).unwrap();
    assert_eq!(shaped.usage().nodes, decoded.usage().nodes);
    assert_eq!(shaped.usage().peak_depth, decoded.usage().peak_depth);
    // Admission keeps no output, so it charges no bytes.
    assert_eq!(shaped.usage().bytes, 0);

    // The exact node count passes, and one node fewer is refused.
    let nodes = shaped.usage().nodes;
    let mut exact = CodecWork::new(CodecLimits { nodes, ..wide() }).unwrap();
    exact.admit_serialized(&forms()).unwrap();
    assert_eq!(
        exact.admit_serialized(&()).unwrap_err(),
        error("local JSON node/depth limit")
    );
    let mut short = CodecWork::new(CodecLimits {
        nodes: nodes - 1,
        ..wide()
    })
    .unwrap();
    assert_eq!(
        short.admit_serialized(&forms()).unwrap_err(),
        error("local JSON node/depth limit")
    );
    // The depth ceiling also applies before any output exists.
    let depth = shaped.usage().peak_depth;
    let mut shallow = CodecWork::new(CodecLimits {
        depth: depth - 1,
        ..wide()
    })
    .unwrap();
    assert_eq!(
        shallow.admit_serialized(&forms()).unwrap_err(),
        error("local JSON node/depth limit")
    );
}

#[test]
fn shape_admission_refuses_unsupported_shapes_and_keeps_spent_work() {
    use serde::ser::{Error as _, SerializeMap};
    /// Each serializer misuse that the preflight refuses.
    #[derive(Clone, Copy)]
    enum Bad {
        /// The value reports its own error.
        Custom,
        /// The value claims the raw JSON encoding of `serde_json`.
        Raw,
        /// A map key is an array.
        ContainerKey,
        /// A map key is an enum variant with data.
        TaggedKey,
        /// A map key is null.
        NullKey,
        /// A map value comes before its key.
        ValueFirst,
        /// Two map keys come in a row.
        SecondKey,
        /// A map ends after a key without its value.
        DanglingKey,
    }
    /// A variant with data, used as a map key.
    #[derive(Serialize)]
    enum Key {
        /// Serialized as an object.
        Tagged(u8),
    }
    impl Serialize for Bad {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            if let Self::Custom = self {
                return Err(S::Error::custom("fixture serializer error"));
            }
            if let Self::Raw = self {
                return serializer.serialize_unit_struct("$serde_json::private::RawValue");
            }
            let mut map = serializer.serialize_map(Some(1))?;
            match self {
                Self::ContainerKey => {
                    map.serialize_key(&vec![1u8])?;
                    map.serialize_value(&1u8)?;
                }
                Self::TaggedKey => {
                    map.serialize_key(&Key::Tagged(1))?;
                    map.serialize_value(&1u8)?;
                }
                Self::NullKey => {
                    map.serialize_key(&())?;
                    map.serialize_value(&1u8)?;
                }
                Self::ValueFirst => map.serialize_value(&1u8)?,
                Self::SecondKey => {
                    map.serialize_key("first")?;
                    map.serialize_key("second")?;
                }
                _ => map.serialize_key("first")?,
            }
            map.end()
        }
    }
    for (bad, message) in [
        (Bad::Custom, "fixture serializer error"),
        (Bad::Raw, "unsupported audit raw JSON encoding"),
        (Bad::ContainerKey, "unsupported audit JSON map key"),
        (Bad::TaggedKey, "unsupported audit JSON map key"),
        (Bad::NullKey, "unsupported audit JSON map key"),
        (Bad::ValueFirst, "audit map value without key"),
        (Bad::SecondKey, "audit map key without value"),
        (Bad::DanglingKey, "audit map key without value"),
    ] {
        let mut work = CodecWork::new(wide()).unwrap();
        assert_eq!(work.admit_serialized(&bad).unwrap_err(), error(message));
        // The ledger keeps the work spent before the refusal and stays usable.
        let used = work.usage().nodes;
        let good = std::collections::BTreeMap::from([("first", 1u8)]);
        work.admit_serialized(&good).unwrap();
        assert_eq!(work.usage().nodes, used + 3);
    }
}
