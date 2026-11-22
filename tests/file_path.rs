//! Check the inherited lexical cases and the explicit controls of the resource bounds.
use gossil::file_path::{
    Limits, PathError, SourcePlatform, simplify_name, simplify_name_for_source,
};
use serde_json::Value;

/// Supply finite limits for small lexical examples.
fn limits() -> Limits {
    Limits {
        max_input_bytes: 1024,
        max_output_bytes: 1024,
    }
}

/// Preserve all 25 arguments and assertions of `simplify-name` 100 to 107.
#[test]
fn inherited_simplify_name_100_through_107() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/file-path/inherited.json")).unwrap();
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 25);
    for case in cases {
        assert_eq!(
            simplify_name(case["input"].as_str().unwrap().as_bytes(), false, limits()).unwrap(),
            case["expected"].as_str().unwrap().as_bytes(),
            "{}",
            case["id"]
        );
    }
}

/// Check the rules of the source for the trailing slash, byte preservation and cancellation.
#[test]
fn source_derived_lexical_controls() {
    for (input, keep, expected) in [
        (&b"a///"[..], true, &b"a/"[..]),
        (b"///", true, b"//"),
        // Native file.c:1229 retains tail bytes observed by lookahead at 1238.
        (b"a///./", true, b"a"),
        (b"//", false, b"/"),
        (b"//a", false, b"//a"),
        (b"./", true, b"/"),
        (b"/a/..", false, b"/"),
        (b"a/../", true, b"/"),
        (b"/../../x", false, b"/../../x"),
        (b"../a/../b", false, b"../b"),
        (b"a/.", true, b"a"),
        (b"a\\b", false, b"a\\b"),
        (b"\xff/a/../\xfe", false, b"\xff/\xfe"),
        (b"/.../..", false, b"/"),
        (b"", true, b""),
    ] {
        assert_eq!(simplify_name(input, keep, limits()).unwrap(), expected);
    }
}

/// Enforce input, actual output and NUL boundaries independently.
#[test]
fn caller_bounds_and_diagnostics() {
    let bounds = Limits {
        max_input_bytes: 4,
        max_output_bytes: 1,
    };
    assert!(matches!(
        simplify_name(b"abcde", false, bounds),
        Err(PathError::InputLimit)
    ));
    assert!(matches!(
        simplify_name(b"ab", false, bounds),
        Err(PathError::OutputLimit)
    ));
    assert!(matches!(
        simplify_name(b"a\0b", false, bounds),
        Err(PathError::EmbeddedNul)
    ));
    assert_eq!(simplify_name(b"/./", false, bounds).unwrap(), b"/");
    for name in [b"a/..".as_slice(), b"a/../", b".../.."] {
        assert!(matches!(
            simplify_name(name, false, limits()),
            Err(PathError::TerminalCancellation)
        ));
    }
    let zero = Limits {
        max_input_bytes: 0,
        max_output_bytes: 0,
    };
    assert_eq!(simplify_name(b"", false, zero).unwrap(), b"");
    use std::error::Error;
    for (error, message) in [
        (PathError::InputLimit, "filename input exceeds its limit"),
        (PathError::OutputLimit, "filename output exceeds its limit"),
        (PathError::EmbeddedNul, "filename contains NUL"),
        (
            PathError::TerminalCancellation,
            "relative terminal cancellation has no safe native result",
        ),
    ] {
        assert_eq!(error.to_string(), message);
        assert!(error.source().is_none());
    }
}

/// Keep a reservation failure visible in the message and as the error source.
#[test]
fn allocation_error_preserves_its_cause() {
    use std::error::Error;
    let cause = Vec::<u8>::new().try_reserve_exact(usize::MAX).unwrap_err();
    let expected = cause.to_string();
    let error = PathError::Allocation(cause);
    assert_eq!(
        error.to_string(),
        format!("filename allocation failed: {expected}")
    );
    assert_eq!(error.source().unwrap().to_string(), expected);
}

/// Replay the 14 Unix source-adapter cases, with both trailing slash options.
#[test]
fn unix_adapter_plan() {
    let plan: Value =
        serde_json::from_str(include_str!("fixtures/file-path/adapter-plan.json")).unwrap();
    let cases = plan["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 14);
    for case in cases {
        let result = simplify_name(
            case["input"].as_str().unwrap().as_bytes(),
            case["slash"].as_u64().unwrap() != 0,
            limits(),
        )
        .unwrap();
        let hex = result
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            case["expected_hex"].as_str().unwrap(),
            "{}",
            case["id"]
        );
    }
}

/// Preserve six inherited Windows assertions and their native returned spans.
#[test]
fn inherited_windows_108_through_109() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/file-path/windows-pending.json")).unwrap();
    let cases = cases["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let result = simplify_name_for_source(
            case["input"].as_str().unwrap().as_bytes(),
            false,
            limits(),
            SourcePlatform::WindowsCygwin,
        )
        .unwrap();
        let expected = case["expected"].as_str().unwrap().as_bytes();
        assert_eq!(result.visible_bytes(), expected, "{}", case["id"]);
        let span = if expected.is_empty() {
            &[0][..]
        } else {
            expected
        };
        assert_eq!(result.reported_span(), span, "{}", case["id"]);
    }
}

/// Each pinned Windows/Cygwin case holds an id, an input, a slash flag, a reported span and
/// visible bytes in hex.
const WINDOWS_CASES: [(&str, &str, u64, &str, &str); 18] = [
    (
        "simplify-name-108.1",
        "//?/a:/a/b",
        0,
        "613a2f612f62",
        "613a2f612f62",
    ),
    (
        "simplify-name-108.2",
        "//?/UNC/a/b",
        0,
        "2f2f612f62",
        "2f2f612f62",
    ),
    ("simplify-name-108.3", "//?/", 0, "00", ""),
    (
        "simplify-name-109.1",
        "\\\\?\\a:\\a\\b",
        0,
        "613a2f612f62",
        "613a2f612f62",
    ),
    (
        "simplify-name-109.2",
        "\\\\?\\UNC\\a\\b",
        0,
        "2f2f612f62",
        "2f2f612f62",
    ),
    ("simplify-name-109.3", "\\\\?\\", 0, "00", ""),
    (
        "lowercase-unc",
        "//?/unc/a/b",
        0,
        "2f2f612f62",
        "2f2f612f62",
    ),
    ("mixed-unc", "//?/uNc/a/b", 0, "2f2f612f62", "2f2f612f62"),
    ("unc-no-slash", "//?/UNC", 0, "2f", "2f"),
    ("unc-suffix", "//?/UNCx", 0, "2f78", "2f78"),
    ("prefix-u", "//?/U", 0, "55", "55"),
    ("prefix-un", "//?/UN", 0, "554e", "554e"),
    ("short-prefix", "//?", 0, "2f2f3f", "2f2f3f"),
    ("one-backslash", "a\\b", 0, "612f62", "612f62"),
    ("backslash-tail", "a\\\\\\.\\", 1, "61", "61"),
    (
        "mixed-separators",
        "\\\\?\\uNc/a\\b",
        0,
        "2f2f612f62",
        "2f2f612f62",
    ),
    ("nonunc", "//?/UNx/a", 0, "554e782f61", "554e782f61"),
    (
        "ordinary-unicode",
        "//?/\u{fc}nc/a",
        0,
        "c3bc6e632f61",
        "c3bc6e632f61",
    ),
];

/// Exercise the source prefixes, the lookahead into the original tail and ordinary backslashes.
///
/// The test pins all 18 cases of the plan, so a dropped, added or changed case fails.
#[test]
fn windows_source_controls() {
    let plan: Value =
        serde_json::from_str(include_str!("fixtures/file-path/windows-adapter-plan.json")).unwrap();
    let cases = plan["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 18);
    let recorded: Vec<_> = cases
        .iter()
        .map(|case| {
            (
                case["id"].as_str().unwrap(),
                case["input"].as_str().unwrap(),
                case["slash"].as_u64().unwrap(),
                case["expected_hex"].as_str().unwrap(),
                case["visible_hex"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(recorded, WINDOWS_CASES);
    for case in cases {
        let result = simplify_name_for_source(
            case["input"].as_str().unwrap().as_bytes(),
            case["slash"].as_u64().unwrap() != 0,
            limits(),
            SourcePlatform::WindowsCygwin,
        )
        .unwrap();
        let hex = |bytes: &[u8]| {
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        assert_eq!(
            hex(result.reported_span()),
            case["expected_hex"].as_str().unwrap(),
            "{}",
            case["id"]
        );
        assert_eq!(
            hex(result.visible_bytes()),
            case["visible_hex"].as_str().unwrap(),
            "{}",
            case["id"]
        );
    }
    assert_eq!(
        simplify_name_for_source(b"a\\b", false, limits(), SourcePlatform::Unix)
            .unwrap()
            .reported_span(),
        b"a\\b"
    );
}

/// Bounds count native reported bytes even when the visible name is empty.
#[test]
fn windows_source_bounds() {
    let zero_output = Limits {
        max_input_bytes: 32,
        max_output_bytes: 0,
    };
    assert!(matches!(
        simplify_name_for_source(b"//?/", false, zero_output, SourcePlatform::WindowsCygwin),
        Err(PathError::OutputLimit)
    ));
    let one_output = Limits {
        max_input_bytes: 4,
        max_output_bytes: 1,
    };
    let name = simplify_name_for_source(b"//?/", false, one_output, SourcePlatform::WindowsCygwin)
        .unwrap();
    assert_eq!(name.reported_span(), &[0]);
    assert!(name.visible_bytes().is_empty());
    assert!(matches!(
        simplify_name_for_source(b"//?/U", false, one_output, SourcePlatform::WindowsCygwin),
        Err(PathError::InputLimit)
    ));
    assert!(matches!(
        simplify_name_for_source(b"a\0b", false, limits(), SourcePlatform::WindowsCygwin),
        Err(PathError::EmbeddedNul)
    ));
    assert!(matches!(
        simplify_name_for_source(b"a\\..", false, limits(), SourcePlatform::WindowsCygwin),
        Err(PathError::TerminalCancellation)
    ));
    assert!(
        simplify_name_for_source(b"", false, zero_output, SourcePlatform::WindowsCygwin)
            .unwrap()
            .reported_span()
            .is_empty()
    );
}

/// Pin the cited lines of `src/file.c` at Fossil check-in `7e8ecf0ce4` in the page and the fixture.
///
/// The pristine source has `backup_dir` at lines 1172 to 1180, `file_simplify_name` at lines 1195
/// to 1260 and its Windows and Cygwin conversion branch at lines 1201 to 1216.
#[test]
fn fossil_source_citations() {
    let page = include_str!("../docs/file-path.md").replace('\n', " ");
    assert!(page.contains("lines 1172 to 1180 for `backup_dir`"));
    assert!(page.contains("lines 1195 to 1260 for `file_simplify_name`"));
    let pending: Value =
        serde_json::from_str(include_str!("fixtures/file-path/windows-pending.json")).unwrap();
    assert_eq!(
        pending["source_branch"],
        "src/file.c:1201-1216 (Windows/Cygwin conversion branch)"
    );
}

/// Pin the status fields of the fixtures to what this tree has run.
///
/// The port is compiled and tested, and the `TerminalCancellation` contract is adopted. No
/// native C adapter run is recorded, and no case has run on a native Windows or Cygwin host.
#[test]
fn fixture_status_fields() {
    let plan: Value =
        serde_json::from_str(include_str!("fixtures/file-path/adapter-plan.json")).unwrap();
    assert_eq!(
        plan["status"],
        "source-derived expected values; port implemented, compiled and tested by \
         tests/file_path.rs; no native C adapter run is recorded in the repository"
    );
    assert_eq!(
        plan["excluded"],
        "No relative terminal cancellation with slash=false or terminal-negative-cursor inputs; \
         the adopted TerminalCancellation contract rejects them and is tested in tests/file_path.rs."
    );
    let pending: Value =
        serde_json::from_str(include_str!("fixtures/file-path/windows-pending.json")).unwrap();
    let cases = pending["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    for case in cases {
        assert_eq!(
            case["port_status"], "source-implemented-compiled-tested",
            "{}",
            case["id"]
        );
        assert_eq!(
            case["native_platform_qualification"], "not-run",
            "{}",
            case["id"]
        );
    }
    assert_eq!(
        pending["recommendations"],
        serde_json::json!(["Obtain real Windows/Cygwin runtime qualification."])
    );
}
