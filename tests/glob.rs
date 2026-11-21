//! Fossil glob.test (2013 D. Richard Hipp, 2-Clause BSD) assertion port.
use std::process::Command;

#[cfg(unix)]
#[path = "support/closed_stdout.rs"]
mod closed_stdout;

/// Decode exact UTF-8 test arguments captured from Tcl source evaluation.
fn unhex(s: &str) -> String {
    String::from_utf8(
        s.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect(),
    )
    .unwrap()
}
/// Replay all 20 pinned upstream assertion families against the Rust diagnostic.
#[test]
fn upstream_assertions() {
    for line in include_str!("../evidence/glob/upstream-cases.hex").lines() {
        let row: Vec<_> = line.split(' ').collect();
        let out = Command::new(env!("CARGO_BIN_EXE_gossil"))
            .args(["vcs", "test-glob", &unhex(row[1]), &unhex(row[2])])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "glob-parse-{}: {}",
            row[0],
            String::from_utf8_lossy(&out.stderr)
        );
        let normalize = |s: String| s.replace("\r\n", "\n").replace('\r', "\n");
        assert_eq!(
            normalize(String::from_utf8(out.stdout).unwrap()).trim_end_matches('\n'),
            normalize(unhex(row[3])),
            "glob-parse-{}",
            row[0]
        );
    }
}

use gossil::glob::{GlobError, Limits, glob_create, glob_expr, glob_multi_match};
/// Supply finite list, candidate, pattern-count and SQL budgets for these controls.
fn limits() -> Limits {
    Limits {
        list_bytes: 4096,
        candidate_bytes: 4096,
        patterns: 128,
        expression_bytes: 16384,
    }
}
/// Check native separators, quotes, NUL prefixes, match indices and engine quirks.
#[test]
fn source_boundaries() {
    let l = limits();
    for delimiter in [b' ', 9, 10, 11, 12, 13, b','] {
        let list = [b'a', delimiter, b'b'];
        assert_eq!(
            glob_create(Some(&list), l)
                .unwrap()
                .patterns()
                .collect::<Vec<_>>(),
            vec![b"a".as_slice(), b"b"]
        );
        assert_eq!(glob_multi_match(Some(&list), b"b", l).unwrap(), 2);
    }
    for list in [None, Some(b"".as_slice()), Some(b", \t\n\r\x0b\x0c")] {
        assert_eq!(glob_create(list, l).unwrap().glob_match(b"").unwrap(), 0);
        assert_eq!(glob_expr(b"x", list, l).unwrap(), b"0");
    }
    let g = glob_create(Some(b"'a'bc,\"\", 'unterminated"), l).unwrap();
    assert_eq!(
        g.patterns().collect::<Vec<_>>(),
        vec![b"a".as_slice(), b"bc", b"", b"unterminated"]
    );
    assert_eq!(
        glob_expr(b"x", Some(b"'a'bc"), l).unwrap(),
        b"(x GLOB 'a' OR x GLOB 'c')"
    );
    assert_eq!(g.glob_match(b"").unwrap(), 3);
    assert_eq!(glob_multi_match(Some(b"a*,abc,*"), b"abc", l).unwrap(), 1);
    assert_eq!(glob_multi_match(Some(b"z,a*,abc"), b"abc", l).unwrap(), 2);
    assert_eq!(glob_multi_match(Some(b"abc\0z"), b"abc\0z", l).unwrap(), 1);
    assert_eq!(
        glob_expr(b"x\0ignored", Some(b"'a''b'\0ignored"), l).unwrap(),
        b"(x GLOB 'a' OR x GLOB 'b''')"
    );
    for n in [99, 100, 4096] {
        let p = vec![b'a'; n];
        assert_eq!(glob_multi_match(Some(&p), &p, l).unwrap(), 1);
    }
    for (p, v, r) in [
        ("?", "é", 1),
        ("[a-c]", "b", 1),
        ("[^a]", "b", 1),
        ("[", "[", 0),
        ("a*", "a/b", 1),
        ("A", "a", 0),
        ("*", ".git/x", 1),
        ("a\\b", "a\\b", 1),
    ] {
        assert_eq!(
            glob_multi_match(Some(p.as_bytes()), v.as_bytes(), l).unwrap(),
            r,
            "{p}"
        );
    }
    assert_eq!(glob_multi_match(Some(&[255]), &[255], l).unwrap(), 1);
}
/// Check reusable parsing, matching and SQL output at finite budget boundaries.
#[test]
fn finite_budgets() {
    let mut l = limits();
    l.list_bytes = 1;
    assert!(matches!(
        glob_create(Some(b"ab"), l),
        Err(GlobError::Limit("list"))
    ));
    assert!(matches!(
        glob_expr(b"x", Some(b"ab"), l),
        Err(GlobError::Limit("list"))
    ));
    assert!(glob_create(Some(b"a\0b"), l).is_ok());
    l = limits();
    l.patterns = 0;
    assert!(matches!(
        glob_create(Some(b"''"), l),
        Err(GlobError::Limit("patterns"))
    ));
    assert!(matches!(
        glob_expr(b"x", Some(b"a"), l),
        Err(GlobError::Limit("patterns"))
    ));
    l = limits();
    l.candidate_bytes = 0;
    assert!(matches!(
        glob_create(Some(b"*"), l).unwrap().glob_match(b"a"),
        Err(GlobError::Limit("candidate"))
    ));
    l = limits();
    l.candidate_bytes = 1;
    let reusable = glob_create(Some(b"*"), l).unwrap();
    assert_eq!(reusable.glob_match(b"a").unwrap(), 1);
    assert_eq!(reusable.glob_match(b"a\0ignored").unwrap(), 1);
    assert!(matches!(
        reusable.glob_match(b"ab"),
        Err(GlobError::Limit("candidate"))
    ));
    l = limits();
    l.expression_bytes = 0;
    assert!(matches!(
        glob_expr(b"x", None, l),
        Err(GlobError::Limit("expression"))
    ));
    for n in 0..12 {
        l.expression_bytes = n;
        assert!(glob_expr(b"x", Some(b"a"), l).is_err());
    }
    l.expression_bytes = 12;
    assert_eq!(glob_expr(b"x", Some(b"a"), l).unwrap(), b"(x GLOB 'a')");
}
/// Reject missing diagnostic arguments and unsupported settings lookup.
#[test]
fn cli_rejects_incomplete_and_settings() {
    for args in [
        vec!["vcs", "test-glob"],
        vec!["vcs", "test-glob", "@ignore-glob", "x"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_gossil"))
            .args(args)
            .output()
            .unwrap();
        assert!(!out.status.success());
    }
}

/// Compare generated SQL with selected-engine matching, retaining quote divergence.
#[tokio::test]
async fn selected_engine_sql_agrees() {
    let db = libsql::Builder::new_local(":memory:")
        .build()
        .await
        .unwrap();
    let conn = db.connect().unwrap();
    for (pattern, value) in [
        ("*", "a/b"),
        ("?", "é"),
        ("[a-z]", "j"),
        ("[^a]", "b"),
        ("[", "["),
        ("'a'bc", "c"),
    ] {
        let expr = glob_expr(b"?1", Some(pattern.as_bytes()), limits()).unwrap();
        let sql = format!("SELECT {}", String::from_utf8(expr).unwrap());
        let mut rows = conn.query(&sql, [value]).await.unwrap();
        let sql_match: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        if pattern != "'a'bc" {
            assert_eq!(
                sql_match != 0,
                glob_multi_match(Some(pattern.as_bytes()), value.as_bytes(), limits()).unwrap()
                    != 0
            );
        } else {
            assert_eq!(sql_match, 1);
        }
    }
}
/// Check one-shot early success, exhausted budgets and retained error causes.
#[test]
fn multi_limits_and_failures() {
    let mut l = limits();
    l.patterns = 1;
    assert_eq!(glob_multi_match(Some(b"a,b"), b"a", l).unwrap(), 1);
    assert!(matches!(
        glob_multi_match(Some(b"a,b"), b"z", l),
        Err(GlobError::Limit("patterns"))
    ));
    l.list_bytes = 0;
    assert!(matches!(
        glob_multi_match(Some(b"a"), b"a", l),
        Err(GlobError::Limit("list"))
    ));
    l = limits();
    l.candidate_bytes = 0;
    assert!(matches!(
        glob_multi_match(Some(b"a"), b"a", l),
        Err(GlobError::Limit("candidate"))
    ));
    assert_eq!(glob_multi_match(None, b"", l).unwrap(), 0);
    assert_eq!(glob_multi_match(Some(b", \t\x0b"), b"", l).unwrap(), 0);
    for e in [
        GlobError::Limit("list"),
        GlobError::Overflow,
        GlobError::Allocation(Vec::<u8>::new().try_reserve(usize::MAX).unwrap_err()),
    ] {
        assert!(!e.to_string().is_empty());
        if matches!(e, GlobError::Allocation(_)) {
            assert!(std::error::Error::source(&e).is_some());
        } else {
            assert!(std::error::Error::source(&e).is_none());
        }
    }
}

/// A consumer closing stdout must produce a failed command with its output path.
#[cfg(unix)]
#[test]
fn cli_reports_closed_stdout() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gossil"));
    command.args(["vcs", "test-glob", "a", "a"]);
    let out = closed_stdout::configure(command).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        out.stderr,
        b"unable to open file \"<stdout>\" for writing\n"
    );
}

/// Reusable matching keeps source order across overlapping and absent matches.
#[test]
fn reusable_first_match_order() {
    for (list, expected) in [(b"a*,abc,*".as_slice(), 1), (b"z,a*,abc", 2)] {
        let glob = glob_create(Some(list), limits()).unwrap();
        assert_eq!(glob.glob_match(b"abc").unwrap(), expected);
        assert_eq!(glob.glob_match(b"abc").unwrap(), expected);
    }
    let glob = glob_create(Some(b"z,a*"), limits()).unwrap();
    assert_eq!(glob.glob_match(b"no-match").unwrap(), 0);
    assert_eq!(glob.glob_match(b"abc").unwrap(), 2);
}

/// Execute generated SQL, distinguishing an absent list from a SQL NULL value.
#[tokio::test]
async fn selected_engine_sql_boundaries() {
    let db = libsql::Builder::new_local(":memory:")
        .build()
        .await
        .unwrap();
    let conn = db.connect().unwrap();
    for (list, value, expected) in [
        (Some(b"a'b".as_slice()), "a'b", 1i64),
        (Some(b"z*,a'b,other"), "a'b", 1),
        (Some(b"z*,a'b,other"), "absent", 0),
        (Some(b"''"), "", 1),
        (Some(b""), "", 0),
        (Some(b", \t"), "", 0),
        (None, "anything", 0),
    ] {
        let expression = glob_expr(b"?1", list, limits()).unwrap();
        // Retain a parameter even when the generated expression is constant zero.
        let sql = format!(
            "SELECT {} FROM (SELECT ?1)",
            String::from_utf8(expression).unwrap()
        );
        let mut rows = conn.query(&sql, [value]).await.unwrap();
        let actual: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
        assert_eq!(actual, expected, "{list:?} {value:?}");
        assert_eq!(
            glob_create(list, limits())
                .unwrap()
                .glob_match(value.as_bytes())
                .unwrap()
                != 0,
            expected != 0
        );
    }
    let expression = glob_expr(b"NULL", Some(b"*"), limits()).unwrap();
    let sql = format!("SELECT {} IS NULL", String::from_utf8(expression).unwrap());
    let mut rows = conn.query(&sql, ()).await.unwrap();
    assert_eq!(
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
        1
    );
    assert_eq!(glob_expr(b"NULL", None, limits()).unwrap(), b"0");
}
