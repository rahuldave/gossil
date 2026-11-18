//! Portable replay of pinned native decoder observations.
use gossil::delta::{Limits, apply, output_size};
use serde_json::Value;

/// Decode the durable exact-byte fixture representation.
fn bytes(value: &Value) -> Vec<u8> {
    let text = value.as_str().unwrap();
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

/// Replay every bounded native apply observation and nine native encoder vectors.
#[test]
fn pinned_observations() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/delta/cases.json")).unwrap();
    let mut counts = [0; 3];
    for case in cases {
        if case["status"] == "not-run" {
            counts[2] += 1;
            continue;
        }
        let result = apply(
            &bytes(&case["source"]),
            &bytes(&case["delta"]),
            Limits::default(),
        );
        if case["status"] == "accepted" {
            assert_eq!(result.unwrap(), bytes(&case["target"]), "{}", case["id"]);
            if case["group"] == "original" {
                counts[0] += 1;
            }
        } else {
            assert!(result.is_err(), "{}", case["id"]);
            if case["group"] == "original" {
                counts[1] += 1;
            }
        }
    }
    assert_eq!(counts, [35, 24, 5]);
}

/// Header inspection does not validate commands or impose apply's allocation limit.
#[test]
fn header_only_and_limits() {
    assert_eq!(output_size(b"3~~~~~\n").unwrap(), u32::MAX);
    assert_eq!(output_size(b"400000\ngarbage").unwrap(), 0);
    assert_eq!(apply(b"", b"1\n1:A;", Limits::default()).unwrap(), b"A");
}

/// Keep exact typed failure categories and byte offsets separate from CLI presentation.
#[test]
fn typed_failures_and_explicit_cap() {
    use gossil::delta::DeltaError::*;
    let decode = |delta: &[u8]| apply(b"ABC", delta, Limits::default());
    assert!(matches!(decode(b"x"), Err(MissingHeaderNewline)));
    assert!(matches!(decode(b"0\n1"), Err(TruncatedCommand { at: 3 })));
    assert!(matches!(decode(b"0\n0@0"), Err(MissingCopyComma { at: 5 })));
    assert!(matches!(
        decode(b"0\n0!"),
        Err(UnknownOperator { at: 3, byte: b'!' })
    ));
    assert!(matches!(
        decode(b"0\n0@4,;"),
        Err(SourceRange {
            offset: 4,
            count: 0,
            source_len: 3
        })
    ));
    assert!(matches!(
        decode(b"0\n1:A;"),
        Err(OutputRange {
            produced: 0,
            count: 1,
            advertised: 0
        })
    ));
    assert!(matches!(
        decode(b"2\n2:A"),
        Err(TruncatedLiteral {
            at: 4,
            count: 2,
            remaining: 1
        })
    ));
    assert!(matches!(decode(b"0\n\0;"), Err(Unterminated)));
    assert!(matches!(
        decode(b"0\n0\0;"),
        Err(UnknownOperator { at: 3, byte: 0 })
    ));
    assert!(matches!(
        decode(b"1\n;"),
        Err(SizeMismatch {
            advertised: 1,
            produced: 0
        })
    ));
    assert!(matches!(
        decode(b"1~~~~~\n;"),
        Err(SizeMismatch {
            advertised: 2147483647,
            produced: 0
        })
    ));
    for (header, expected) in [
        (b"200000\n;".as_slice(), 2147483648),
        (b"3~~~~~\n;".as_slice(), u32::MAX),
    ] {
        assert!(
            matches!(decode(header), Err(OutputTooLarge { advertised, limit: 2147483647 }) if advertised == expected)
        );
    }
    let delta = [b"11\n11:".as_slice(), &[b'A'; 65], b";"].concat();
    assert!(matches!(
        apply(
            b"",
            &delta,
            Limits {
                max_output_bytes: 64
            }
        ),
        Err(OutputTooLarge {
            advertised: 65,
            limit: 64
        })
    ));
    assert_eq!(
        apply(
            b"",
            &delta,
            Limits {
                max_output_bytes: 65
            }
        )
        .unwrap(),
        [b'A'; 65]
    );
    assert_eq!(apply(b"", &delta, Limits::default()).unwrap(), [b'A'; 65]);
    assert_eq!(decode(b"0\n0@3,;").unwrap(), b"");
}

/// Inspect every possible unsigned input byte without seven-bit aliasing.
#[test]
fn all_header_bytes() {
    let alphabet = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ_abcdefghijklmnopqrstuvwxyz~";
    for byte in 0..=255u8 {
        let input = [byte, b'\n'];
        let result = output_size(&input);
        if let Some(value) = alphabet.iter().position(|digit| *digit == byte) {
            assert_eq!(result.unwrap(), value as u32);
        } else if byte == b'\n' {
            assert_eq!(result.unwrap(), 0);
        } else {
            assert!(result.is_err(), "{byte}");
        }
    }
}

/// Bind all portable fixture bytes and provenance copies to their recorded hashes.
#[test]
fn portable_fixture_integrity() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/delta");
    let index: std::collections::BTreeMap<String, Value> =
        serde_json::from_str(include_str!("fixtures/delta/portable-index.json")).unwrap();
    for (path, expected) in index {
        let bytes = std::fs::read(root.join(&path)).unwrap();
        assert_eq!(
            bytes.len() as u64,
            expected["bytes"].as_u64().unwrap(),
            "{path}"
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            expected["sha256"].as_str().unwrap(),
            "{path}"
        );
    }
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/delta/cases.json")).unwrap();
    let original: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/delta/original/results.json")).unwrap();
    let supplement: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/delta/supplement/results.json")).unwrap();
    assert_eq!(original.len(), 64);
    assert_eq!(supplement.len(), 27);
    assert_eq!(cases.len(), 81);
    for case in cases {
        let reference = if case["group"] == "original" {
            &original
        } else {
            &supplement
        }
        .iter()
        .find(|row| row["id"] == case["id"])
        .unwrap();
        if case["group"] == "original" {
            assert_eq!(case["source"], reference["input_bytes"]["source_hex"]);
            assert_eq!(case["delta"], reference["input_bytes"]["delta_hex"]);
            assert_eq!(case["status"], reference["status"]);
            if case["status"] != "not-run" {
                assert_eq!(case["target"], reference["observed_target_hex"]);
            }
        } else if case["group"] == "golden" {
            assert_eq!(case["source"], reference["before"]["origin"]["hex"]);
            assert_eq!(case["delta"], reference["after"]["target"]["hex"]);
            assert_eq!(case["target"], reference["before"]["target_input"]["hex"]);
        } else {
            assert_eq!(case["source"], reference["before"]["origin"]["hex"]);
            assert_eq!(case["delta"], reference["before"]["delta"]["hex"]);
            assert_eq!(case["target"], reference["after"]["target"]["hex"]);
        }
        let dir = root
            .join("bytes")
            .join(case["group"].as_str().unwrap())
            .join(case["id"].as_str().unwrap());
        for key in ["source", "delta", "target", "stdout", "stderr"] {
            assert_eq!(
                std::fs::read(dir.join(format!("{key}.bin"))).unwrap(),
                bytes(&case[key])
            );
        }
    }
}
