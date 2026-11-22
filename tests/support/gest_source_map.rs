//! Check a Gest converter source map against expectations committed here, so
//! that a stale or malformed map fails its test. The model source map of
//! `tests/gest_model.rs` reads the same pin, addendum and hash table.
//!
//! The pinned Gest tree is not in this repository. These checks compare each
//! recorded hash and field with the values below. They cannot hash the Gest
//! files themselves.

use serde_json::Value;
use std::collections::BTreeSet;

/// The Gest commit that every converter and model source map cites.
pub const SOURCE_PIN: &str = "eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8";

/// The sha256 of the test-accounting addendum that governs every map.
pub const ADDENDUM_SHA256: &str =
    "afddd1c2f5e905f72126209605737cc80e63791870d3ce8bd59f26c9589d8454";

/// The sha256 of each Gest source file at `SOURCE_PIN`, one entry per path.
///
/// One table serves every module, so two maps cannot record different hashes
/// for one file. Each value was compared with `git show <pin>:<path>` in a
/// Gest clone when this table was written.
const PINNED_SOURCES: &[(&str, &str)] = &[
    (
        "src/store/migration/m00008_create_entity_tags.rs",
        "de0893d54052d816b8e00c75c7cb4581ef9aefc187472195fef851618edf0926",
    ),
    (
        "src/store/migration/m00009_create_notes.rs",
        "07b81be0801dbfb6dd0ba6a9e84f70760cee87c23002e0bb4e26615499414084",
    ),
    (
        "src/store/migration/m00010_create_relationships.rs",
        "f8ba3ae9497e11042171aee70bb810ba4365baa6122b7a2afc05223c199a7c07",
    ),
    (
        "src/store/migration/m00011_create_iteration_tasks.rs",
        "b22427c910718b0352bb69670875e330a0f0795ac1456c28f01d6e543a4c71fc",
    ),
    (
        "src/store/model.rs",
        "6424e0b9791e9f4f2d4d0252200a951a1445bc709cca8e770de0c0ea922e767a",
    ),
    (
        "src/store/model/artifact.rs",
        "39f56375587cdbb8247275ba1ed7868ab54c06fe0a2faba44787fe5229b4f909",
    ),
    (
        "src/store/model/author.rs",
        "fe3fb4a00f27bb7c70a111c1070c3e4107c26459350ad5f7f632d32aead3bba4",
    ),
    (
        "src/store/model/entity_tag.rs",
        "a6cafa3169e9927f102f78b3ebc0aaaa407a95212a5c01b5d7020658459f4d90",
    ),
    (
        "src/store/model/iteration.rs",
        "d03df134c4ecd4ca42577f624f8e5fdf8579886bef5f5a6086c5bb0057f2624b",
    ),
    (
        "src/store/model/note.rs",
        "e63bc968d54cbf15d8e5f5f478d507471f6dc760f58e2c1a795eab43a6623877",
    ),
    (
        "src/store/model/primitives.rs",
        "f0377d97b40d7564c66c03a630abc7a006861e27151b3507f1d9baca3df890ed",
    ),
    (
        "src/store/model/primitives/author_type.rs",
        "51933978a31f2470e6de30d9edbdc058e0477a7156d86d6c045780267910cc9a",
    ),
    (
        "src/store/model/primitives/entity_type.rs",
        "e44c53c43f299e9d0d1d7e1df45c49c64717c605ce797abd7305690e70be3b92",
    ),
    (
        "src/store/model/primitives/id.rs",
        "1e257f775bdbdff3922f68af9a64a374ee48c12b2b587ab8c315218e19577bbc",
    ),
    (
        "src/store/model/primitives/iteration_status.rs",
        "6b4e39ba285b4ab8dd60e300c904960263631bc635718802f2e34322b3d59ed8",
    ),
    (
        "src/store/model/primitives/priority.rs",
        "fc80070c99ee14fc58575dd7c87fd64ce252d0ee8db9a6791d565db436d87537",
    ),
    (
        "src/store/model/primitives/relationship_type.rs",
        "08badb7cc7bd41dbd2458deb07db68ab48c200793c23b4376cb7ebe1742d4556",
    ),
    (
        "src/store/model/primitives/task_status.rs",
        "4a924e4390ea0dcf3e1fe3e814966274844cc801a2d8f054c70d388ba8d15649",
    ),
    (
        "src/store/model/project.rs",
        "3a95afe11cf39b88a20af1ceb34a2255441c30bc9849a21d717691961eaf89c8",
    ),
    (
        "src/store/model/project_workspace.rs",
        "2383f44aa4a9b7463acb9ed4b6cac5aed963ab5e5f56289d77b4c5dae928c6fb",
    ),
    (
        "src/store/model/relationship.rs",
        "268acb2ff02f81c9d378c718c679f0ae9f16c000957b7116216320c751d340a1",
    ),
    (
        "src/store/model/tag.rs",
        "40c657ba2dd3695c385e1485ba1e58a57d7f67622665dd736efc10b30f51c6b5",
    ),
    (
        "src/store/model/task.rs",
        "dd011be45e961b46f5963e8c20429b9afc68395afa9cb9f2e5332e219ec3a0d7",
    ),
    (
        "src/store/model/transaction.rs",
        "ab193f96d9cb280b7cb5a7b9acfe09c0a9aa50d961662a6cba1320a8626d89b5",
    ),
    (
        "src/store/repo/iteration.rs",
        "aba6044e5bff183d9ea42d616c15df61e5f63562d173997bc4dd554154ba088c",
    ),
];

/// The keys that each entry of `fields` must have, no more and no fewer.
const FIELD_KEYS: &[&str] = &[
    "canonical_field",
    "control_file",
    "controls",
    "name",
    "nullable",
    "owned_value_rule",
    "sql",
    "sqlite_decl_allows_null",
];

/// The keys that each entry of `supporting_sources` must have.
const SUPPORT_KEYS: &[&str] = &["path", "role", "sha256"];

/// What one module expects of its source map.
pub struct Expected<'a> {
    /// The cited Gest files in map order: `source` first, then each
    /// `supporting_sources` path.
    pub sources: &'a [&'a str],
    /// Each Gest column name with its canonical field, in map order.
    pub fields: &'a [(&'a str, &'a str)],
    /// The test file that the map names as `control_file`.
    pub control_file: &'a str,
}

/// Return the error `message` unless `ok` holds.
fn ensure(ok: bool, message: impl FnOnce() -> String) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message()) }
}

/// Return the string at `value`, or an error that names `what`.
fn text<'v>(value: &'v Value, what: &str) -> Result<&'v str, String> {
    value
        .as_str()
        .ok_or_else(|| format!("{what} is not a string"))
}

/// Return the keys of the object `value`, or an error that names `what`.
fn keys<'v>(value: &'v Value, what: &str) -> Result<BTreeSet<&'v str>, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{what} is not an object"))?;
    Ok(object.keys().map(String::as_str).collect())
}

/// Return the list of strings at `value`, refusing a duplicate entry.
fn unique_strings<'v>(value: &'v Value, what: &str) -> Result<Vec<&'v str>, String> {
    let items = value
        .as_array()
        .ok_or_else(|| format!("{what} is not an array"))?;
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for item in items {
        let item = text(item, what)?;
        ensure(seen.insert(item), || format!("{what} repeats {item}"))?;
        out.push(item);
    }
    Ok(out)
}

/// Return the pinned hash of `path`, or an error when the table has no entry.
pub fn pinned(path: &str) -> Result<&'static str, String> {
    PINNED_SOURCES
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, hash)| *hash)
        .ok_or_else(|| format!("{path} is not a pinned Gest source"))
}

/// Check `map` against `expected` and the pinned tables.
///
/// The check fails when the map cites another Gest commit or addendum, cites a
/// file other than the expected ones, records a hash that differs from the
/// pinned one, or lists a field twice, out of order or not at all. It also
/// fails when a field entry has a missing or extra key, when a control is
/// repeated or not in `test_functions`, and when a later obligation cites
/// another commit or has a malformed hash.
///
/// # Arguments
///
/// * `map` - the parsed source map.
/// * `expected` - the committed expectations of its module.
///
/// # Returns
///
/// `Ok(())`, or the first problem found.
///
/// # Errors
///
/// Returns a message that names the first clause above that the map breaks, or
/// names a key whose value is not the string, array or object that it expects.
pub fn check(map: &Value, expected: &Expected<'_>) -> Result<(), String> {
    ensure(map["source_pin"] == SOURCE_PIN, || {
        format!("source_pin is {}", map["source_pin"])
    })?;
    ensure(map["governing_addendum_sha256"] == ADDENDUM_SHA256, || {
        "governing_addendum_sha256 differs".to_owned()
    })?;

    let mut cited = vec![(
        text(&map["source"], "source")?,
        text(&map["source_sha256"], "source_sha256")?,
    )];
    if let Some(support) = map.get("supporting_sources") {
        let support = support
            .as_array()
            .ok_or("supporting_sources is not an array")?;
        for entry in support {
            let found = keys(entry, "a supporting source")?;
            ensure(found == SUPPORT_KEYS.iter().copied().collect(), || {
                format!("a supporting source has keys {found:?}")
            })?;
            cited.push((
                text(&entry["path"], "path")?,
                text(&entry["sha256"], "sha256")?,
            ));
        }
    }
    let paths: Vec<&str> = cited.iter().map(|(path, _)| *path).collect();
    ensure(paths == expected.sources, || {
        format!("cited sources are {paths:?}")
    })?;
    for (path, hash) in &cited {
        ensure(*hash == pinned(path)?, || {
            format!("{path} records hash {hash}")
        })?;
    }

    let fields = map["fields"].as_array().ok_or("fields is not an array")?;
    let mut names = BTreeSet::new();
    let mut found_fields = Vec::new();
    let mut controls = BTreeSet::new();
    for field in fields {
        let found = keys(field, "a field")?;
        ensure(found == FIELD_KEYS.iter().copied().collect(), || {
            format!("a field has keys {found:?}")
        })?;
        let name = text(&field["name"], "name")?;
        let canonical = text(&field["canonical_field"], "canonical_field")?;
        ensure(names.insert(name), || format!("field {name} appears twice"))?;
        ensure(field["control_file"] == expected.control_file, || {
            format!("field {name} names another control file")
        })?;
        let listed = unique_strings(&field["controls"], "controls")?;
        ensure(!listed.is_empty(), || {
            format!("field {name} has no control")
        })?;
        controls.extend(listed);
        found_fields.push((name, canonical));
    }
    ensure(found_fields == expected.fields, || {
        format!("fields are {found_fields:?}")
    })?;
    ensure(map["source_field_count"] == expected.fields.len(), || {
        "source_field_count differs from the field list".to_owned()
    })?;
    let functions: BTreeSet<&str> = unique_strings(&map["test_functions"], "test_functions")?
        .into_iter()
        .collect();
    ensure(functions == controls, || {
        "test_functions differs from the controls of the fields".to_owned()
    })?;

    if let Some(rows) = map.get("later_exact_api_obligations") {
        let rows = rows
            .as_array()
            .ok_or("later_exact_api_obligations is not an array")?;
        let mut ids = BTreeSet::new();
        for row in rows {
            let id = text(&row["test_id"], "test_id")?;
            ensure(ids.insert(id), || format!("obligation {id} appears twice"))?;
            ensure(id.starts_with(&format!("{SOURCE_PIN}:")), || {
                format!("obligation {id} cites another commit")
            })?;
            let hash = text(&row["fixture_sha256"], "fixture_sha256")?;
            ensure(
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                || format!("obligation {id} has a malformed fixture hash"),
            )?;
        }
    }
    Ok(())
}

/// One change to a copy of a source map, which `check` must then refuse.
type Mutation = fn(&mut Value);

/// Assert that `check` refuses `copy` with an error that names `reason`.
///
/// The reason ties each bad copy to the clause of `check` that must refuse it, so
/// a broken clause fails even when another clause would refuse the copy later.
///
/// # Arguments
///
/// * `copy` - a changed copy of a source map.
/// * `expected` - the committed expectations of its module.
/// * `what` - the name of the change, for the failure message.
/// * `reason` - a part of the error text that the refusing clause gives.
///
/// # Panics
///
/// Panics when `check` accepts the copy, or refuses it for another reason.
fn assert_refused(copy: &Value, expected: &Expected<'_>, what: &str, reason: &str) {
    match check(copy, expected) {
        Ok(()) => panic!("{what} was accepted"),
        Err(error) => assert!(error.contains(reason), "{what} gave {error}"),
    }
}

/// Check that `check` refuses each kind of stale or malformed copy of `map`.
///
/// Each copy changes one thing, and each clause of `check` refuses at least one
/// copy. The changes are a stale pin, addendum or cited hash, a cited path that
/// the pin table lacks, a field that is dropped, repeated or renamed, a field
/// with a missing or extra key, another control file or no control, a repeated
/// control, an extra test function, and a wrong field count. A map with
/// supporting sources also gets a stale, dropped and malformed supporting
/// source. A map with later obligations also gets an obligation that cites
/// another commit, has a malformed hash or appears twice.
///
/// # Arguments
///
/// * `map` - a source map that `check` accepts.
/// * `expected` - the committed expectations of its module.
///
/// # Panics
///
/// Panics when `check` refuses `map`, or accepts a bad copy.
pub fn assert_rejects_stale_and_malformed(map: &Value, expected: &Expected<'_>) {
    check(map, expected).unwrap();
    let mutants: [(&str, &str, Mutation); 13] = [
        ("stale source hash", "records hash", |m| {
            m["source_sha256"] = Value::from("0".repeat(64));
        }),
        ("other pin", "source_pin is", |m| {
            m["source_pin"] = Value::from("0".repeat(40));
        }),
        ("other addendum", "governing_addendum_sha256 differs", |m| {
            m["governing_addendum_sha256"] = Value::from("0".repeat(64));
        }),
        ("dropped field", "fields are", |m| {
            m["fields"].as_array_mut().unwrap().pop();
        }),
        ("repeated field", "appears twice", |m| {
            let first = m["fields"][0].clone();
            m["fields"].as_array_mut().unwrap().push(first);
        }),
        ("renamed field", "fields are", |m| {
            m["fields"][0]["canonical_field"] = Value::from("renamed");
        }),
        ("extra field key", "a field has keys", |m| {
            m["fields"][0]["extra"] = Value::Bool(true);
        }),
        ("missing field key", "a field has keys", |m| {
            m["fields"][0].as_object_mut().unwrap().remove("sql");
        }),
        ("other control file", "names another control file", |m| {
            m["fields"][0]["control_file"] = Value::from("tests/other.rs");
        }),
        ("no control", "has no control", |m| {
            m["fields"][0]["controls"] = Value::Array(Vec::new());
        }),
        ("repeated control", "controls repeats", |m| {
            let first = m["fields"][0]["controls"][0].clone();
            m["fields"][0]["controls"]
                .as_array_mut()
                .unwrap()
                .push(first);
        }),
        ("extra test function", "test_functions differs", |m| {
            m["test_functions"]
                .as_array_mut()
                .unwrap()
                .push(Value::from("no_such_control"));
        }),
        ("wrong field count", "source_field_count differs", |m| {
            m["source_field_count"] = Value::from(0);
        }),
    ];
    for (what, reason, mutate) in mutants {
        let mut copy = map.clone();
        mutate(&mut copy);
        assert_refused(&copy, expected, what, reason);
    }

    let mut copy = map.clone();
    copy["source"] = Value::from("src/unpinned.rs");
    let mut sources = expected.sources.to_vec();
    sources[0] = "src/unpinned.rs";
    let unpinned = Expected {
        sources: &sources,
        fields: expected.fields,
        control_file: expected.control_file,
    };
    assert_refused(
        &copy,
        &unpinned,
        "unpinned path",
        "is not a pinned Gest source",
    );

    if map.get("supporting_sources").is_some() {
        let mut copy = map.clone();
        copy["supporting_sources"][0]["sha256"] = Value::from("f".repeat(64));
        assert_refused(&copy, expected, "stale supporting hash", "records hash");
        let mut copy = map.clone();
        copy["supporting_sources"].as_array_mut().unwrap().pop();
        assert_refused(
            &copy,
            expected,
            "dropped supporting source",
            "cited sources are",
        );
        let mut copy = map.clone();
        copy["supporting_sources"][0]
            .as_object_mut()
            .unwrap()
            .remove("role");
        assert_refused(
            &copy,
            expected,
            "supporting source key",
            "a supporting source has keys",
        );
    }

    let has_obligations = map
        .get("later_exact_api_obligations")
        .and_then(Value::as_array)
        .is_some_and(|rows| !rows.is_empty());
    if has_obligations {
        let mut copy = map.clone();
        copy["later_exact_api_obligations"][0]["test_id"] = Value::from("0:src/x.rs:1:t");
        assert_refused(&copy, expected, "obligation commit", "cites another commit");
        let mut copy = map.clone();
        copy["later_exact_api_obligations"][0]["fixture_sha256"] = Value::from("A".repeat(64));
        assert_refused(&copy, expected, "obligation hash", "malformed fixture hash");
        let mut copy = map.clone();
        let first = copy["later_exact_api_obligations"][0].clone();
        copy["later_exact_api_obligations"]
            .as_array_mut()
            .unwrap()
            .push(first);
        assert_refused(&copy, expected, "repeated obligation", "appears twice");
    }
}
