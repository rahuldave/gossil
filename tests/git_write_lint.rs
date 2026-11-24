//! GIT-T07: a call of each Git write path that `clippy.toml` bans fails the lint of `just lint`.
//!
//! The test writes a small crate with one call of each banned path, and runs clippy on it with
//! the flags of the `lint` recipe of the Justfile and the real `clippy.toml`. Each marked line
//! must give its own disallowed-path error, and a plain read must give none.
//!
//! `every_gix_writer_is_banned_or_explained` checks that each public gix function whose name
//! says that it may write is banned or has a reason why it cannot write to Git.
//! `gix_is_used_only_in_the_reader_module` checks that no source file other than
//! `src/git_reader.rs` names a gix path.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The probe source has one call or mention per banned path. The comment after each line
/// names the path it must trip.
const PROBE: &str = r#"#![allow(unused, unreachable_code, clippy::diverging_sub_expression, clippy::let_unit_value)]
use gix_object::Write as _;

pub fn objects(odb: &gix_odb::Handle, id: gix_hash::ObjectId) {
    let _ = gix_odb::at("objects", gix_hash::Kind::Sha1); // allowed
    let _ = odb.write(&gix_object::Tree::empty()); // ban: gix_object::Write::write
    let _ = odb.write_buf(gix_object::Kind::Blob, b""); // ban: gix_object::Write::write_buf
    let _ = odb.write_buf_with_known_id(gix_object::Kind::Blob, b"", id); // ban: gix_object::Write::write_buf_with_known_id
    let _ = odb.write_stream(gix_object::Kind::Blob, 0, &mut std::io::empty()); // ban: gix_object::Write::write_stream
    let _ = odb.write_stream_with_known_id(gix_object::Kind::Blob, 0, &mut std::io::empty(), id); // ban: gix_object::Write::write_stream_with_known_id
    let _ = gix_odb::sink(gix_hash::Kind::Sha1); // ban: gix_odb::sink
}

pub fn sink_type(sink: Option<gix_odb::Sink>) {} // ban: gix_odb::Sink

pub fn packs() {
    let _ = gix_odb::pack::multi_index::write_from_index_paths(Vec::new(), todo!(), todo!(), todo!(), todo!()); // ban: gix_odb::pack::multi_index::write_from_index_paths
}

pub fn refs(store: &gix_ref::file::Store) {
    let _ = store.find_loose("HEAD"); // allowed
    let transaction = store.transaction(); // ban: gix_ref::file::Store::transaction
    let prepared = transaction
        .prepare(Vec::new(), gix_lock::acquire::Fail::Immediately, gix_lock::acquire::Fail::Immediately) // ban: gix_ref::file::Transaction::prepare
        .unwrap();
    let _ = prepared.commit(None); // ban: gix_ref::file::Transaction::commit
}

pub fn ref_type(transaction: Option<gix_ref::file::Transaction<'_, '_>>) {} // ban: gix_ref::file::Transaction

pub fn locks() {
    let fail = gix_lock::acquire::Fail::Immediately;
    let _ = gix_lock::File::acquire_to_update_resource("x", fail, None, 0); // ban: gix_lock::File::acquire_to_update_resource
    let _ = gix_lock::Marker::acquire_to_hold_resource("x", fail, None, 0); // ban: gix_lock::Marker::acquire_to_hold_resource
}

pub fn lock_types(file: Option<gix_lock::File>) {} // ban: gix_lock::File

pub fn marker_type(marker: Option<gix_lock::Marker>) {} // ban: gix_lock::Marker

pub fn config(file: &gix_config::File) {
    let _ = file.string("core.hooksPath"); // allowed
    let _ = file.write_to(&mut Vec::new()); // ban: gix_config::File::write_to
    let _ = file.write_to_filter(&mut Vec::new(), |_| true); // ban: gix_config::File::write_to_filter
}

pub fn serializers() {
    let mut out = Vec::new();
    let _ = gix_config::file::SectionRef::write_to(todo!(), &mut out); // ban: gix_config::file::SectionRef::write_to
    let _ = gix_config::parse::EventRef::write_to(todo!(), &mut out); // ban: gix_config::parse::EventRef::write_to
    let _ = gix_object::WriteTo::write_to(&gix_object::Tree::empty(), &mut out); // ban: gix_object::WriteTo::write_to
    let _ = gix_object::tree::Editor::write(todo!(), |_: &gix_object::Tree| -> Result<gix_hash::ObjectId, std::io::Error> { todo!() }); // ban: gix_object::tree::Editor::write
    let _ = gix_object::tree::editor::Cursor::write(todo!(), |_: &gix_object::Tree| -> Result<gix_hash::ObjectId, std::io::Error> { todo!() }); // ban: gix_object::tree::editor::Cursor::write
    let _ = gix_ref::log::Line::write_to(todo!(), &mut out); // ban: gix_ref::log::Line::write_to
    let _ = gix_odb::pack::multi_index::chunk::index_names::write(Vec::<std::path::PathBuf>::new(), &mut out); // ban: gix_odb::pack::multi_index::chunk::index_names::write
    let _ = gix_odb::pack::data::entry::Header::write_to(todo!(), 0, &mut out); // ban: gix_odb::pack::data::entry::Header::write_to
    let _ = gix_actor::Signature::write_to(todo!(), &mut out); // ban: gix_actor::Signature::write_to
    let _ = gix_actor::SignatureRef::write_to(todo!(), &mut out); // ban: gix_actor::SignatureRef::write_to
    let _ = gix_actor::Identity::write_to(todo!(), &mut out); // ban: gix_actor::Identity::write_to
    let _ = gix_actor::IdentityRef::write_to(todo!(), &mut out); // ban: gix_actor::IdentityRef::write_to
    let _ = gix_date::Time::write_to(todo!(), &mut out); // ban: gix_date::Time::write_to
    let _ = gix_tempfile::Handle::<gix_tempfile::handle::Writable>::persist(todo!(), "x"); // ban: gix_tempfile::Handle::persist
    let _ = gix_lock::File::commit(todo!()); // ban: gix_lock::File::commit
    let _ = gix_lock::Marker::commit(todo!()); // ban: gix_lock::Marker::commit
}
"#;

/// The extra crates of the probe, pinned to the versions of Cargo.lock. They are crates whose
/// writers `clippy.toml` bans but that gossil reaches only through the six gix crates.
const PROBE_EXTRA: &str = "gix-lock = \"=26.0.1\"\n\
    gix-tempfile = { version = \"=26.0.1\", default-features = false }\n\
    gix-actor = { version = \"=0.44.1\", default-features = false }\n\
    gix-date = { version = \"=0.18.1\", default-features = false }\n";

/// Return the clippy flags that the `lint` recipe of the Justfile passes after `--`.
fn lint_flags(root: &Path) -> Vec<String> {
    let justfile = fs::read_to_string(root.join("Justfile")).unwrap();
    let recipe = justfile
        .split("\nlint *args:\n")
        .nth(1)
        .expect("the Justfile has its own lint recipe");
    let line = recipe.lines().next().unwrap();
    assert!(line.contains("cargo clippy"), "{line}");
    let flags: Vec<String> = line
        .split(" -- ")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    for lint in ["clippy::disallowed_methods", "clippy::disallowed_types"] {
        let position = flags.iter().position(|flag| flag == lint).expect(lint);
        assert_eq!(
            flags[position - 1],
            "-F",
            "{lint} must be forbidden, so that a local #[allow] cannot lift it"
        );
    }
    flags
}

/// Return the banned paths of `clippy.toml`, those of this build and those marked `allow-invalid`.
fn banned_paths(root: &Path) -> (BTreeSet<String>, BTreeSet<String>) {
    let config: toml::Table = fs::read_to_string(root.join("clippy.toml"))
        .unwrap()
        .parse()
        .unwrap();
    let mut present = BTreeSet::new();
    let mut absent = BTreeSet::new();
    for key in ["disallowed-methods", "disallowed-types"] {
        for entry in config[key].as_array().unwrap() {
            let path = entry["path"].as_str().unwrap().to_string();
            match entry.get("allow-invalid").and_then(toml::Value::as_bool) {
                Some(true) => absent.insert(path),
                _ => present.insert(path),
            };
        }
    }
    (present, absent)
}

/// One diagnostic of the probe, with its line, its code and its text.
struct Diagnostic {
    /// Line of the primary span.
    line: u64,
    /// Lint name or error code, empty when none.
    code: String,
    /// Level, such as `error`.
    level: String,
    /// Short message.
    text: String,
}

/// Write a probe crate with `source`, run the lint flags on it, and return its diagnostics.
fn run_probe(root: &Path, flags: &[String], source: &str) -> Vec<Diagnostic> {
    let dir = tempfile::Builder::new()
        .prefix("gossil-lint-probe-")
        .tempdir()
        .unwrap();
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let gix: Vec<&str> = manifest
        .lines()
        .filter(|line| line.starts_with("gix-"))
        .collect();
    assert_eq!(gix.len(), 6, "the six pinned gix crates");
    let manifest = format!(
        "[package]\nname = \"git-write-lint-probe\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n\
         [lib]\npath = \"lib.rs\"\n\n[dependencies]\n{}\n{PROBE_EXTRA}\n[workspace]\n",
        gix.join("\n")
    );
    fs::write(dir.path().join("Cargo.toml"), manifest).unwrap();
    fs::write(dir.path().join("lib.rs"), source).unwrap();
    fs::copy(root.join("Cargo.lock"), dir.path().join("Cargo.lock")).unwrap();

    let mut command = Command::new("cargo");
    command
        .args(["clippy", "--offline", "--message-format=json", "--"])
        .args(flags)
        .current_dir(dir.path())
        .env_clear()
        .env("CLIPPY_CONF_DIR", root)
        .env("CARGO_TARGET_DIR", root.join("target/git-write-lint-probe"));
    for key in [
        "PATH",
        "HOME",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "RUSTUP_TOOLCHAIN",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let output = command.output().unwrap();
    assert!(!output.status.success(), "each probe must fail");
    let mut out = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if message["reason"] != "compiler-message" {
            continue;
        }
        let message = &message["message"];
        for span in message["spans"].as_array().unwrap() {
            out.push(Diagnostic {
                line: span["line_start"].as_u64().unwrap(),
                code: message["code"]["code"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                level: message["level"].as_str().unwrap_or_default().to_string(),
                text: message["message"].as_str().unwrap_or_default().to_string(),
            });
        }
    }
    out
}

/// Report whether `code` is a compile error code such as `E0599`.
fn is_compile_error(code: &str) -> bool {
    code.len() == 5 && code.starts_with('E') && code[1..].bytes().all(|b| b.is_ascii_digit())
}

/// GIT-T07: each banned write path fails the lint, a read does not, and each writer that the
/// build leaves out does not compile.
#[test]
fn git_t07_each_write_path_fails_the_lint() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let flags = lint_flags(root);
    let (present, absent) = banned_paths(root);

    let found = run_probe(root, &flags, PROBE);
    for diagnostic in &found {
        assert!(
            !is_compile_error(&diagnostic.code),
            "the probe must compile: line {}: {}",
            diagnostic.line,
            diagnostic.text
        );
    }
    let mut marked = BTreeSet::new();
    for (index, line) in PROBE.lines().enumerate() {
        let number = index as u64 + 1;
        let here: Vec<&Diagnostic> = found
            .iter()
            .filter(|d| d.line == number && d.code.starts_with("clippy::disallowed_"))
            .collect();
        if line.ends_with("// allowed") {
            assert!(here.is_empty(), "line {number} is a read and must pass");
        } else if let Some((_, path)) = line.split_once("// ban: ") {
            assert!(
                here.iter()
                    .any(|d| d.level == "error" && d.text.contains(&format!("`{path}`"))),
                "line {number} must fail the lint for `{path}`"
            );
            marked.insert(path.to_string());
        }
    }
    assert_eq!(
        marked, present,
        "the probe covers each entry of clippy.toml"
    );

    let source: String = absent
        .iter()
        .enumerate()
        .map(|(index, path)| format!("pub fn absent_{index}() {{ let _ = {path}; }}\n"))
        .collect();
    let found = run_probe(root, &flags, &source);
    for (index, _) in source.lines().enumerate() {
        let number = index as u64 + 1;
        assert!(
            found.iter().any(|d| d.line == number && ["E0425", "E0433", "E0599"].contains(&d.code.as_str())),
            "line {number}: an allow-invalid entry must name a writer that this build leaves out"
        );
    }
    assert!(!absent.is_empty());
}

/// One public function of a gix crate whose name says that it may write, as (package, file below
/// `src`, function name, whether it takes `self`).
struct Site {
    /// Package name, such as `gix-ref`.
    package: String,
    /// File below the `src` folder of the package.
    file: String,
    /// Function name.
    function: String,
    /// True when the function takes `self`, so a caller needs only a value of its type.
    method: bool,
}

/// Report whether a function name says that the function may write, create, remove or change.
fn is_writer_name(name: &str) -> bool {
    const PREFIXES: [&str; 14] = [
        "write",
        "persist",
        "create",
        "remove",
        "delete",
        "rename",
        "set_",
        "mark_at",
        "writable_at",
        "probe",
        "cleanup",
        "clear_readonly",
        "adjust_",
        "open_options",
    ];
    name == "commit" || name == "setup" || PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

/// Return the packages that gossil can name, which are the six gix dependencies and each package
/// that one of them re-exports, whole or in part, such as `gix_odb::pack`.
fn nameable_packages(sources: &BTreeMap<String, PathBuf>) -> BTreeSet<String> {
    let manifest =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let mut nameable: BTreeSet<String> = manifest
        .lines()
        .filter(|line| line.starts_with("gix-"))
        .map(|line| line.split_whitespace().next().unwrap().to_string())
        .collect();
    assert_eq!(nameable.len(), 6);
    loop {
        let mut added = BTreeSet::new();
        for package in &nameable {
            for (_, text) in rust_files(&sources[package]) {
                for line in text.lines() {
                    let line = line.trim_start();
                    let Some(rest) = line
                        .strip_prefix("pub use ")
                        .or_else(|| line.strip_prefix("pub extern crate "))
                    else {
                        continue;
                    };
                    let module: String = rest
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    let package = module.replace('_', "-");
                    if module.starts_with("gix_")
                        && sources.contains_key(&package)
                        && !nameable.contains(&package)
                    {
                        added.insert(package);
                    }
                }
            }
        }
        if added.is_empty() {
            return nameable;
        }
        nameable.extend(added);
    }
}

/// Return each `.rs` file below `dir`, with its path below `dir` and its text.
fn rust_files(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(folder) = stack.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let relative = path
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.push((relative, fs::read_to_string(&path).unwrap()));
            }
        }
    }
    out
}

/// Return the `src` folder of each gix package of Cargo.lock, from `cargo metadata`.
fn gix_sources(root: &Path) -> BTreeMap<String, PathBuf> {
    let output = Command::new("cargo")
        .args(["metadata", "--offline", "--locked", "--format-version", "1"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "cargo metadata");
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut out = BTreeMap::new();
    for package in metadata["packages"].as_array().unwrap() {
        let name = package["name"].as_str().unwrap();
        if name.starts_with("gix-") {
            let manifest = Path::new(package["manifest_path"].as_str().unwrap());
            out.insert(name.to_string(), manifest.parent().unwrap().join("src"));
        }
    }
    let lock: toml::Table = fs::read_to_string(root.join("Cargo.lock"))
        .unwrap()
        .parse()
        .unwrap();
    let locked = lock["package"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|package| package["name"].as_str().unwrap().starts_with("gix-"))
        .count();
    assert_eq!(
        out.len(),
        locked,
        "each gix package of Cargo.lock has its sources"
    );
    out
}

/// Return each public function with a writer name in the gix packages.
fn writer_sites(sources: &BTreeMap<String, PathBuf>) -> Vec<Site> {
    let mut out = Vec::new();
    for (package, dir) in sources {
        for (file, text) in rust_files(dir) {
            let lines: Vec<&str> = text.lines().collect();
            for (index, line) in lines.iter().enumerate() {
                let line = line.trim_start();
                let rest = [
                    "pub fn ",
                    "pub const fn ",
                    "pub unsafe fn ",
                    "pub async fn ",
                ]
                .iter()
                .find_map(|prefix| line.strip_prefix(prefix));
                let Some(rest) = rest else { continue };
                let function: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !is_writer_name(&function) {
                    continue;
                }
                let signature = lines[index..lines.len().min(index + 8)].join(" ");
                let parameters = signature.split_once('(').map_or("", |(_, rest)| rest);
                let first = parameters.split([',', ')']).next().unwrap_or_default();
                out.push(Site {
                    package: package.clone(),
                    file: file.clone(),
                    function,
                    method: first.contains("self"),
                });
            }
        }
    }
    out
}

/// Each writer site that `clippy.toml` bans, as (package, file, function, banned path). A file with
/// two sites of one name, such as `Signature::write_to` and `SignatureRef::write_to`, has a ban for
/// each.
const BANNED_SITES: [(&str, &str, &str, &str); 22] = [
    (
        "gix-actor",
        "identity.rs",
        "write_to",
        "gix_actor::Identity::write_to",
    ),
    (
        "gix-actor",
        "identity.rs",
        "write_to",
        "gix_actor::IdentityRef::write_to",
    ),
    (
        "gix-actor",
        "signature/mod.rs",
        "write_to",
        "gix_actor::Signature::write_to",
    ),
    (
        "gix-actor",
        "signature/mod.rs",
        "write_to",
        "gix_actor::SignatureRef::write_to",
    ),
    (
        "gix-config",
        "file/section/mod.rs",
        "write_to",
        "gix_config::file::SectionRef::write_to",
    ),
    (
        "gix-config",
        "file/write.rs",
        "write_to",
        "gix_config::File::write_to",
    ),
    (
        "gix-config",
        "file/write.rs",
        "write_to_filter",
        "gix_config::File::write_to_filter",
    ),
    (
        "gix-config",
        "parse/event.rs",
        "write_to",
        "gix_config::parse::EventRef::write_to",
    ),
    (
        "gix-date",
        "time/write.rs",
        "write_to",
        "gix_date::Time::write_to",
    ),
    ("gix-lock", "commit.rs", "commit", "gix_lock::File::commit"),
    (
        "gix-lock",
        "commit.rs",
        "commit",
        "gix_lock::Marker::commit",
    ),
    (
        "gix-object",
        "tree/editor.rs",
        "write",
        "gix_object::tree::Editor::write",
    ),
    (
        "gix-object",
        "tree/editor.rs",
        "write",
        "gix_object::tree::editor::Cursor::write",
    ),
    (
        "gix-pack",
        "bundle/write/mod.rs",
        "write_to_directory",
        "gix_odb::pack::Bundle::write_to_directory",
    ),
    (
        "gix-pack",
        "bundle/write/mod.rs",
        "write_to_directory_eagerly",
        "gix_odb::pack::Bundle::write_to_directory_eagerly",
    ),
    (
        "gix-pack",
        "data/entry/header.rs",
        "write_to",
        "gix_odb::pack::data::entry::Header::write_to",
    ),
    (
        "gix-pack",
        "index/write/mod.rs",
        "write_data_iter_to_stream",
        "gix_odb::pack::index::write_data_iter_to_stream",
    ),
    (
        "gix-pack",
        "multi_index/chunk.rs",
        "write",
        "gix_odb::pack::multi_index::chunk::index_names::write",
    ),
    (
        "gix-pack",
        "multi_index/write.rs",
        "write_from_index_paths",
        "gix_odb::pack::multi_index::write_from_index_paths",
    ),
    (
        "gix-ref",
        "store/file/log/line.rs",
        "write_to",
        "gix_ref::log::Line::write_to",
    ),
    (
        "gix-ref",
        "store/file/transaction/commit.rs",
        "commit",
        "gix_ref::file::Transaction::commit",
    ),
    (
        "gix-tempfile",
        "handle.rs",
        "persist",
        "gix_tempfile::Handle::persist",
    ),
];

/// Each writer site that has no ban, with the reason why it cannot write to Git, as
/// (package, file, function, reason).
const EXPLAINED_SITES: [(&str, &str, &str, &str); 43] = [
    (
        "gix-config",
        "file/access/mutate.rs",
        "remove_section",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/access/mutate.rs",
        "remove_section_by_id",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/access/mutate.rs",
        "remove_section_filter",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/access/mutate.rs",
        "rename_section",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/access/mutate.rs",
        "rename_section_filter",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/access/raw.rs",
        "set_existing_raw_multi_value",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/access/raw.rs",
        "set_existing_raw_multi_value_by",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/access/raw.rs",
        "set_existing_raw_value",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/access/raw.rs",
        "set_existing_raw_value_by",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/access/raw.rs",
        "set_raw_value",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/access/raw.rs",
        "set_raw_value_by",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/access/raw.rs",
        "set_raw_value_filter",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/access/raw.rs",
        "set_raw_value_filter_by",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/access/read_only.rs",
        "set_meta",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/mutable/multi_value.rs",
        "delete",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/mutable/multi_value.rs",
        "delete_all",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/mutable/multi_value.rs",
        "set_all",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/mutable/multi_value.rs",
        "set_at",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/mutable/multi_value.rs",
        "set_string_at",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/mutable/multi_value.rs",
        "set_values",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/mutable/section.rs",
        "remove",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/mutable/section.rs",
        "rename",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/mutable/section.rs",
        "set_implicit_newline",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/mutable/section.rs",
        "set_leading_whitespace",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/mutable/section.rs",
        "set_trust",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/mutable/value.rs",
        "delete",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-config",
        "file/mutable/value.rs",
        "set_string",
        CONFIG_IN_MEMORY,
    ),
    (
        "gix-discover",
        "upwards/mod.rs",
        "probe_repository",
        "tests whether a folder is a repository; it only reads",
    ),
    (
        "gix-error",
        "concrete/metadata.rs",
        "remove",
        "removes a value from the metadata of an error",
    ),
    (
        "gix-hash",
        "oid.rs",
        "write_hex_to",
        "formats an object ID as hex text into the caller's buffer",
    ),
    (
        "gix-hash",
        "prefix.rs",
        "write_hex_to",
        "formats an ID prefix as hex text into the caller's buffer",
    ),
    (
        "gix-object",
        "commit/decode.rs",
        "commit",
        "parses the bytes of a commit object; it writes nothing",
    ),
    ("gix-object", "tree/editor.rs", "remove", TREE_IN_MEMORY),
    (
        "gix-object",
        "tree/editor.rs",
        "remove_if_leaf",
        TREE_IN_MEMORY,
    ),
    (
        "gix-object",
        "tree/editor.rs",
        "remove_leaf",
        TREE_IN_MEMORY,
    ),
    ("gix-object", "tree/editor.rs", "set_root", TREE_IN_MEMORY),
    (
        "gix-odb",
        "cache.rs",
        "set_object_cache",
        "sets the in-memory object cache of a handle",
    ),
    (
        "gix-odb",
        "cache.rs",
        "set_pack_cache",
        "sets the in-memory pack cache of a handle",
    ),
    (
        "gix-odb",
        "memory.rs",
        "set_object_memory",
        "sets the in-memory object map of a proxy",
    ),
    (
        "gix-ref",
        "store/file/mod.rs",
        "set_packed_buffer_mmap_threshold",
        "sets how the store maps packed-refs for reading",
    ),
    (
        "gix-ref",
        "store/packed/transaction.rs",
        "commit",
        "packed::Transaction is private to gix-ref; only the banned file transaction reaches it",
    ),
    (
        "gix-ref",
        "transaction/mod.rs",
        "delete",
        "makes a RefEdit value; only the banned transaction applies it",
    ),
    (
        "gix-ref",
        "transaction/mod.rs",
        "delete_with_log",
        "makes a RefEdit value; only the banned transaction applies it",
    ),
];

/// Further explained sites in packages that gossil cannot name, which only a value reaches.
const EXPLAINED_METHODS: [(&str, &str, &str, &str); 3] = [
    (
        "gix-tempfile",
        "forksafe.rs",
        "persist",
        "a method of a type of the private module forksafe; Handle::persist, which calls it, is banned",
    ),
    (
        "gix-tempfile",
        "lib.rs",
        "remove",
        "removes an entry of the in-memory registry map of the private module hashmap",
    ),
    (
        "gix-trace",
        "forest/printer/mod.rs",
        "writer",
        "returns the writer of trace output, not of a Git file",
    ),
];

/// The reason of the setters of a parsed configuration.
const CONFIG_IN_MEMORY: &str =
    "changes a parsed configuration in memory; only the banned write_to writes it out";

/// The reason of the edits of a tree editor.
const TREE_IN_MEMORY: &str = "edits a tree in memory; only the banned Editor::write stores it";

/// Each public function of the gix crates whose name says that it may write is banned by
/// `clippy.toml`, explained, or a free function of a package that gossil cannot name.
///
/// gossil names only its six gix dependencies and the packages they re-export, such as `gix_pack`
/// through `gix_odb::pack`. A free function of another package, such as `gix_fs::symlink::create`,
/// needs a path that gossil cannot write. A method needs only a value of its type, so each method
/// needs a ban or a reason, in every package.
///
/// The scan is a best-effort guard. It matches names, so it cannot be complete. The guarantees are
/// three other guards. All use of gix sits in `src/git_reader.rs`, the disallowed lists of
/// `clippy.toml` ban the write paths, and the test `git_t06` checks that the Git directories keep
/// their bytes and metadata after a run of every call of the reader.
#[test]
fn every_gix_writer_is_banned_or_explained() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (present, absent) = banned_paths(root);
    let sources = gix_sources(root);
    let nameable = nameable_packages(&sources);
    for package in ["gix-pack", "gix-date"] {
        assert!(nameable.contains(package), "{package} is re-exported");
    }
    for package in ["gix-fs", "gix-tempfile", "gix-lock", "gix-actor"] {
        assert!(!nameable.contains(package), "{package} is not re-exported");
    }
    let sites = writer_sites(&sources);
    assert!(
        sites.len() > 60,
        "the scan finds the writers: {}",
        sites.len()
    );
    let explained: Vec<(&str, &str, &str, &str)> = EXPLAINED_SITES
        .iter()
        .chain(&EXPLAINED_METHODS)
        .copied()
        .collect();
    let mut used = BTreeSet::new();
    for site in &sites {
        if !site.method && !nameable.contains(&site.package) {
            continue;
        }
        let key = (
            site.package.as_str(),
            site.file.as_str(),
            site.function.as_str(),
        );
        let mut covered = false;
        for (index, (package, file, function, path)) in BANNED_SITES.iter().enumerate() {
            if (*package, *file, *function) == key {
                assert!(
                    present.contains(*path) || absent.contains(*path),
                    "{path} is in clippy.toml"
                );
                used.insert(("ban", index));
                covered = true;
            }
        }
        for (index, (package, file, function, reason)) in explained.iter().enumerate() {
            if (*package, *file, *function) == key {
                assert!(!reason.is_empty());
                used.insert(("reason", index));
                covered = true;
            }
        }
        assert!(
            covered,
            "{}/{}: `pub fn {}` may write; ban it in clippy.toml or explain it in tests/git_write_lint.rs",
            site.package, site.file, site.function
        );
    }
    assert_eq!(
        used.len(),
        BANNED_SITES.len() + explained.len(),
        "each entry of the tables names a site that the scan finds"
    );
}

/// Return the numbers of the lines of `text` that name a gix crate as a Rust path.
///
/// A path is `gix::`, or `gix_` and a name followed by `::`, or an `extern crate` of a gix crate.
/// A word such as "gix crates" in prose does not count.
fn gix_path_lines(text: &str) -> Vec<usize> {
    let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let names_gix = |line: &str| {
        let bytes = line.as_bytes();
        line.trim_start().starts_with("extern crate gix")
            || (0..bytes.len()).any(|start| {
                if start > 0 && word(bytes[start - 1]) || !bytes[start..].starts_with(b"gix") {
                    return false;
                }
                let mut end = start + 3;
                if bytes.get(end) == Some(&b'_') {
                    while bytes.get(end).is_some_and(|byte| word(*byte)) {
                        end += 1;
                    }
                }
                bytes[end..].starts_with(b"::")
            })
    };
    text.lines()
        .enumerate()
        .filter(|(_, line)| names_gix(line))
        .map(|(index, _)| index + 1)
        .collect()
}

/// All use of gix sits in `src/git_reader.rs`, so no other source file of the crate names a gix
/// path.
///
/// The test of unchanged directories, `git_t06`, runs the calls of that module only, so a gix call
/// in another module would escape it. A probe text checks that the scan finds each form.
#[test]
fn gix_is_used_only_in_the_reader_module() {
    let probe = "use gix_ref::FullName;\nlet a = gix_odb::at(dir);\n// the gix crates\nlet b = ::gix::open();\nextern crate gix_hash;\nlet c = my_gix_odb::at();\n";
    assert_eq!(gix_path_lines(probe), vec![1, 2, 4, 5]);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let reader = rust_files(&root.join("src"))
        .into_iter()
        .find(|(path, _)| path == "git_reader.rs")
        .expect("the reader module exists");
    assert!(
        !gix_path_lines(&reader.1).is_empty(),
        "the reader module uses gix"
    );
    for (path, text) in rust_files(&root.join("src")) {
        if path == "git_reader.rs" {
            continue;
        }
        assert_eq!(
            gix_path_lines(&text),
            Vec::<usize>::new(),
            "src/{path} names gix"
        );
    }
}
