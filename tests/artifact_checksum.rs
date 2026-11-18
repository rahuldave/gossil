use gossil::artifact_checksum::{Md5Checksum, md5};
use gossil::content_hash::hex;

#[test]
fn rfc_1321_vectors() {
    for (input, expected) in [
        ("", "d41d8cd98f00b204e9800998ecf8427e"),
        ("a", "0cc175b9c0f1b6a831c399e269772661"),
        ("abc", "900150983cd24fb0d6963f7d28e17f72"),
        ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
        (
            "abcdefghijklmnopqrstuvwxyz",
            "c3fcd3d76192e4007dfb496cca67e13b",
        ),
        (
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
            "d174ab98d277d9f5a5611c2c9f419d9f",
        ),
        (
            "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
            "57edf4a22be3c955ac49da2e2107b67a",
        ),
    ] {
        assert_eq!(hex(&md5(input.as_bytes())), expected, "{input:?}");
    }
}

#[test]
fn native_bytes_and_chunking() {
    let cases: Vec<(Vec<u8>, &str)> =
        serde_json::from_str(include_str!("fixtures/artifact-checksum/cases.json")).unwrap();
    for (bytes, expected) in cases {
        assert_eq!(hex(&md5(&bytes)), expected);
        for size in [1, 2, 7, 55, 56, 63, 64, 65, 128] {
            let mut checksum = Md5Checksum::new();
            checksum.update(&[]);
            for chunk in bytes.chunks(size) {
                checksum.update(chunk);
                checksum.update(&[]);
            }
            assert_eq!(
                hex(&checksum.finish()),
                expected,
                "len={} chunk={size}",
                bytes.len()
            );
        }
    }
}

#[test]
fn independent_owned_computations() {
    let mut first = Md5Checksum::new();
    let mut second = Md5Checksum::default();
    first.update(b"a");
    second.update(b"message ");
    first.update(b"bc");
    second.update(b"digest");
    assert_eq!(hex(&first.finish()), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(hex(&second.finish()), "f96b697d7cb7938d525a2f31aaf161d0");
}
