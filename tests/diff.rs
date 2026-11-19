use gossil::diff::{DiffError, DiffLimits, Edit, LineDiffOptions, Whitespace, line_diff};

fn diff(a: &[u8], b: &[u8]) -> Vec<Edit> {
    line_diff(a, b, LineDiffOptions::default(), DiffLimits::default()).unwrap()
}

#[test]
fn basic_triples_and_newlines() {
    assert_eq!(diff(b"", b""), vec![Edit::default()]);
    assert_eq!(
        diff(b"a\nb\n", b"a\nc\n"),
        vec![
            Edit {
                copy: 1,
                delete: 1,
                insert: 1
            },
            Edit::default()
        ]
    );
    assert_eq!(
        diff(b"a\n", b"a"),
        vec![
            Edit {
                copy: 1,
                delete: 0,
                insert: 0
            },
            Edit::default()
        ]
    );
    assert_eq!(
        diff(b"", b"a"),
        vec![
            Edit {
                copy: 0,
                delete: 0,
                insert: 1
            },
            Edit::default()
        ]
    );
}

#[test]
fn binary_and_line_boundaries() {
    assert_eq!(
        line_diff(
            b"a\0b",
            b"",
            LineDiffOptions::default(),
            DiffLimits::default()
        ),
        Err(DiffError::Binary)
    );
    assert_eq!(diff(&vec![b'x'; 32767], b"")[0].delete, 1);
    assert_eq!(
        line_diff(
            &vec![b'x'; 32768],
            b"",
            LineDiffOptions::default(),
            DiffLimits::default()
        ),
        Err(DiffError::Binary)
    );
}

#[test]
fn whitespace_and_cr_options() {
    let mut options = LineDiffOptions {
        whitespace: Whitespace::IgnoreEol,
        ..LineDiffOptions::default()
    };
    assert_eq!(
        line_diff(b"a \n", b"a\n", options, DiffLimits::default()),
        Err(DiffError::WhitespaceOnly)
    );
    options.whitespace = Whitespace::IgnoreAll;
    assert_eq!(
        line_diff(b" a b\n", b"ab\n", options, DiffLimits::default()),
        Err(DiffError::WhitespaceOnly)
    );
    options.whitespace = Whitespace::Exact;
    options.strip_cr = true;
    assert_eq!(
        line_diff(b"a\r\n", b"a\n", options, DiffLimits::default()).unwrap()[0].copy,
        1
    );
}

#[test]
fn caller_limits_are_failures() {
    let limits = DiffLimits {
        max_input_bytes: 1,
        ..DiffLimits::default()
    };
    assert_eq!(
        line_diff(b"ab", b"", LineDiffOptions::default(), limits),
        Err(DiffError::Limit)
    );
    let limits = DiffLimits {
        max_work: 0,
        ..DiffLimits::default()
    };
    assert_eq!(
        line_diff(b"a", b"b", LineDiffOptions::default(), limits),
        Err(DiffError::Limit)
    );
}

#[test]
fn work_budget_applies_after_line_construction() {
    // Two input bytes cost 2 at admission, 2 for line inspection and 2 for
    // hashing. A budget of 6 admits both lines but cannot compare them.
    let limits = DiffLimits {
        max_work: 6,
        ..DiffLimits::default()
    };
    assert_eq!(
        line_diff(b"a", b"b", LineDiffOptions::default(), limits),
        Err(DiffError::Limit)
    );
    let limits = DiffLimits {
        max_work: 100,
        ..limits
    };
    assert_eq!(
        line_diff(b"a", b"b", LineDiffOptions::default(), limits),
        Ok(vec![
            Edit {
                copy: 0,
                delete: 1,
                insert: 1
            },
            Edit::default()
        ])
    );
}

#[test]
fn fixed_combined_line_cap_overrides_larger_caller_limit() {
    const HALF_CAP: usize = 500_000;
    // Empty LF-terminated lines minimize input storage. Identical inputs make
    // the admitted boundary a linear suffix scan rather than a large search.
    let input = vec![b'\n'; HALF_CAP + 1];
    let at_boundary = &input[..HALF_CAP];
    let limits = DiffLimits {
        max_lines: 1_000_001,
        ..DiffLimits::default()
    };
    // Each side is below the fixed cap; only their combined count exceeds it.
    assert_eq!(
        line_diff(&input, at_boundary, LineDiffOptions::default(), limits),
        Err(DiffError::Limit)
    );
    let edits = line_diff(at_boundary, at_boundary, LineDiffOptions::default(), limits).unwrap();
    assert_eq!(
        edits,
        vec![
            Edit {
                copy: HALF_CAP,
                delete: 0,
                insert: 0
            },
            Edit::default()
        ]
    );
    let consumed_from: usize = edits.iter().map(|edit| edit.copy + edit.delete).sum();
    let consumed_to: usize = edits.iter().map(|edit| edit.copy + edit.insert).sum();
    assert_eq!(consumed_from, HALF_CAP);
    assert_eq!(consumed_to, HALF_CAP);
}

#[test]
fn inherited_diff_test_binary_payloads() {
    let cases = [
        (b"".to_vec(), b"\0".to_vec()),
        (
            b"test file 1 (one line no term).".to_vec(),
            vec![b'z'; 32768],
        ),
        (
            b"test file 2 (NUL character).\0".to_vec(),
            b"test file 2 (no NUL character).".to_vec(),
        ),
        (
            [b"test file 3 (long line).".as_slice(), &vec![b'x'; 32768]].concat(),
            b"test file 3 (not a long line).".to_vec(),
        ),
        (
            [
                b"test file 4 (long line).".as_slice(),
                &vec![b'y'; 32768],
                b"\ntwo",
            ]
            .concat(),
            b"test file 4 (not a long line).\ntwo".to_vec(),
        ),
        (
            [vec![b'z'; 32768].as_slice(), b"\ntest file 5 (long line)."].concat(),
            b"0000000000000000\ntest file 5 (not a long line).".to_vec(),
        ),
    ];
    for (from, to) in cases {
        assert_eq!(
            line_diff(
                &from,
                &to,
                LineDiffOptions::default(),
                DiffLimits::default()
            ),
            Err(DiffError::Binary)
        );
    }
}

#[test]
fn invert_change_count_and_admission_bounds() {
    let options = LineDiffOptions {
        invert: true,
        optimize: false,
        ..LineDiffOptions::default()
    };
    assert_eq!(
        line_diff(b"", b"a\nb", options, DiffLimits::default()).unwrap()[0],
        Edit {
            copy: 0,
            delete: 2,
            insert: 0
        }
    );
    let limits = DiffLimits {
        max_lines: 1,
        ..DiffLimits::default()
    };
    assert_eq!(
        line_diff(b"a\nb", b"", options, limits),
        Err(DiffError::Limit)
    );
    let limits = DiffLimits {
        max_edits: 1,
        ..DiffLimits::default()
    };
    assert_eq!(
        line_diff(b"a", b"b", options, limits),
        Err(DiffError::Limit)
    );
    let options = LineDiffOptions {
        not_too_big: true,
        optimize: false,
        ..LineDiffOptions::default()
    };
    assert_eq!(
        line_diff(&b"a\n".repeat(10001), b"", options, DiffLimits::default()),
        Err(DiffError::TooManyChanges)
    );
    assert_eq!(
        line_diff(&b"a\n".repeat(10000), b"", options, DiffLimits::default()).unwrap()[0].delete,
        10000
    );
}

#[test]
fn normalization_is_explicitly_callers_responsibility() {
    // A UTF-8 BOM is data at this layer. The full text_diff wrapper strips it.
    let edits = diff(b"\xef\xbb\xbfa\n", b"a\n");
    assert_eq!(
        edits[0],
        Edit {
            copy: 0,
            delete: 1,
            insert: 1
        }
    );
    // Invalid UTF-8 is likewise not a binary predicate in break_into_lines.
    assert_eq!(diff(b"\xff\n", b"\xff")[0].copy, 1);
}

#[test]
fn pinned_native_raw_goldens() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/diff/native-raw.json")).unwrap();
    let bytes = |s: &str| -> Vec<u8> {
        s.as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    };
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut options = LineDiffOptions::default();
        for flag in case["flags"].as_array().unwrap() {
            match flag.as_str().unwrap() {
                "--noopt" => options.optimize = false,
                "-w" => options.whitespace = Whitespace::IgnoreAll,
                "-Z" => options.whitespace = Whitespace::IgnoreEol,
                "--strip-trailing-cr" => options.strip_cr = true,
                "--invert" => options.invert = true,
                unknown => panic!("unknown native option {unknown}"),
            }
        }
        let actual = line_diff(
            &bytes(case["from_hex"].as_str().unwrap()),
            &bytes(case["to_hex"].as_str().unwrap()),
            options,
            DiffLimits::default(),
        );
        if let Some(error) = case["error"].as_str() {
            let expected = match error {
                "Binary" => DiffError::Binary,
                "WhitespaceOnly" => DiffError::WhitespaceOnly,
                unknown => panic!("unknown native error {unknown}"),
            };
            assert_eq!(actual, Err(expected), "{name}");
        } else {
            let expected: Vec<Edit> = case["triples"]
                .as_array()
                .unwrap()
                .iter()
                .map(|triple| Edit {
                    copy: triple[0].as_u64().unwrap() as usize,
                    delete: triple[1].as_u64().unwrap() as usize,
                    insert: triple[2].as_u64().unwrap() as usize,
                })
                .collect();
            assert_eq!(actual, Ok(expected), "{name}");
        }
    }
}

fn assert_complete_script(
    from: &[u8],
    to: &[u8],
    options: LineDiffOptions,
    expected: &[(usize, usize, usize)],
) {
    let edits = line_diff(from, to, options, DiffLimits::default()).unwrap();
    let expected: Vec<Edit> = expected
        .iter()
        .map(|&(copy, delete, insert)| Edit {
            copy,
            delete,
            insert,
        })
        .collect();
    assert_eq!(edits, expected);
    assert_eq!(edits.last(), Some(&Edit::default()));
    let line_count = |text: &[u8]| {
        text.iter().filter(|&&byte| byte == b'\n').count()
            + usize::from(!text.is_empty() && text.last() != Some(&b'\n'))
    };
    assert_eq!(
        edits
            .iter()
            .map(|edit| edit.copy + edit.delete)
            .sum::<usize>(),
        line_count(from)
    );
    assert_eq!(
        edits
            .iter()
            .map(|edit| edit.copy + edit.insert)
            .sum::<usize>(),
        line_count(to)
    );
}

#[test]
fn zero_edit_capacity_rejects_replacement() {
    let options = LineDiffOptions {
        optimize: false,
        ..LineDiffOptions::default()
    };
    assert_eq!(
        line_diff(
            b"a",
            b"b",
            options,
            DiffLimits {
                max_edits: 0,
                ..DiffLimits::default()
            }
        ),
        Err(DiffError::Limit)
    );
    assert_complete_script(b"a", b"b", options, &[(0, 1, 1), (0, 0, 0)]);
}

#[test]
fn ignore_all_matches_interior_spaces_in_both_directions() {
    let options = LineDiffOptions {
        whitespace: Whitespace::IgnoreAll,
        optimize: false,
        ..LineDiffOptions::default()
    };
    // A real second-line change keeps the result distinct from WhitespaceOnly.
    for (from, to) in [
        (b"ab\nold".as_slice(), b"a b\nnew".as_slice()),
        (b"a b\nnew".as_slice(), b"ab\nold".as_slice()),
    ] {
        assert_complete_script(from, to, options, &[(1, 1, 1), (0, 0, 0)]);
    }
}

#[test]
fn tiny_anchor_requires_a_unique_trimmed_from_line() {
    let options = LineDiffOptions {
        optimize: false,
        ..LineDiffOptions::default()
    };
    let to = b"t0\nt1\nt2\nt3\nanchor\nt5\nt6\nt7\nt8";
    // Equal trimmed width alone does not make an outside line a duplicate.
    assert_complete_script(
        b"f0\nfoobar\nf2\nf3\nanchor\nf5\nf6\nf7\nf8",
        to,
        options,
        &[(0, 4, 4), (1, 4, 4), (0, 0, 0)],
    );
    // Exact comparison distinguishes the leading space, while anchor validation
    // trims it and rejects the otherwise identical interior anchor as nonunique.
    assert_complete_script(
        b" anchor\nf1\nf2\nf3\nanchor\nf5\nf6\nf7\nf8",
        to,
        options,
        &[(0, 9, 9), (0, 0, 0)],
    );
}

#[test]
fn optimization_shifts_insertion_left_across_repeated_line() {
    let from = b"a\nlong\nend1";
    let to = b"a\nb\na\nlong\nend2";
    assert_complete_script(
        from,
        to,
        LineDiffOptions {
            optimize: false,
            ..LineDiffOptions::default()
        },
        &[(1, 0, 2), (1, 1, 1), (0, 0, 0)],
    );
    assert_complete_script(
        from,
        to,
        LineDiffOptions::default(),
        &[(0, 0, 2), (2, 1, 1), (0, 0, 0)],
    );
}

#[test]
fn exact_hash_collision_still_compares_line_bytes() {
    let mut from = [100, 1, 1, 1, 1, 1, 1];
    let mut to = [41, 1, 1, 1, 1, 1, 2];
    if cfg!(target_endian = "big") {
        from.reverse();
        to.reverse();
    }
    // The native-endian padded tails differ by P=2^48-59 (or P<<8 on
    // big endian), so their residues collide, but their line bytes differ.
    assert_complete_script(
        &from,
        &to,
        LineDiffOptions {
            optimize: false,
            ..LineDiffOptions::default()
        },
        &[(0, 1, 1), (0, 0, 0)],
    );
}

#[test]
fn exhaustive_fallback_extends_anchor_hidden_by_global_suffix_chains() {
    let mut from = b"X\nA\nB\nY".to_vec();
    let mut to = b"U\nA\nB\nV".to_vec();
    for _ in 0..67 {
        from.extend_from_slice(b"\nA\nB");
        to.extend_from_slice(b"\nA\nB");
    }
    // Suffix trimming leaves a 4-by-4 search while the global hash chains
    // retain 67 later copies of each anchor line, beyond all heuristic cutoffs.
    assert_complete_script(
        &from,
        &to,
        LineDiffOptions {
            optimize: false,
            ..LineDiffOptions::default()
        },
        &[(0, 1, 1), (2, 1, 1), (134, 0, 0), (0, 0, 0)],
    );
}

#[test]
fn ignore_all_hash_collision_still_compares_nonspace_bytes() {
    // Both ten-byte lines have native IgnoreAll hash 6553963404753993738.
    // Their distinct non-space bytes still require a replacement in either direction.
    let first = b"jqQAlDBAP4";
    let second = b"1*T_]Tut#0";
    let options = LineDiffOptions {
        whitespace: Whitespace::IgnoreAll,
        optimize: false,
        ..LineDiffOptions::default()
    };
    for (from, to) in [(first, second), (second, first)] {
        assert_complete_script(from, to, options, &[(0, 1, 1), (0, 0, 0)]);
    }
}
