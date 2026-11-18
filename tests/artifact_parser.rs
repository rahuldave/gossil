use gossil::artifact::{
    ArtifactError, ArtifactType, DateProvider, DateProviderError, ParseLimits, parse_artifact,
};
struct FixtureDates;
impl DateProvider for FixtureDates {
    async fn julianday(&self, token: Option<&[u8]>) -> Result<Option<f64>, DateProviderError> {
        // Grammar isolation only; production provider qualification is separate.
        Ok(token.filter(|t| !t.is_empty()).map(|_| 2460000.5))
    }
}
fn limits() -> ParseLimits {
    ParseLimits {
        input_bytes: 1_000_000,
        cards: 10_000,
        owned_bytes: 4_000_000,
        entries: 20_000,
    }
}
#[tokio::test]
async fn eight_preserved_valid_raw_baselines() {
    for (name, kind) in [
        ("manifest-valid", ArtifactType::Manifest),
        ("cluster", ArtifactType::Cluster),
        ("control", ArtifactType::Control),
        ("wiki", ArtifactType::Wiki),
        ("ticket", ArtifactType::Ticket),
        ("attachment", ArtifactType::Attachment),
        ("technote", ArtifactType::Technote),
        ("forum-root", ArtifactType::Forum),
    ] {
        let input = std::fs::read(format!("tests/fixtures/artifact/{name}.artifact")).unwrap();
        let parsed = parse_artifact(&input, limits(), &FixtureDates)
            .await
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(parsed.artifact_type(), kind, "{name}");
        assert_eq!(parsed.raw(), input, "{name}: untouched raw bytes");
    }
}
#[tokio::test]
async fn corrupt_checksum_precedes_decoding() {
    let input = include_bytes!("fixtures/artifact/checksum-corrupt.artifact");
    assert!(matches!(
        parse_artifact(input, limits(), &FixtureDates).await,
        Err(ArtifactError::Checksum)
    ));
}

#[tokio::test]
async fn preserved_fixture_acceptance_receipts() {
    for (name, accepted) in [
        ("manifest", false),
        ("cluster", true),
        ("control", true),
        ("wiki", true),
        ("ticket", true),
        ("attachment", true),
        ("technote", true),
        ("forum", false),
        ("manifest-missing-D", false),
        ("manifest-missing-Z", false),
        ("cluster-missing-M", false),
        ("cluster-missing-Z", false),
        ("control-missing-D", false),
        ("control-missing-T", false),
        ("control-missing-U", false),
        ("control-missing-Z", false),
        ("wiki-missing-D", false),
        ("wiki-missing-L", false),
        ("wiki-missing-U", false),
        ("wiki-missing-W", false),
        ("wiki-missing-Z", false),
        ("ticket-missing-D", false),
        ("ticket-missing-J", false),
        ("ticket-missing-K", false),
        ("ticket-missing-U", false),
        ("ticket-missing-Z", false),
        ("attachment-missing-A", false),
        ("attachment-missing-D", false),
        ("attachment-missing-Z", false),
        ("technote-missing-D", false),
        ("technote-missing-E", false),
        ("technote-missing-W", false),
        ("technote-missing-Z", false),
        ("forum-missing-D", false),
        ("forum-missing-U", false),
        ("forum-missing-W", false),
        ("forum-missing-Z", false),
        ("forum-root", true),
        ("forum-reply", true),
        ("forum-edit", false),
        ("forum-H-I", false),
        ("forum-two-P", false),
        ("attachment-delete", true),
        ("attachment-backslash", true),
        ("attachment-target-escaped-hash", true),
        ("attachment-target-NUL", true),
        ("delta-file-delete", true),
        ("technote-legacy-PN", true),
        ("ticket-append-empty", true),
        ("W-0", true),
        ("W-3suffix", true),
        ("W--1", true),
        ("W-x", true),
        ("W-4294967296", false),
        ("W-4294967297", false),
        ("W-18446744073709551616", false),
        ("W-999999999999999999999999", false),
        ("W-03", true),
        ("W-+3", false),
        ("W-multiline", true),
        ("W-NUL", true),
        ("W-invalid-utf8", true),
        ("F-reserved", false),
        ("F-reserved-nested", false),
        ("F-invalid-utf8", true),
        ("F-backslash", true),
        ("F-dotdot", false),
        ("F-absolute", false),
        ("F-trailing", false),
        ("F-double", false),
        ("F-NUL", false),
        ("F-suppression-sort", false),
        ("F-suppression-first", true),
        ("F-duplicate", false),
        ("F-reverse", false),
        ("cluster-reverse", false),
        ("cluster-duplicate", false),
        ("raw-NUL-comment", false),
        ("escaped-NUL-comment", true),
        ("hash-uppercase", true),
        ("R-syntax-only", true),
        ("repeated-empty-P", false),
        ("control-self", false),
        ("signed-PGP", true),
        ("signed-PGP-CRLF-header", true),
        ("signed-SSH", true),
        ("signed-SSH-CRLF-header", true),
        ("checksum-corrupt", false),
        ("checksum-uppercase", false),
        ("missing-LF", false),
        ("native-root-manifest", false),
        ("manifest-valid", true),
        ("native-root-trailer-removed", true),
    ] {
        let bytes = std::fs::read(format!("tests/fixtures/artifact/{name}.artifact")).unwrap();
        let result = parse_artifact(&bytes, limits(), &FixtureDates).await;
        assert_eq!(result.is_ok(), accepted, "{name}: {result:?}");
    }
}

fn artifact(prefix: &[u8]) -> Vec<u8> {
    let mut out = prefix.to_vec();
    out.extend_from_slice(b"Z ");
    for b in gossil::artifact_checksum::md5(prefix) {
        out.extend_from_slice(format!("{b:02x}").as_bytes());
    }
    out.push(b'\n');
    out
}
#[tokio::test]
async fn typed_fields_are_owned_and_c_prefixes_survive() {
    let bytes=artifact(b"C initial\\sempty\\0tail\nD 2026-11-18\nR d41d8cd98f00b204e9800998ecf8427e\nT *branch * trunk\nT *sym-trunk *\nU user\\sname\n");
    let parsed = parse_artifact(&bytes, limits(), &FixtureDates)
        .await
        .unwrap();
    assert_eq!(parsed.artifact_type(), ArtifactType::Manifest);
    let comment = parsed.comment().unwrap();
    assert_eq!(comment.bytes, b"initial empty\0tail");
    assert_eq!(comment.c_string_len, 13);
    assert_eq!(parsed.user().unwrap().bytes, b"user name");
    assert_eq!(
        parsed.repository_checksum(),
        Some(b"d41d8cd98f00b204e9800998ecf8427e".as_slice())
    );
    assert_eq!(parsed.tags().len(), 2);
    assert_eq!(parsed.tags()[0].name().bytes, b"*branch");
    assert_eq!(parsed.tags()[0].value().unwrap().bytes, b"trunk");
    assert_eq!(parsed.tags()[1].target(), None);
    assert_eq!(parsed.raw(), bytes);
}
#[tokio::test]
async fn explicit_limits_are_distinct_from_syntax_and_checksum() {
    let bytes = include_bytes!("fixtures/artifact/manifest-valid.artifact");
    for bounded in [
        ParseLimits {
            input_bytes: bytes.len() - 1,
            ..limits()
        },
        ParseLimits {
            cards: 0,
            ..limits()
        },
        ParseLimits {
            owned_bytes: bytes.len() - 1,
            ..limits()
        },
    ] {
        assert!(matches!(
            parse_artifact(bytes, bounded, &FixtureDates).await,
            Err(ArtifactError::ResourceLimit)
        ));
    }
    let tagged = include_bytes!("fixtures/artifact/control.artifact");
    assert!(matches!(
        parse_artifact(
            tagged,
            ParseLimits {
                entries: 0,
                ..limits()
            },
            &FixtureDates
        )
        .await,
        Err(ArtifactError::ResourceLimit)
    ));
    assert!(matches!(
        parse_artifact(b"", limits(), &FixtureDates).await,
        Err(ArtifactError::Syntax { .. })
    ));
}
// These totals are an explicit accounting ledger: raw input, encoded token
// lengths (not decoded lengths), copied payloads, and each retained list slot.
// They do not discover a passing limit by probing the parser.
#[tokio::test]
async fn aggregate_owned_budget_counts_escaped_tokens() {
    let bytes = artifact(b"C a\\sb\nD 2026-11-18\nU u\\sv\n");
    let exact = bytes.len() + 4 + 10 + 4;
    for owned_bytes in [bytes.len(), exact - 1] {
        assert!(matches!(
            parse_artifact(
                &bytes,
                ParseLimits {
                    owned_bytes,
                    ..limits()
                },
                &FixtureDates
            )
            .await,
            Err(ArtifactError::ResourceLimit)
        ));
    }
    let parsed = parse_artifact(
        &bytes,
        ParseLimits {
            owned_bytes: exact,
            ..limits()
        },
        &FixtureDates,
    )
    .await
    .unwrap();
    assert_eq!(parsed.comment().unwrap().bytes, b"a b");
    assert_eq!(parsed.user().unwrap().bytes, b"u v");
}

#[tokio::test]
async fn aggregate_owned_budget_counts_cumulative_list_slots() {
    let bytes = artifact(
        b"M 1111111111111111111111111111111111111111\nM 2222222222222222222222222222222222222222\n",
    );
    // Each M retains a 40-byte hash and one Vec<u8> list slot.
    let one_entry = 40 + std::mem::size_of::<Vec<u8>>();
    let exact = bytes.len() + 2 * one_entry;
    for owned_bytes in [bytes.len(), bytes.len() + one_entry, exact - 1] {
        assert!(matches!(
            parse_artifact(
                &bytes,
                ParseLimits {
                    owned_bytes,
                    ..limits()
                },
                &FixtureDates
            )
            .await,
            Err(ArtifactError::ResourceLimit)
        ));
    }
    let parsed = parse_artifact(
        &bytes,
        ParseLimits {
            owned_bytes: exact,
            ..limits()
        },
        &FixtureDates,
    )
    .await
    .unwrap();
    assert_eq!(parsed.cluster_children(), &[vec![b'1'; 40], vec![b'2'; 40]]);
}

#[tokio::test]
async fn aggregate_owned_budget_counts_body_after_tokens() {
    let bytes = artifact(b"D 2026-11-18\nL Title\nU user\nW 3\nabc\n");
    let before_body = bytes.len() + 10 + 5 + 4;
    let exact = before_body + 3;
    for owned_bytes in [bytes.len(), before_body, exact - 1] {
        assert!(matches!(
            parse_artifact(
                &bytes,
                ParseLimits {
                    owned_bytes,
                    ..limits()
                },
                &FixtureDates
            )
            .await,
            Err(ArtifactError::ResourceLimit)
        ));
    }
    let parsed = parse_artifact(
        &bytes,
        ParseLimits {
            owned_bytes: exact,
            ..limits()
        },
        &FixtureDates,
    )
    .await
    .unwrap();
    assert_eq!(parsed.body(), Some(b"abc".as_slice()));
}

struct BrokenDates;
impl DateProvider for BrokenDates {
    async fn julianday(&self, _: Option<&[u8]>) -> Result<Option<f64>, DateProviderError> {
        Err(DateProviderError(Box::new(std::io::Error::other(
            "selected provider failed",
        ))))
    }
}
struct NullDates;
impl DateProvider for NullDates {
    async fn julianday(&self, _: Option<&[u8]>) -> Result<Option<f64>, DateProviderError> {
        Ok(None)
    }
}
#[tokio::test]
async fn date_failures_preserve_provider_cause() {
    use std::error::Error;
    let bytes = include_bytes!("fixtures/artifact/manifest-valid.artifact");
    let error = parse_artifact(bytes, limits(), &BrokenDates)
        .await
        .unwrap_err();
    assert!(matches!(error, ArtifactError::Provider(_)));
    assert_eq!(
        error.source().unwrap().source().unwrap().to_string(),
        "selected provider failed"
    );
    assert!(matches!(
        parse_artifact(bytes, limits(), &NullDates).await,
        Err(ArtifactError::Syntax { .. })
    ));
}
#[tokio::test]
async fn raw_nul_depends_on_each_token_consumer() {
    let cases:[(&[u8],bool);9]=[
        (b"C comment\0more\nD date\n",false),
        (b"D date\nJ status\0value\nK aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nU u\n",true),
        (b"D date\nJ status value\0tail\nK aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nU u\n",false),
        (b"C c\nD date\nP aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\0bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n",true),
        (b"D date\nE date\0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nW 0\n\n",true),
        (b"D date\nL page\nU u\nW 0\0\n",false),
        (b"C c\nD date\nT +tag\0*\0value\n",true),
        (b"C c\nD date\nF file\0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\0x\0prior\n",true),
        (b"A dir/file\0page\0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nD date\n",true),
    ];
    for (prefix, accepted) in cases {
        let bytes = artifact(prefix);
        let parsed = parse_artifact(&bytes, limits(), &FixtureDates).await;
        assert_eq!(parsed.is_ok(), accepted, "prefix={prefix:?}: {parsed:?}");
    }
}
#[tokio::test]
async fn modular_w_scanner_and_exact_body_boundaries() {
    // Source-derived synthetic cases; these are not native observations.
    for (size, body, accepted) in [
        ("8589934592", b"".as_slice(), false),
        ("4294967296", b"", false),
        ("3suffix", b"abc", true),
        ("x", b"", true),
        ("+3", b"abc", false),
        ("0", b"", true),
    ] {
        let mut prefix = b"D date\nL page\nU u\nW ".to_vec();
        prefix.extend_from_slice(size.as_bytes());
        prefix.push(b'\n');
        prefix.extend_from_slice(body);
        prefix.push(b'\n');
        let bytes = artifact(&prefix);
        let result = parse_artifact(&bytes, limits(), &FixtureDates).await;
        assert_eq!(result.is_ok(), accepted, "{size}: {result:?}");
        if let Ok(parsed) = result {
            assert_eq!(parsed.body(), Some(body));
        }
    }
}
#[tokio::test]
async fn repeated_cards_and_native_inference_are_not_blanket_singletons() {
    for (prefix, accepted) in [
        (b"C c\nD date\nP\nP\n".as_slice(), true),
        (b"D date\nP\nP\n", false),
        (b"C c\nC c\nD date\n", false),
        (b"D date\nU\nU\n", false),
        (b"D date\nH title\nU u\nW 0\n\n", true),
        (b"D date\nU u\nW 0\n\n", false),
        (
            b"C c\nD date\nT +tag *\nT +tag aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
            true,
        ),
        (
            b"C c\nD date\nT +tag aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nT +tag *\n",
            false,
        ),
    ] {
        let bytes = artifact(prefix);
        let result = parse_artifact(&bytes, limits(), &FixtureDates).await;
        assert_eq!(result.is_ok(), accepted, "{prefix:?}: {result:?}");
    }
}

struct SelectedLibsqlDates(libsql::Connection);
impl DateProvider for SelectedLibsqlDates {
    async fn julianday(&self, token: Option<&[u8]>) -> Result<Option<f64>, DateProviderError> {
        async {
            let value = token.map_or(libsql::Value::Null, |t| libsql::Value::Blob(t.to_vec()));
            let mut rows = self
                .0
                .query("SELECT julianday(CAST(?1 AS TEXT))", [value])
                .await?;
            let row = rows
                .next()
                .await?
                .ok_or_else(|| std::io::Error::other("no date row"))?;
            match row.get_value(0)? {
                libsql::Value::Null => Ok(None),
                libsql::Value::Real(day) => Ok(Some(day)),
                _ => Err(Box::<dyn std::error::Error + Send + Sync>::from(
                    std::io::Error::other("unexpected date type"),
                )),
            }
        }
        .await
        .map_err(DateProviderError)
    }
}
#[tokio::test]
async fn actual_selected_libsql_provider_parses_all_eight_baselines() {
    let db = libsql::Builder::new_local(":memory:")
        .build()
        .await
        .unwrap();
    let dates = SelectedLibsqlDates(db.connect().unwrap());
    for name in [
        "manifest-valid",
        "cluster",
        "control",
        "wiki",
        "ticket",
        "attachment",
        "technote",
        "forum-root",
    ] {
        let bytes = std::fs::read(format!("tests/fixtures/artifact/{name}.artifact")).unwrap();
        let result = parse_artifact(&bytes, limits(), &dates)
            .await
            .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        if name != "cluster" {
            assert!(result.date().unwrap() > 0.0);
            assert!(result.date_token().is_some());
        }
        if name == "technote" {
            assert!(result.event_date().unwrap() > 0.0);
            assert!(result.event_date_token().is_some());
        }
    }
    for token in [
        b"2026-11-18".as_slice(),
        b"2000-01-01T12:00:00.125Z",
        b"2000-01-01 14:00:00+02:00",
        b"2451545",
        b"12:34:56",
        b"invalid",
        b"",
        b"\xff",
    ] {
        let date = dates.julianday(Some(token)).await.unwrap();
        let mut prefix = b"C c\nD ".to_vec();
        prefix.extend_from_slice(token);
        prefix.push(b'\n');
        let bytes = artifact(&prefix);
        let result = parse_artifact(&bytes, limits(), &dates).await;
        // Literal space splits tokens; those forms belong in provider tests only.
        if !token.contains(&b' ') {
            assert_eq!(
                result.is_ok(),
                date.is_some_and(|d| d > 0.0),
                "{token:?}: {result:?}"
            );
        }
    }
}

#[tokio::test]
async fn actual_provider_d_e_preserve_all_pinned_binary64_results() {
    let db = libsql::Builder::new_local(":memory:")
        .build()
        .await
        .unwrap();
    let dates = SelectedLibsqlDates(db.connect().unwrap());
    for (corpus, count) in [
        (include_str!("fixtures/artifact/dates/main.json"), 489),
        (include_str!("fixtures/artifact/dates/threshold.json"), 30),
    ] {
        let cases: serde_json::Value = serde_json::from_str(corpus).unwrap();
        assert_eq!(cases.as_array().unwrap().len(), count);
        for case in cases.as_array().unwrap() {
            let token = case["token"].as_str().unwrap().as_bytes();
            let expected = &case["expected"][0];
            let observed = dates.julianday(Some(token)).await.unwrap();
            let (kind, bits) = observed.map_or((5, 0), |day| (2, day.to_bits()));
            assert_eq!(
                kind,
                expected["type"].as_u64().unwrap(),
                "provider storage class {token:?}"
            );
            for card in [b'D', b'E'] {
                let mut prefix = if card == b'D' {
                    b"C c\nD ".to_vec()
                } else {
                    b"D 2024-01-01\nE ".to_vec()
                };
                prefix.extend_from_slice(token);
                if card == b'E' {
                    prefix.extend_from_slice(b" aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nW 0\n\n");
                } else {
                    prefix.push(b'\n');
                }
                let result = parse_artifact(&artifact(&prefix), limits(), &dates).await;
                let admitted = observed.is_some_and(|day| day > 0.0)
                    && !token.iter().any(|b| matches!(b, b' ' | b'\n' | 0));
                assert_eq!(result.is_ok(), admitted, "{card} {token:?}: {result:?}");
                if let Ok(parsed) = result {
                    let got = if card == b'D' {
                        parsed.date()
                    } else {
                        parsed.event_date()
                    };
                    assert_eq!(
                        format!("{:016x}", got.unwrap().to_bits()),
                        expected["bits"].as_str().unwrap(),
                        "parsed bits {card} {token:?}"
                    );
                }
            }
            assert_eq!(
                format!("{bits:016x}"),
                expected["bits"].as_str().unwrap(),
                "provider bits {token:?}"
            );
        }
    }
}

fn records(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        let start = pos;
        let end = pos + bytes[pos..].iter().position(|b| *b == b'\n').unwrap() + 1;
        pos = end;
        if bytes[start] == b'W' {
            let size = std::str::from_utf8(&bytes[start + 2..end - 1])
                .unwrap()
                .parse::<usize>()
                .unwrap();
            pos += size + 1;
        }
        out.push(bytes[start..pos].to_vec());
    }
    out
}
fn sorted_artifact(mut cards: Vec<Vec<u8>>) -> Vec<u8> {
    cards.sort_by_key(|c| c[0]);
    let prefix: Vec<u8> = cards.into_iter().flatten().collect();
    artifact(&prefix)
}
fn baseline(name: &str) -> Vec<Vec<u8>> {
    let name = match name {
        "manifest" => "manifest-valid",
        "forum" => "forum-root",
        other => other,
    };
    let bytes = std::fs::read(format!("tests/fixtures/artifact/{name}.artifact")).unwrap();
    records(&bytes)
        .into_iter()
        .filter(|c| c[0] != b'Z')
        .collect()
}
fn kind(name: &str) -> ArtifactType {
    match name {
        "manifest" => ArtifactType::Manifest,
        "cluster" => ArtifactType::Cluster,
        "control" => ArtifactType::Control,
        "wiki" => ArtifactType::Wiki,
        "ticket" => ArtifactType::Ticket,
        "attachment" => ArtifactType::Attachment,
        "technote" => ArtifactType::Technote,
        "forum" => ArtifactType::Forum,
        _ => panic!("unknown kind"),
    }
}
#[tokio::test]
async fn all_208_matrix_insertions_and_29_required_deletions() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/artifact/card-matrix.json")).unwrap();
    let (mut edges, mut required, mut optional, mut changed_type) = (0, 0, 0, 0);
    for row in rows
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r.get("recipe").is_some())
    {
        let name = row["type"].as_str().unwrap();
        let c = row["card"].as_str().unwrap().as_bytes()[0];
        let expected = kind(name);
        let mut cards = baseline(name);
        cards.retain(|r| r[0] != c);
        if c != b'Z' {
            let recipe = if name == "technote" && c == b'T' {
                "T +tag *"
            } else {
                row["recipe"].as_str().unwrap()
            };
            let mut record = recipe.replace("\\n", "\n").into_bytes();
            record.push(b'\n');
            cards.push(record);
        }
        if name == "forum" && c == b'I' {
            cards.retain(|r| r[0] != b'H');
        }
        let result = parse_artifact(&sorted_artifact(cards), limits(), &FixtureDates).await;
        let allowed = row["allowed"].as_bool().unwrap();
        if allowed {
            assert_eq!(
                result
                    .as_ref()
                    .map(|p| p.artifact_type())
                    .unwrap_or_else(|e| panic!("{}: {e:?}", row["id"])),
                expected,
                "{}",
                row["id"]
            );
        } else if let Ok(parsed) = result {
            assert_ne!(
                parsed.artifact_type(),
                expected,
                "forbidden same-type {}",
                row["id"]
            );
            changed_type += 1;
        }
        if row["required"].as_bool().unwrap() {
            let mut cards = baseline(name);
            cards.retain(|r| r[0] != c);
            let bytes = if c == b'Z' {
                cards.into_iter().flatten().collect()
            } else {
                sorted_artifact(cards)
            };
            assert!(
                parse_artifact(&bytes, limits(), &FixtureDates)
                    .await
                    .is_err(),
                "required deletion {}",
                row["id"]
            );
            required += 1;
        } else if allowed {
            let mut cards = baseline(name);
            cards.retain(|r| r[0] != c);
            if name == "manifest" && c == b'C' {
                cards.push(b"R aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n".to_vec());
            }
            if name == "forum" && c == b'H' {
                cards.push(b"G aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n".to_vec());
            }
            let parsed = parse_artifact(&sorted_artifact(cards), limits(), &FixtureDates)
                .await
                .unwrap_or_else(|e| panic!("optional absence {} {e:?}", row["id"]));
            assert_eq!(
                parsed.artifact_type(),
                expected,
                "optional absence {}",
                row["id"]
            );
            optional += 1;
        }
        edges += 1;
    }
    assert_eq!((edges, required, optional), (208, 29, 25));
    // Source selectors make a few forbidden-original-type mutations valid as another type.
    assert!(changed_type > 0);
}
#[tokio::test]
async fn all_21_repeat_branches_reject_or_accumulate_per_source() {
    let matrix: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/artifact/card-matrix.json")).unwrap();
    for c in b"ABCDEFGHIJKLMNPQRTUWZ" {
        let row = matrix
            .as_array()
            .unwrap()
            .iter()
            .find(|r| {
                r.get("recipe").is_some()
                    && r["card"].as_str().unwrap().as_bytes()[0] == *c
                    && r["allowed"] == true
            })
            .unwrap();
        let name = row["type"].as_str().unwrap();
        let mut cards = baseline(name);
        cards.retain(|r| r[0] != *c);
        let record = if *c == b'Z' {
            b"Z aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n".to_vec()
        } else {
            let mut v = row["recipe"]
                .as_str()
                .unwrap()
                .replace("\\n", "\n")
                .into_bytes();
            v.push(b'\n');
            v
        };
        if *c == b'I' {
            cards.retain(|r| r[0] != b'H');
        }
        let mut single = cards.clone();
        single.push(record.clone());
        assert!(
            parse_artifact(&sorted_artifact(single.clone()), limits(), &FixtureDates)
                .await
                .is_ok(),
            "single {}",
            *c as char
        );
        let mut zero = cards.clone();
        zero.push(vec![*c, b'\n']);
        assert_eq!(
            parse_artifact(&sorted_artifact(zero), limits(), &FixtureDates)
                .await
                .is_ok(),
            matches!(c, b'P' | b'U'),
            "zero {}",
            *c as char
        );
        let mut extra = record.clone();
        let at = extra.iter().position(|b| *b == b'\n').unwrap();
        extra.splice(at..at, b" extra extra extra extra extra".iter().copied());
        let mut extra_cards = cards.clone();
        extra_cards.push(extra);
        assert!(
            parse_artifact(&sorted_artifact(extra_cards), limits(), &FixtureDates)
                .await
                .is_err(),
            "extra {}",
            *c as char
        );
        let mut reversed = b"Z aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n".to_vec();
        single.sort_by_key(|r| r[0]);
        for r in single {
            reversed.extend_from_slice(&r);
        }
        assert!(
            parse_artifact(&artifact(&reversed), limits(), &FixtureDates)
                .await
                .is_err(),
            "reverse {}",
            *c as char
        );
        let differing = match c {
            b'F' => b"F filez aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n".to_vec(),
            b'J' => b"J fieldz value\n".to_vec(),
            b'M' => b"M bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n".to_vec(),
            b'T' => b"T +tagz aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n".to_vec(),
            b'P' => b"P bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n".to_vec(),
            b'Q' => b"Q -bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n".to_vec(),
            b'Z' => b"Z bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n".to_vec(),
            _ => record.clone(),
        };
        let mut two = cards.clone();
        two.push(record.clone());
        two.push(differing);
        assert_eq!(
            parse_artifact(&sorted_artifact(two), limits(), &FixtureDates)
                .await
                .is_ok(),
            matches!(c, b'F' | b'J' | b'M' | b'T' | b'P' | b'Q' | b'Z'),
            "differing {}",
            *c as char
        );
        cards.push(record.clone());
        cards.push(record);
        let result = parse_artifact(&sorted_artifact(cards), limits(), &FixtureDates).await;
        assert_eq!(
            result.is_ok(),
            matches!(c, b'P' | b'Q' | b'Z'),
            "repeat {}: {result:?}",
            *c as char
        );
    }
}

#[tokio::test]
async fn escape_and_hash_edge_families() {
    let tokens: [&[u8]; 12] = [
        b"a\\sb", b"a\\nb", b"a\\rb", b"a\\tb", b"a\\vb", b"a\\fb", b"a\\0b", b"a\\\\b", b"a\\qb",
        b"a\\", b"\xff", b"a\t\rb",
    ];
    for token in tokens {
        let want = gossil::token_codec::defossilize(token).unwrap();
        for card in [b'C', b'H', b'N', b'U'] {
            let mut cards = if card == b'H' {
                baseline("forum")
            } else {
                baseline("manifest")
            };
            // N needs a wiki type; preserve all required wiki cards.
            if card == b'N' {
                cards = baseline("wiki");
            }
            cards.retain(|r| r[0] != card);
            let mut record = vec![card, b' '];
            record.extend_from_slice(token);
            record.push(b'\n');
            cards.push(record);
            let parsed = parse_artifact(&sorted_artifact(cards), limits(), &FixtureDates)
                .await
                .unwrap();
            let got = match card {
                b'C' => parsed.comment(),
                b'H' => parsed.thread_title(),
                b'N' => parsed.mimetype(),
                _ => parsed.user(),
            }
            .unwrap();
            assert_eq!(got, &want, "{card} {token:?}");
        }
        let mut cards = baseline("ticket");
        cards.retain(|r| r[0] != b'J');
        let mut j = b"J +append ".to_vec();
        j.extend_from_slice(token);
        j.push(b'\n');
        cards.push(j);
        let parsed = parse_artifact(&sorted_artifact(cards), limits(), &FixtureDates)
            .await
            .unwrap();
        assert_eq!(parsed.ticket_fields()[0].value(), &want);
        assert_eq!(parsed.ticket_fields()[0].name(), b"+append");
    }
    for len in [0, 1, 31, 32, 33, 39, 40, 41, 63, 64, 65] {
        for byte in [b'a', b'A', b'g'] {
            let token = vec![byte; len];
            let mut cards = baseline("manifest");
            let mut b = b"B ".to_vec();
            b.extend_from_slice(&token);
            b.push(b'\n');
            cards.push(b);
            let parsed = parse_artifact(&sorted_artifact(cards), limits(), &FixtureDates).await;
            assert_eq!(
                parsed.is_ok(),
                matches!(len, 40 | 64) && byte != b'g',
                "hash length {len} byte {byte}"
            );
        }
    }
    for offset in 0..=64 {
        let mut token = vec![b'a'; 64];
        token.insert(offset, 0);
        let mut cards = baseline("manifest");
        let mut b = b"B ".to_vec();
        b.extend_from_slice(&token);
        b.push(b'\n');
        cards.push(b);
        assert!(
            parse_artifact(&sorted_artifact(cards), limits(), &FixtureDates)
                .await
                .is_err(),
            "raw hash NUL {offset}"
        );
    }
}
#[tokio::test]
async fn attachment_endpoint_wiki_and_source_edges() {
    let targets: [(&[u8], bool); 15] = [
        (b"a", true),
        (b"", false),
        (&[b'a'; 100], true),
        (&[b'a'; 101], false),
        (b"\\sa", false),
        (b"a\\s", false),
        (b"a\\s\\sb", false),
        (b"a\\tb", false),
        (b"\xff", true),
        (&[b'a'; 39], true),
        (&[b'a'; 41], true),
        (b"a\\sb", true),
        (b"\\0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", false),
        (b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\\0", true),
        (b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\\q", true),
    ];
    for (target, accepted) in targets {
        for source in [
            None,
            Some(vec![b'a'; 40]),
            Some(vec![b'A'; 64]),
            Some(b"bad".to_vec()),
        ] {
            let mut a = b"A dir/name ".to_vec();
            a.extend_from_slice(target);
            if let Some(source) = source.as_ref() {
                a.push(b' ');
                a.extend_from_slice(source);
            }
            a.push(b'\n');
            let result = parse_artifact(
                &sorted_artifact(vec![a, b"D date\n".to_vec()]),
                limits(),
                &FixtureDates,
            )
            .await;
            assert_eq!(
                result.is_ok(),
                accepted && source.as_ref().is_none_or(|s| s.len() != 3),
                "target {target:?} source {source:?}: {result:?}"
            );
            if let Ok(parsed) = result {
                assert_eq!(&parsed.attachment().unwrap().name().bytes, b"name");
                let _ = parsed.attachment().unwrap().target();
                let _ = parsed.attachment().unwrap().source();
            }
        }
    }
    for (path, want) in [
        (b"dir/name".as_slice(), b"name".as_slice()),
        (
            b"dir\\\\name",
            if cfg!(windows) { b"name" } else { b"dir\\name" },
        ),
        (
            b"mixed/slash\\\\name",
            if cfg!(windows) {
                b"name"
            } else {
                b"slash\\name"
            },
        ),
    ] {
        let mut a = b"A ".to_vec();
        a.extend_from_slice(path);
        a.extend_from_slice(b" page\n");
        let parsed = parse_artifact(
            &sorted_artifact(vec![a, b"D date\n".to_vec()]),
            limits(),
            &FixtureDates,
        )
        .await
        .unwrap();
        assert_eq!(parsed.attachment().unwrap().name().bytes, want);
    }
}
#[tokio::test]
async fn w_modular_threshold_and_body_edges() {
    for (size, body, message) in [
        ("00", b"".as_slice(), None),
        ("03", b"abc", None),
        ("-1", b"", None),
        ("4294967295", b"", Some("not enough W content")),
        ("5000000000", b"", Some("not enough W content")),
        ("8589934591", b"", Some("not enough W content")),
        ("4294967296", b"", Some("size overflow on W-card")),
        ("4294967297", b"", Some("size overflow on W-card")),
        ("18446744073709551616", b"", Some("size overflow on W-card")),
        (
            "999999999999999999999999",
            b"",
            Some("size overflow on W-card"),
        ),
        ("3", b"ab", Some("W content has no LF")),
        ("2", b"abc", Some("W content has no LF")),
        ("3", b"a\0\xff", None),
        ("3", b"a\nb", None),
    ] {
        let mut w = b"W ".to_vec();
        w.extend_from_slice(size.as_bytes());
        w.push(b'\n');
        w.extend_from_slice(body);
        w.push(b'\n');
        let result = parse_artifact(
            &sorted_artifact(vec![
                b"D date\n".to_vec(),
                b"L page\n".to_vec(),
                b"U u\n".to_vec(),
                w,
            ]),
            limits(),
            &FixtureDates,
        )
        .await;
        match message {
            None => assert!(result.is_ok(), "W {size}: {result:?}"),
            Some(want) => assert!(
                matches!(result,Err(ArtifactError::Syntax{message,..}) if message==want),
                "W {size}: {result:?}"
            ),
        }
    }
    for prefix in [
        b"D date\nL page\nU u\nW 0 extra\n\n".as_slice(),
        b"D date\nL page\nU u\nW 0\nx",
        b"D date\nL page\nU u\nW 1\n",
    ] {
        assert!(
            parse_artifact(&artifact(prefix), limits(), &FixtureDates)
                .await
                .is_err()
        );
    }
}
#[tokio::test]
async fn signature_and_checksum_envelopes_follow_source_scanning() {
    let plain = include_bytes!("fixtures/artifact/manifest-valid.artifact");
    for opener in ["PGP", "SSH"] {
        for closer in ["PGP", "SSH"] {
            for blank in ["\n\n", "\r\n\r\n"] {
                let mut bytes =
                    format!("-----BEGIN {opener} SIGNED MESSAGE-----{blank}").into_bytes();
                bytes.extend_from_slice(plain);
                bytes.extend_from_slice(
                    format!("-----BEGIN {closer} SIGNATURE-----\nanything\n").as_bytes(),
                );
                assert!(
                    parse_artifact(&bytes, limits(), &FixtureDates)
                        .await
                        .is_ok()
                );
            }
        }
    }
    for prefix in [
        b"-----BEGIN PGP SIGNED MESSAGE-----\n".as_slice(),
        b"-----BEGIN WRONG SIGNED MESSAGE-----\n\n",
    ] {
        let mut bytes = prefix.to_vec();
        bytes.extend_from_slice(plain);
        assert!(
            parse_artifact(&bytes, limits(), &FixtureDates)
                .await
                .is_err()
        );
    }
    let mut no_tail = b"-----BEGIN SSH SIGNED MESSAGE-----\n\n".to_vec();
    no_tail.extend_from_slice(plain);
    assert!(
        parse_artifact(&no_tail, limits(), &FixtureDates)
            .await
            .is_ok()
    );
    let mut not_line = no_tail.clone();
    not_line.extend_from_slice(b"x-----BEGIN SSH SIGNATURE-----\n");
    assert!(
        parse_artifact(&not_line, limits(), &FixtureDates)
            .await
            .is_err()
    );
    let mut no_lf = no_tail.clone();
    no_lf.pop();
    assert!(
        parse_artifact(&no_lf, limits(), &FixtureDates)
            .await
            .is_err()
    );
    let mut multiple = no_tail.clone();
    multiple.extend_from_slice(
        b"-----BEGIN PGP SIGNATURE-----\nanything\n-----BEGIN SSH SIGNATURE-----\nanything\n",
    );
    assert!(
        parse_artifact(&multiple, limits(), &FixtureDates)
            .await
            .is_err()
    );
    let mut duplicate = b"C c\nD date\nZ aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n".to_vec();
    let bytes = artifact(&duplicate);
    assert!(
        parse_artifact(&bytes, limits(), &FixtureDates)
            .await
            .is_ok()
    );
    duplicate.extend_from_slice(b"Z bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n");
    assert!(
        parse_artifact(&artifact(&duplicate), limits(), &FixtureDates)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn order_paths_forum_and_remaining_raw_nul_controls() {
    for prefix in [
        b"D date\nG aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nU u\nW 0\n\n".as_slice(),
        b"D date\nI aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nU u\nW 0\n\n",
    ] {
        assert_eq!(
            parse_artifact(&artifact(prefix), limits(), &FixtureDates)
                .await
                .unwrap()
                .artifact_type(),
            ArtifactType::Forum
        );
    }
    let parsed=parse_artifact(&artifact(b"B aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nC c\nD date\nF file aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa opaque old/name\nP aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nQ +aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n"),limits(),&FixtureDates).await.unwrap();
    assert_eq!(
        parsed.baseline(),
        Some(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".as_slice())
    );
    assert_eq!(parsed.files()[0].name().bytes, b"file");
    assert_eq!(parsed.files()[0].hash(), parsed.baseline());
    assert_eq!(parsed.files()[0].permissions(), Some(b"opaque".as_slice()));
    assert_eq!(parsed.files()[0].prior_name().unwrap().bytes, b"old/name");
    assert_eq!(parsed.parents().len(), 1);
    assert_eq!(
        parsed.cherrypicks()[0].target(),
        b"+aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(
        parsed.cherrypicks()[0].base(),
        Some(b"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".as_slice())
    );
    for prefix in [
        b"C c\nD date\nF file aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa x ../old\n".as_slice(),
        b"C c\nD date\nP\nN text/plain\n",
        b"C c\nD date\nT + *\n",
        b"C c\nD date\nT +aaa *\n",
        b"\0C c\nD date\n",
        b"C\0 c\nD date\n",
        b"C c\0\nD date\n",
        b"D date\nU u\0more\n",
    ] {
        assert!(
            parse_artifact(&artifact(prefix), limits(), &FixtureDates)
                .await
                .is_err(),
            "{prefix:?}"
        );
    }
    let mut trailing = artifact(b"C c\nD date\n");
    trailing.extend_from_slice(b"\0\n");
    // Corrected source-derived expectation: the zero sentinel consumes its LF,
    // exits the card loop and leaves an exhausted cursor. Historical rejection
    // remains preserved in the original frozen candidate and receipts.
    assert!(
        parse_artifact(&trailing, limits(), &FixtureDates)
            .await
            .is_ok()
    );
    let crlf = artifact(b"C c\r\nD date\r\n");
    assert!(parse_artifact(&crlf, limits(), &FixtureDates).await.is_ok());
    let mut zcrlf = artifact(b"C c\nD date\n");
    zcrlf.pop();
    zcrlf.extend_from_slice(b"\r\n");
    assert!(
        parse_artifact(&zcrlf, limits(), &FixtureDates)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn actual_provider_now_stays_within_same_engine_clock_interval() {
    let db = libsql::Builder::new_local(":memory:")
        .build()
        .await
        .unwrap();
    let dates = SelectedLibsqlDates(db.connect().unwrap());
    let before = dates.julianday(Some(b"now")).await.unwrap().unwrap();
    let parsed = parse_artifact(&artifact(b"C c\nD now\n"), limits(), &dates)
        .await
        .unwrap();
    let after = dates.julianday(Some(b"now")).await.unwrap().unwrap();
    assert!(before <= parsed.date().unwrap() && parsed.date().unwrap() <= after);
}

#[tokio::test]
async fn empty_tokens_mixed_hashes_and_shortened_attachment_targets() {
    for (prefix, accepted) in [
        (b"C \nD date\n".as_slice(), true),
        (b"C  \nD date\n", false),
    ] {
        assert_eq!(
            parse_artifact(&artifact(prefix), limits(), &FixtureDates)
                .await
                .is_ok(),
            accepted
        );
    }
    let parsed = parse_artifact(
        &artifact(b"B aAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaAaA\nC c\nD date\n"),
        limits(),
        &FixtureDates,
    )
    .await
    .unwrap();
    assert_eq!(parsed.baseline().unwrap().len(), 40);
    let mut shortened = b"\\q".repeat(24);
    shortened.extend_from_slice(&[b'a'; 16]);
    assert_eq!(shortened.len(), 64);
    for target in [vec![b'a'; 40], vec![b'A'; 64], shortened] {
        let mut a = b"A dir/name ".to_vec();
        a.extend_from_slice(&target);
        a.extend_from_slice(b"\nD date\n");
        let parsed = parse_artifact(&artifact(&a), limits(), &FixtureDates)
            .await
            .unwrap();
        assert_eq!(
            parsed.attachment().unwrap().target().bytes,
            gossil::token_codec::defossilize(&target).unwrap().bytes
        );
    }
}

#[tokio::test]
async fn typed_fields_match_preserved_artifact_payloads() {
    async fn parsed(name: &str) -> gossil::artifact::ParsedArtifact {
        let bytes = std::fs::read(format!("tests/fixtures/artifact/{name}.artifact")).unwrap();
        parse_artifact(&bytes, limits(), &FixtureDates)
            .await
            .unwrap()
    }
    let cluster = parsed("cluster").await;
    assert_eq!(
        cluster.cluster_children(),
        [b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_vec()]
    );
    let wiki = parsed("wiki").await;
    assert_eq!(wiki.wiki_title().unwrap().bytes, b"Title");
    assert_eq!(wiki.body(), Some(b"abc".as_slice()));
    assert!(wiki.mimetype().is_none());
    let ticket = parsed("ticket").await;
    assert_eq!(
        ticket.ticket_id(),
        Some(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".as_slice())
    );
    assert_eq!(ticket.ticket_fields()[0].name(), b"title");
    assert_eq!(ticket.ticket_fields()[0].value().bytes, b"value");
    let reply = parsed("forum-reply").await;
    assert_eq!(
        reply.thread_root(),
        Some(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".as_slice())
    );
    assert_eq!(
        reply.in_reply_to(),
        Some(b"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".as_slice())
    );
    assert!(reply.thread_title().is_none());
    let note = parsed("technote-legacy-PN").await;
    assert_eq!(
        note.event_id(),
        Some(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".as_slice())
    );
    assert_eq!(note.mimetype().unwrap().bytes, b"text/plain");
    assert_eq!(note.event_date_token(), Some(b"2024-01-01".as_slice()));
    for (bytes, expected) in [
        (b"".as_slice(), "card"),
        (
            include_bytes!("fixtures/artifact/checksum-corrupt.artifact").as_slice(),
            "incorrect Z-card checksum",
        ),
    ] {
        let error = parse_artifact(bytes, limits(), &FixtureDates)
            .await
            .unwrap_err();
        assert!(error.to_string().starts_with(expected));
    }
    let error = parse_artifact(
        include_bytes!("fixtures/artifact/manifest-valid.artifact"),
        ParseLimits {
            input_bytes: 0,
            ..limits()
        },
        &FixtureDates,
    )
    .await
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "artifact resource limit or allocation failure"
    );
    let error = parse_artifact(
        include_bytes!("fixtures/artifact/manifest-valid.artifact"),
        limits(),
        &BrokenDates,
    )
    .await
    .unwrap_err();
    assert_eq!(error.to_string(), "date provider: selected provider failed");
}

#[tokio::test]
async fn source_derived_terminal_zero_card_stops_after_consuming_its_delimiter() {
    // Source-derived predictions from manifest.c:403-416,552,1077;
    // no new native execution or observed-native acceptance is claimed.
    let original = include_bytes!("fixtures/artifact/manifest-valid.artifact");
    for (tail, accepted) in [
        (b"\0\n".as_slice(), true),
        (b"\0 \n", false),
        (b"\0 ", false),
        (b"\0\nextra\n", false),
        (b"\0\n\0\n", false),
        (b"\0\t\n", false),
        (b"\0\r\n", false),
        (b"\x01\n", false),
    ] {
        let mut bytes = original.to_vec();
        bytes.extend_from_slice(tail);
        let parsed = parse_artifact(&bytes, limits(), &FixtureDates).await;
        assert_eq!(parsed.is_ok(), accepted, "tail {tail:?}: {parsed:?}");
        if let Ok(parsed) = parsed {
            assert_eq!(parsed.artifact_type(), ArtifactType::Manifest);
            assert_eq!(parsed.raw(), bytes);
        }
    }
}

#[tokio::test]
async fn terminal_zero_does_not_turn_unread_suffix_or_incomplete_type_into_success() {
    // Final Z verification only runs at the exact n-35 offset. This is a
    // source-derived consequence, not a native experiment or authenticity claim.
    let mut bytes = include_bytes!("fixtures/artifact/checksum-corrupt.artifact").to_vec();
    bytes.extend_from_slice(b"\0\n");
    assert!(
        parse_artifact(&bytes, limits(), &FixtureDates)
            .await
            .is_ok()
    );
    let missing_date = artifact(b"C c\n");
    let mut bytes = missing_date;
    bytes.extend_from_slice(b"\0\n");
    assert!(matches!(
        parse_artifact(&bytes, limits(), &FixtureDates).await,
        Err(ArtifactError::Syntax {
            message: "artifact card matrix violation",
            ..
        })
    ));
    let original = include_bytes!("fixtures/artifact/manifest-valid.artifact");
    for tail in [b"\0 \n".as_slice(), b"\0\n\n"] {
        let mut bytes = original.to_vec();
        bytes.extend_from_slice(tail);
        assert!(
            matches!(
                parse_artifact(&bytes, limits(), &FixtureDates).await,
                Err(ArtifactError::Syntax {
                    message: "extra characters at end of card",
                    ..
                })
            ),
            "tail {tail:?}"
        );
    }
    let mut unread = original.to_vec();
    unread.extend_from_slice(b"\0\n");
    // A checksum-valid final Z cannot make the parser resume after the sentinel.
    assert!(matches!(
        parse_artifact(&artifact(&unread), limits(), &FixtureDates).await,
        Err(ArtifactError::Syntax {
            message: "extra characters at end of card",
            ..
        })
    ));
    unread.extend_from_slice(b"Z aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n");
    // When a final checksum is recognizable but wrong, it fails before lexing.
    assert!(matches!(
        parse_artifact(&unread, limits(), &FixtureDates).await,
        Err(ArtifactError::Checksum)
    ));
}

#[tokio::test]
async fn malformed_card_fields_return_syntax_without_provider_causes() {
    use std::error::Error;
    let mut tab_target = b"A file ".to_vec();
    tab_target.extend_from_slice(&[b'a'; 39]);
    tab_target.extend_from_slice(b"\t\nD date\n");
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        ("lowercase card", b"C c\nD date\na text\n".to_vec(), "bad card type"),
        ("attachment parent path", b"A ../file page\nD date\n".to_vec(), "invalid A filename"),
        ("attachment control in forty-byte target", tab_target, "invalid A target"),
        ("wiki leading space", b"D date\nL \\spage\nU user\nW 0\n\n".to_vec(), "invalid wiki name"),
        ("cherrypick without sign", b"D date\nQ aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n".to_vec(), "invalid Q target"),
        ("tag invalid object identifier", b"D date\nT +name bad\nU user\n".to_vec(), "invalid T target"),
        ("technote tag points elsewhere", b"D date\nE date aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nT +name aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nW 0\n\n".to_vec(), "invalid technote tag"),
        ("technote removes tag", b"D date\nE date aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nT -name *\nW 0\n\n".to_vec(), "invalid technote tag"),
    ];
    for (name, prefix, expected) in cases {
        let bytes = artifact(&prefix);
        let error = parse_artifact(&bytes, limits(), &FixtureDates)
            .await
            .unwrap_err();
        assert!(
            matches!(&error, ArtifactError::Syntax { message, .. } if *message == expected),
            "{name}: {error:?}"
        );
        assert!(
            error.source().is_none(),
            "{name}: syntax has no provider cause"
        );
    }
}
