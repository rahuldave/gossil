use gossil::artifact::{
    ArtifactError, DateProvider, DateProviderError, ParseLimits, parse_artifact_with_usage,
};
struct Dates;
impl DateProvider for Dates {
    async fn julianday(&self, _: Option<&[u8]>) -> Result<Option<f64>, DateProviderError> {
        Ok(Some(1.0))
    }
}
fn limits() -> ParseLimits {
    ParseLimits {
        input_bytes: 10000,
        cards: 100,
        owned_bytes: 10000,
        entries: 100,
    }
}
fn artifact(prefix: &[u8]) -> Vec<u8> {
    let mut input = prefix.to_vec();
    input.extend_from_slice(b"Z ");
    for byte in gossil::artifact_checksum::md5(prefix) {
        input.extend_from_slice(format!("{byte:02x}").as_bytes());
    }
    input.push(b'\n');
    input
}
#[tokio::test]
async fn reports_existing_charges_and_exact_thresholds() {
    let input = artifact(b"C a\\sb\\0tail\nD date\nP\nP\n");
    let (_, usage) = parse_artifact_with_usage(&input, limits(), &Dates)
        .await
        .unwrap();
    assert_eq!(usage.input_bytes, input.len());
    assert_eq!(usage.cards, 5); // C, D, two empty P cards, Z.
    assert_eq!(usage.owned_bytes, input.len() + 10 + 4); // Encoded C token, copied D token.
    assert_eq!(usage.entries, 0); // Empty P cards append nothing.
    let exact = ParseLimits {
        input_bytes: usage.input_bytes,
        cards: usage.cards,
        owned_bytes: usage.owned_bytes,
        entries: usage.entries,
    };
    assert!(
        parse_artifact_with_usage(&input, exact, &Dates)
            .await
            .is_ok()
    );
    for bound in 0..3 {
        let mut narrow = exact;
        match bound {
            0 => narrow.input_bytes -= 1,
            1 => narrow.cards -= 1,
            _ => narrow.owned_bytes -= 1,
        }
        assert!(matches!(
            parse_artifact_with_usage(&input, narrow, &Dates).await,
            Err(ArtifactError::ResourceLimit)
        ));
    }
}
#[tokio::test]
async fn wrapper_and_zero_marker_keep_the_original_counting_rules() {
    let plain = artifact(b"C c\nD date\n");
    let (_, plain_usage) = parse_artifact_with_usage(&plain, limits(), &Dates)
        .await
        .unwrap();
    let mut sentinel = plain.clone();
    sentinel.extend_from_slice(b"\0\n");
    let mut wrapped = b"-----BEGIN PGP SIGNED MESSAGE-----\n\n".to_vec();
    wrapped.extend_from_slice(&plain);
    wrapped.extend_from_slice(b"-----BEGIN PGP SIGNATURE-----\nanything\n");
    for bytes in [&sentinel, &wrapped] {
        let (parsed, used) = parse_artifact_with_usage(bytes, limits(), &Dates)
            .await
            .unwrap();
        assert_eq!(parsed.raw(), bytes);
        assert_eq!(used.input_bytes, bytes.len());
        assert_eq!(used.cards, plain_usage.cards);
        assert_eq!(
            used.owned_bytes,
            plain_usage.owned_bytes + bytes.len() - plain.len()
        );
        assert_eq!(used.entries, 0);
    }
}
#[tokio::test]
async fn usage_can_narrow_a_later_parse_before_its_allocation() {
    let input = artifact(b"M aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n");
    let (_, used) = parse_artifact_with_usage(&input, limits(), &Dates)
        .await
        .unwrap();
    assert_eq!(used.entries, 1);
    assert_eq!(used.cards, 2);
    assert_eq!(
        used.owned_bytes,
        input.len() + 40 + std::mem::size_of::<Vec<u8>>()
    );
    for dimension in 0..4 {
        let mut remaining = ParseLimits {
            input_bytes: used.input_bytes * 2,
            cards: used.cards * 2,
            owned_bytes: used.owned_bytes * 2,
            entries: used.entries * 2,
        };
        match dimension {
            0 => remaining.input_bytes -= 1,
            1 => remaining.cards -= 1,
            2 => remaining.owned_bytes -= 1,
            _ => remaining.entries -= 1,
        }
        let (_, first) = parse_artifact_with_usage(&input, remaining, &Dates)
            .await
            .unwrap();
        remaining.input_bytes -= first.input_bytes;
        remaining.cards -= first.cards;
        remaining.owned_bytes -= first.owned_bytes;
        remaining.entries -= first.entries;
        assert!(
            matches!(
                parse_artifact_with_usage(&input, remaining, &Dates).await,
                Err(ArtifactError::ResourceLimit)
            ),
            "dimension {dimension}"
        );
    }
}

#[tokio::test]
async fn every_preserved_fixture_matches_the_original_entry_point() {
    let mut fixtures = std::fs::read_dir("tests/fixtures/artifact")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "artifact")
        })
        .collect::<Vec<_>>();
    fixtures.sort();
    assert_eq!(fixtures.len(), 93);
    for path in fixtures {
        let bytes = std::fs::read(&path).unwrap();
        let old = gossil::artifact::parse_artifact(&bytes, limits(), &Dates).await;
        let reported = parse_artifact_with_usage(&bytes, limits(), &Dates)
            .await
            .map(|(parsed, _)| parsed);
        assert_eq!(
            format!("{old:?}"),
            format!("{reported:?}"),
            "{}",
            path.display()
        );
    }
}
