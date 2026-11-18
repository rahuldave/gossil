//! Compare safe byte APIs against pinned native observations and derived output.
use gossil::token_codec::{defossilize, fossilize, fossilize_c_string};
use serde_json::Value;

/// Decode the fixture's exact hexadecimal bytes.
fn unhex(value: &Value) -> Vec<u8> {
    value
        .as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
/// Check every native observation, retaining complete output after decoded NUL.
fn pinned_reference_cases() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/token-codec/fixtures.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 538);
    for case in cases {
        let input = unhex(&case["input_hex"]);
        if case["operation"] == "encode" {
            let length = case["length"].as_i64().unwrap();
            let actual = if length < 0 {
                fossilize_c_string(&input)
            } else {
                fossilize(&input[..length as usize])
            }
            .unwrap();
            assert_eq!(
                actual,
                unhex(&case["expected_encoded_hex"]),
                "{}",
                case["id"]
            );
        } else {
            let actual = defossilize(&input).unwrap();
            assert_eq!(
                actual.bytes,
                unhex(&case["expected_emitted_hex"]),
                "{}",
                case["id"]
            );
            let native_stdout = unhex(&case["stdout_hex"]);
            let native_strlen: usize = std::str::from_utf8(&native_stdout)
                .unwrap()
                .split_whitespace()
                .last()
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(actual.c_string_len, native_strlen, "{}", case["id"]);
            assert_eq!(
                actual.c_string_len,
                unhex(&case["expected_c_string_hex"]).len(),
                "{}",
                case["id"]
            );
        }
    }
}
