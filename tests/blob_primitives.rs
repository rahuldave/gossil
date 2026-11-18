//! Byte-level contracts for Fossil digests and packed blobs.
use gossil::{
    compression::{Unpacked, pack, pack_two, unpack},
    content_hash::{hex, sha1, sha3},
};
#[test]
fn byte_digests_match_known_answers() {
    assert_eq!(
        hex(&sha1(b"abc")),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(
        hex(&sha3(b"abc", 256).unwrap()),
        "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
    );
    assert!(sha3(b"abc", 129).is_none());
}
#[test]
fn packed_capacity_and_no_payload_are_distinct() {
    let packed = pack(b"abc", -1).unwrap();
    assert_eq!(&packed[..4], &[0, 0, 0, 3]);
    assert_eq!(
        unpack(&packed, 3).unwrap(),
        Unpacked::Bytes(b"abc".to_vec())
    );
    assert_eq!(unpack(&[0; 4], 0).unwrap(), Unpacked::NoPayload);
    assert_eq!(
        unpack(&pack(b"", -1).unwrap(), 0).unwrap(),
        Unpacked::Bytes(vec![])
    );
    assert!(unpack(&packed, 2).is_err());
    assert!(pack(b"abc", 10).is_err());
    assert_eq!(
        unpack(&pack_two(b"a", b"bc").unwrap(), 3).unwrap(),
        Unpacked::Bytes(b"abc".to_vec())
    );
}

#[test]
fn binary_chunked_round_trips_at_every_level() {
    let input: Vec<u8> = (0..40000)
        .map(|i| ((i * 73 + i / 251) % 256) as u8)
        .collect();
    for level in -1..=9 {
        let packed = pack(&input, level).unwrap();
        assert_eq!(
            unpack(&packed, input.len()).unwrap(),
            Unpacked::Bytes(input.clone())
        );
    }
    for split in [0, 1, 8192, input.len()] {
        let packed = pack_two(&input[..split], &input[split..]).unwrap();
        assert_eq!(
            unpack(&packed, input.len()).unwrap(),
            Unpacked::Bytes(input.clone())
        );
    }
}

#[test]
fn capacity_is_upper_bound_and_trailing_bytes_are_ignored() {
    let mut packed = pack(b"abc", -1).unwrap();
    packed[..4].copy_from_slice(&100_u32.to_be_bytes());
    packed.extend_from_slice(b"trailing\0garbage");
    assert_eq!(
        unpack(&packed, 100).unwrap(),
        Unpacked::Bytes(b"abc".to_vec())
    );
    assert!(unpack(&packed, 99).is_err());
    packed[..4].copy_from_slice(&2_u32.to_be_bytes());
    assert!(matches!(
        unpack(&packed, 100),
        Err(gossil::compression::BlobError::Capacity)
    ));
}

#[test]
fn malformed_checksums_and_incomplete_streams_fail() {
    let packed = pack(b"abc", -1).unwrap();
    for end in 5..packed.len() {
        assert!(unpack(&packed[..end], 3).is_err(), "prefix {end}");
    }
    let mut corrupt = packed.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    let error = unpack(&corrupt, 3).unwrap_err();
    assert!(std::error::Error::source(&error).is_some());
    assert!(!error.to_string().is_empty());
    assert!(unpack(&[0, 0, 0, 0, 0, 0, 0], 0).is_err());
    for length in 0..=4 {
        assert_eq!(unpack(&packed[..length], 0).unwrap(), Unpacked::NoPayload);
    }
    assert!(pack(b"", -2).is_err());
    assert!(sha3(b"", 0).is_none());
    assert!(sha3(b"", 544).is_none());
}

/// Read the committed reference corpus without requiring native Fossil.
fn corpus() -> serde_json::Value {
    serde_json::from_slice(
        &std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/blob/reference.json"
        ))
        .unwrap(),
    )
    .unwrap()
}

/// Decode fixture hexadecimal, retaining binary zero bytes.
fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn every_fossil_sha3_width_and_rate_boundary_matches() {
    let fixtures = corpus();
    let rows = fixtures["sha3"].as_array().unwrap();
    assert_eq!(rows.len(), 13 * 6);
    for row in rows {
        let input = unhex(row["input_hex"].as_str().unwrap());
        let bits = row["bits"].as_u64().unwrap() as u16;
        assert_eq!(
            hex(&sha3(&input, bits).unwrap()),
            row["digest_hex"].as_str().unwrap(),
            "bits={bits} length={}",
            input.len()
        );
    }
    let row = &fixtures["sha1"];
    assert_eq!(
        hex(&sha1(&unhex(row["input_hex"].as_str().unwrap()))),
        row["digest_hex"].as_str().unwrap()
    );
}

#[test]
fn native_packed_fixtures_decode_and_native_cli_noops_remain_explicit() {
    let fixtures = corpus();
    for row in fixtures["packed"].as_array().unwrap() {
        let expected = unhex(row["input_hex"].as_str().unwrap());
        let packed = unhex(row["packed_hex"].as_str().unwrap());
        assert_eq!(
            unpack(&packed, expected.len()).unwrap(),
            Unpacked::Bytes(expected)
        );
    }
    for row in fixtures["native_uncompress"].as_array().unwrap() {
        let input = unhex(row["input_hex"].as_str().unwrap());
        let native = unhex(row["output_hex"].as_str().unwrap());
        match row["case"].as_str().unwrap() {
            "short" => {
                assert_eq!(unpack(&input, 100).unwrap(), Unpacked::NoPayload);
                assert_eq!(native, input);
            }
            "invalid" => {
                assert!(unpack(&input, 100).is_err());
                assert_eq!(native, input);
            }
            _ => assert_eq!(unpack(&input, 100).unwrap(), Unpacked::Bytes(native)),
        }
    }
}

#[test]
#[ignore = "requires explicit pinned native Fossil; run just blob-interop"]
fn rust_packed_bytes_decode_with_native_fossil() {
    let fossil =
        std::env::var_os("GOSSIL_FOSSIL_REFERENCE").expect("explicit reference executable");
    let temp = tempfile::tempdir().unwrap();
    let input_path = temp.path().join("packed");
    let output_path = temp.path().join("decoded");
    for row in corpus()["packed"].as_array().unwrap() {
        let input = unhex(row["input_hex"].as_str().unwrap());
        for level in -1..=9 {
            std::fs::write(&input_path, pack(&input, level).unwrap()).unwrap();
            assert!(
                std::process::Command::new("jlog")
                    .args(["run"])
                    .arg(&fossil)
                    .arg("test-uncompress")
                    .arg(&input_path)
                    .arg(&output_path)
                    .status()
                    .unwrap()
                    .success()
            );
            assert_eq!(std::fs::read(&output_path).unwrap(), input);
        }
        let split = input.len() / 2;
        std::fs::write(
            &input_path,
            pack_two(&input[..split], &input[split..]).unwrap(),
        )
        .unwrap();
        assert!(
            std::process::Command::new("jlog")
                .args(["run"])
                .arg(&fossil)
                .arg("test-uncompress")
                .arg(&input_path)
                .arg(&output_path)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(std::fs::read(&output_path).unwrap(), input);
    }
}

#[test]
/// Preserve Fossil's mitigated outputs for the SHA-mbles collision pair.
fn collision_safe_hashes_match_native_fossil() {
    let fixtures = corpus();
    let rows = fixtures["sha1_collisions"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let plain_collision = "8ac60ba76f1999a1ab70223f225aefdc78d4ddc0";
    let mut digests = Vec::new();
    for row in rows {
        let input = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/blob")
                .join(row["file"].as_str().unwrap()),
        )
        .unwrap();
        let digest = hex(&sha1(&input));
        assert_eq!(digest, row["digest_hex"].as_str().unwrap());
        assert_ne!(digest, plain_collision);
        digests.push(digest);
    }
    assert_ne!(digests[0], digests[1]);
}

#[test]
/// Reject huge declared capacities by policy without attempting huge allocation.
fn declared_capacity_limit_is_explicit_caller_policy() {
    let mut packed = pack(b"abc", -1).unwrap();
    packed[..4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(
        unpack(&packed, 1024),
        Err(gossil::compression::BlobError::Limit)
    ));
    assert_eq!(
        unpack(&packed, u32::MAX as usize).unwrap(),
        Unpacked::Bytes(b"abc".to_vec())
    );
}
