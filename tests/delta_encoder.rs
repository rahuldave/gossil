//! Replay all 28 finite native encoder observations without regenerating bytes.
use gossil::delta::{Limits, apply, create};
#[test]
fn native_encoder_vectors() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/encoder");
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/encoder/provenance.json")).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 28);
    for case in cases.as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let dir = root.join(id);
        let source = std::fs::read(dir.join("origin.bin")).unwrap();
        let target = std::fs::read(dir.join("target.bin")).unwrap();
        let expected = std::fs::read(dir.join("delta.bin")).unwrap();
        let actual = create(&source, &target).unwrap();
        assert_eq!(actual, expected, "{id}");
        assert_eq!(
            apply(&source, &actual, Limits::default()).unwrap(),
            target,
            "{id}"
        );
    }
}

/// Enumerate the complete portable archive and verify every preserved byte.
#[test]
fn encoder_fixture_integrity() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/encoder");
    let index: std::collections::BTreeMap<String, serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/encoder/hash-index.json")).unwrap();
    fn enumerate(root: &std::path::Path, dir: &std::path::Path, names: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                enumerate(root, &path, names);
            } else {
                names.push(path.strip_prefix(root).unwrap().to_str().unwrap().into());
            }
        }
    }
    let mut actual = Vec::new();
    enumerate(&root, &root, &mut actual);
    actual.retain(|name| name != "hash-index.json");
    actual.sort();
    assert_eq!(actual, index.keys().cloned().collect::<Vec<_>>());
    assert_eq!(index.len(), 87);
    for (name, entry) in index {
        let bytes = std::fs::read(root.join(&name)).unwrap();
        assert_eq!(
            bytes.len() as u64,
            entry["bytes"].as_u64().unwrap(),
            "{name}"
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            entry["sha256"].as_str().unwrap(),
            "{name}"
        );
    }
}
