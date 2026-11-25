//! Check `issue init` through both executables and both group names.
//!
//! The cases are CAT-T06 for the order `v init`, `v open`, `i init`, the part of
//! CAT-T01 that `i init` meets in this build, the CLI-01 forms, the CLI-05 exit
//! statuses of the refusals, and `vcs` commands before and after `init`. An ignored
//! oracle checks that stock Fossil 2.29 reads a repository after `i init`.
//!
//! ## Recommendations
//! Each case runs in its own home, so no locator or configuration leaks between cases.
use libsql::Value;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::process::Output;

/// The refusal outside an open checkout.
const NOT_OPEN: &str =
    "gossil: no open checkout holds this directory; run gos v init and gos v open first\n";

/// The four executable and group forms of CLI-01.
fn forms() -> [(&'static str, &'static str); 4] {
    [
        (env!("CARGO_BIN_EXE_gossil"), "issue"),
        (env!("CARGO_BIN_EXE_gossil"), "i"),
        (env!("CARGO_BIN_EXE_gos"), "issue"),
        (env!("CARGO_BIN_EXE_gos"), "i"),
    ]
}

/// Run one executable in a directory of a fixture home, with an isolated environment.
/// # Arguments
/// * `binary` - The executable.
/// * `home` - The fixture home, which is also the configuration and data directory.
/// * `cwd` - The working directory.
/// * `args` - The arguments after the executable.
/// * `env` - Extra environment variables.
/// # Returns
/// The finished process output.
fn run_env(
    binary: &str,
    home: &Path,
    cwd: &Path,
    args: &[&OsStr],
    env: &[(&str, OsString)],
) -> Output {
    let mut command = std::process::Command::new(binary);
    for (key, _) in std::env::vars_os() {
        if key.to_str().is_some_and(|key| key.starts_with("GEST_")) {
            command.env_remove(key);
        }
    }
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_DATA_HOME", home)
        .env("GOSSIL_USER", "tester")
        .envs(env.iter().map(|(key, value)| (key, value)))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap()
}

/// Run `gos` in the checkout root of a fixture home.
/// # Arguments
/// * `home` - The fixture home, which holds the root `root`.
/// * `args` - The arguments after the executable.
/// # Returns
/// The finished process output.
fn gos(home: &Path, args: &[&str]) -> Output {
    let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
    run_env(
        env!("CARGO_BIN_EXE_gos"),
        home,
        &home.join("root"),
        &args,
        &[],
    )
}

/// Run one `vcs` verb on the fixture repository and checkout, and require success.
/// # Arguments
/// * `home` - The fixture home.
/// * `verb` - The verb and its own flags.
/// # Returns
/// Standard output.
fn vcs(home: &Path, verb: &[&str]) -> String {
    let repo = home.join("repo");
    let mut args = vec![
        "v",
        verb[0],
        "--repository",
        repo.to_str().unwrap(),
        "--checkout",
        "work",
    ];
    args.extend(&verb[1..]);
    let output = gos(home, &args);
    assert!(
        output.status.success(),
        "{verb:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Make a repository with `gos v init`, and an empty checkout root `root`.
/// # Arguments
/// * `home` - An empty fixture home.
/// # Returns
/// The canonical checkout root.
fn new_repository(home: &Path) -> PathBuf {
    let root = home.join("root");
    std::fs::create_dir(&root).unwrap();
    let init = gos(home, &["v", "init", home.join("repo").to_str().unwrap()]);
    assert!(init.status.success());
    root.canonicalize().unwrap()
}

/// Make a repository and register the checkout `work` at `root` with `gos v open`.
/// # Arguments
/// * `home` - An empty fixture home.
/// # Returns
/// The canonical checkout root.
fn open_checkout(home: &Path) -> PathBuf {
    let root = new_repository(home);
    vcs(home, &["open", "--root", root.to_str().unwrap()]);
    root
}

/// Add, scope and commit one new file with the CLI.
/// # Arguments
/// * `home` - The fixture home.
/// * `n` - The number of the commit, which names the file and the date.
fn commit(home: &Path, n: usize) {
    let file = format!("file{n}");
    std::fs::write(home.join("root").join(&file), format!("content {n}")).unwrap();
    vcs(home, &["add", "--path", &file]);
    vcs(home, &["scope", "--scope", "private", "--path", &file]);
    vcs(
        home,
        &[
            "commit",
            "--scope",
            "private",
            "--all-tracked",
            "--user",
            "tester",
            "--comment",
            &format!("commit {n}"),
            "--date-override",
            &format!("2099-01-01T00:00:{n:02}"),
        ],
    );
}

/// Read the rows of one query on a separate connection.
/// # Arguments
/// * `path` - The repository file.
/// * `sql` - A fixed query.
/// # Returns
/// Every row.
fn rows(path: &Path, sql: &str) -> Vec<Vec<Value>> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async {
            let database = libsql::Builder::new_local(path).build().await.unwrap();
            let connection = database.connect().unwrap();
            let mut rows = connection.query(sql, ()).await.unwrap();
            let mut values = Vec::new();
            while let Some(row) = rows.next().await.unwrap() {
                values.push(
                    (0..row.column_count())
                        .map(|index| row.get_value(index).unwrap())
                        .collect(),
                );
            }
            values
        })
}

/// Run fixture SQL on a separate connection.
/// # Arguments
/// * `path` - The repository file.
/// * `sql` - Fixture statements.
fn execute(path: &Path, sql: &str) {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async {
            let database = libsql::Builder::new_local(path).build().await.unwrap();
            database
                .connect()
                .unwrap()
                .execute_batch(sql)
                .await
                .unwrap();
        });
}

/// Read the bytes of a repository file and its write-ahead log.
/// # Arguments
/// * `path` - The repository file.
/// # Returns
/// The bytes of both files. An absent log reads as empty.
fn file_bytes(path: &Path) -> (Vec<u8>, Vec<u8>) {
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    (
        std::fs::read(path).unwrap(),
        std::fs::read(wal).unwrap_or_default(),
    )
}

/// Store one private project through the library, without a binding.
/// # Arguments
/// * `repo` - The repository file, with no open owner.
fn store_project(repo: &Path) {
    use gossil::gest_project_values::{ProjectValueInputs, project_from_values};
    use gossil::storage::{IssueLimits, LocalOwnerRepository, PathScope, PrivateIssueRequest};
    let stamp = serde_json::json!("2026-11-18T12:00:00Z");
    let project = project_from_values(ProjectValueInputs {
        gest_id: serde_json::json!("p".repeat(32)),
        archived_at: serde_json::Value::Null,
        created_at: stamp.clone(),
        updated_at: stamp,
    })
    .unwrap();
    let date = gossil::native_date::NativeDate::parse(b"2026-11-18T12:00:00Z").unwrap();
    let artifact =
        gossil::gest_ticket::project_creation_bytes(&project, &[b'0'; 40], b"tester", date, 65_536)
            .unwrap();
    let limits = IssueLimits {
        admission: gossil::storage::AdmissionLimits {
            content: gossil::storage::Limits {
                max_stored_bytes: 1 << 20,
                max_output_bytes: 1 << 20,
                max_chain_depth: 128,
                max_total_stored_bytes: 64 << 20,
                max_total_decoded_bytes: 64 << 20,
            },
            parser: gossil::artifact::ParseLimits {
                input_bytes: 1 << 20,
                cards: 4096,
                owned_bytes: 8 << 20,
                entries: 4096,
            },
        },
        sql_rows: 100_000,
        sql_bytes: 64 << 20,
    };
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(async {
            let mut owner = LocalOwnerRepository::open(repo, limits.admission)
                .await
                .unwrap();
            owner
                .create_private_issue(
                    PrivateIssueRequest {
                        artifact: &artifact,
                        scope: PathScope::Private,
                    },
                    limits,
                )
                .await
                .unwrap();
        });
}

/// The text of one value.
/// # Arguments
/// * `value` - A TEXT value.
/// # Returns
/// Its text.
fn text(value: &Value) -> &str {
    match value {
        Value::Text(text) => text,
        other => panic!("not text: {other:?}"),
    }
}

/// Return the Gest ID of the one project and check the two binding rows that name it.
/// # Arguments
/// * `repo` - The repository file.
/// * `root` - The canonical checkout root.
/// # Returns
/// The 32-letter Gest ID.
fn bound_project(repo: &Path, root: &Path) -> String {
    let projects = rows(repo, "SELECT id,archived_at FROM g_gest_projects");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0][1], Value::Null);
    let id = text(&projects[0][0]).to_owned();
    let work = Value::Text("work".into());
    let project = Value::Text(id.clone());
    assert_eq!(
        rows(repo, "SELECT * FROM g_project_binding"),
        [vec![
            work.clone(),
            project.clone(),
            Value::Text(root.to_str().unwrap().into())
        ]]
    );
    assert_eq!(
        rows(repo, "SELECT * FROM g_checkout_project"),
        [vec![work, project]]
    );
    id
}

/// The success message of `init`, as Gest prints it without color.
/// # Arguments
/// * `id` - The Gest ID of the project.
/// * `root` - The checkout root.
/// # Returns
/// The expected standard output.
fn initialized(id: &str, root: &Path) -> String {
    format!(
        "  ✓  initialized project  {}\n\n  root  {}\n",
        &id[..8],
        root.display()
    )
}

#[test]
/// CAT-T06 and CLI-01: after `v init` and `v open`, each of the four forms of
/// `init` creates one private project and binds the checkout to it. The root keeps
/// its one locator, unchanged, and both groups resolve the same repository and
/// checkout IDs. The repository stays at catalog 6 (CAT-T01).
fn each_form_binds_the_open_checkout() {
    for (binary, group) in forms() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let root = open_checkout(home);
        let repo = home.join("repo");
        let locator = std::fs::read(root.join(".gossil/checkout.json")).unwrap();
        let out = run_env(binary, home, &root, &[group.as_ref(), "init".as_ref()], &[]);
        assert_eq!(out.status.code(), Some(0), "{binary} {group}");
        assert!(out.stderr.is_empty(), "{binary} {group}");
        let id = bound_project(&repo, &root);
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            initialized(&id, &root)
        );
        let marker: Vec<_> = std::fs::read_dir(root.join(".gossil"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(marker, ["checkout.json"]);
        assert_eq!(
            std::fs::read(root.join(".gossil/checkout.json")).unwrap(),
            locator
        );
        let value: serde_json::Value = serde_json::from_slice(&locator).unwrap();
        let reopened = vcs(home, &["open"]);
        assert!(
            reopened.contains(&format!(
                "(repository {}, head",
                value["repository_id"].as_str().unwrap()
            )),
            "{reopened}"
        );
        assert_eq!(value["checkout_id"], "work");
        let ticket = rows(
            &repo,
            "SELECT t.gossil_kind,t.gest_id,c.tkt_user FROM ticket t JOIN ticketchng c USING(tkt_id)",
        );
        assert_eq!(
            ticket,
            [vec![
                Value::Text("project".into()),
                Value::Text(format!("\"{id}\"")),
                Value::Text("tester".into())
            ]]
        );
        assert_eq!(
            rows(&repo, "SELECT count(*) FROM private"),
            [vec![Value::Integer(1)]]
        );
        assert_eq!(
            rows(&repo, "SELECT storage_version FROM g_storage"),
            [vec![Value::Integer(6)]]
        );
    }
}

#[test]
/// A second `init`, in the root or in a directory below it, prints the bound project
/// and changes no byte of the repository, as Gest reuses the project of a directory.
/// The accepted options change nothing either.
fn a_second_init_reuses_the_binding() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = open_checkout(home);
    let repo = home.join("repo");
    let first = gos(home, &["i", "init"]);
    assert!(first.status.success());
    let id = bound_project(&repo, &root);
    let files = file_bytes(&repo);
    let below = root.join("a/b");
    std::fs::create_dir_all(&below).unwrap();
    for (cwd, args) in [
        (&root, &["i", "init"][..]),
        (&below, &["issue", "init"]),
        (
            &root,
            &[
                "i",
                "init",
                "--local",
                "--no-color",
                "--no-pager",
                "-vv",
                "--verbose",
            ],
        ),
    ] {
        let args: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        let out = run_env(env!("CARGO_BIN_EXE_gos"), home, cwd, &args, &[]);
        assert_eq!(out.status.code(), Some(0), "{args:?}");
        assert_eq!(out.stdout, first.stdout, "{args:?}");
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            initialized(&id, &root)
        );
        assert_eq!(file_bytes(&repo), files, "{args:?}");
    }
    assert_eq!(bound_project(&repo, &root), id);
}

#[test]
/// Every `vcs` command still works after `init`, and `init` works before and after
/// the first commit. The timeline lists the shared root and each check-in.
fn vcs_commands_work_before_and_after_init() {
    for init_after in [0, 1] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let root = open_checkout(home);
        for n in 1..=2 {
            if n == init_after + 1 {
                let out = gos(home, &["i", "init"]);
                assert!(
                    out.status.success(),
                    "{init_after}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            commit(home, n);
            let status = vcs(home, &["status"]);
            assert!(
                status.contains(&format!("R\tprivate\t\"file{n}\"")),
                "{status}"
            );
        }
        assert_eq!(vcs(home, &["timeline"]).lines().count(), 3);
        assert_eq!(vcs(home, &["log"]).lines().count(), 3);
        bound_project(&home.join("repo"), &root);
        assert!(gos(home, &["i", "init"]).status.success());
    }
}

#[test]
/// `init` outside an open checkout refuses with status 78 and the message of the
/// plan. The cases are a directory with no marker, the state after `v init`
/// alone, and a marker directory that holds no locator. It writes no file and
/// changes no byte.
fn init_outside_an_open_checkout_refuses_with_78() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let empty = home.join("empty");
    std::fs::create_dir(&empty).unwrap();
    for (binary, group) in forms() {
        let out = run_env(
            binary,
            home,
            &empty,
            &[group.as_ref(), "init".as_ref()],
            &[],
        );
        assert_eq!(out.status.code(), Some(78));
        assert!(out.stdout.is_empty());
        assert_eq!(String::from_utf8(out.stderr).unwrap(), NOT_OPEN);
    }
    assert_eq!(std::fs::read_dir(&empty).unwrap().count(), 0);
    new_repository(home);
    let repo = home.join("repo");
    let files = file_bytes(&repo);
    let out = gos(home, &["i", "init"]);
    assert_eq!(out.status.code(), Some(78));
    assert_eq!(String::from_utf8(out.stderr).unwrap(), NOT_OPEN);
    assert_eq!(std::fs::read_dir(home.join("root")).unwrap().count(), 0);
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(home.join("root/.gossil"))
            .unwrap();
    }
    let out = gos(home, &["i", "init"]);
    assert_eq!(out.status.code(), Some(78));
    assert_eq!(String::from_utf8(out.stderr).unwrap(), NOT_OPEN);
    assert_eq!(file_bytes(&repo), files);
}

#[test]
/// CLI-05: each refusal on an open checkout exits with its sysexits status and
/// changes no byte of the repository. A malformed locator, a configured data
/// directory that conflicts with the locator, an older catalog and a login that is
/// not UTF-8 give 78. A tampered binding, a missing repository and a project that
/// no checkout binds give 74. An argument that `init` does not take gives 2.
fn refusals_on_an_open_checkout_keep_every_byte() {
    let older = "PRAGMA ignore_check_constraints=ON; UPDATE g_storage SET storage_version=4";
    let cases: [(&str, u8, &str); 8] = [
        ("argument", 2, "gossil: unexpected argument \"--json\""),
        ("locator", 78, "invalid or incomplete checkout locator"),
        (
            "configured",
            78,
            "configured storage conflicts with explicit repository",
        ),
        (
            "older",
            78,
            "is at catalog 4, and this build needs catalog 6.",
        ),
        (
            "login",
            78,
            "gossil: the local login is not UTF-8; set GOSSIL_USER",
        ),
        ("binding", 74, "unsupported project binding state"),
        ("missing", 74, "No such file or directory"),
        (
            "unbound",
            74,
            "binding an existing project is not available in this build",
        ),
    ];
    for (case, status, message) in cases {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let root = open_checkout(home);
        let repo = home.join("repo");
        let mut args = vec![OsString::from("i"), OsString::from("init")];
        let mut env = Vec::new();
        match case {
            "argument" => args.push("--json".into()),
            "locator" => {
                use std::os::unix::fs::PermissionsExt;
                let path = root.join(".gossil/checkout.json");
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
                std::fs::write(&path, b"{}").unwrap();
            }
            "configured" => {
                let other = home.join("other");
                std::fs::create_dir(&other).unwrap();
                std::fs::write(other.join("gossil.db"), b"").unwrap();
                env.push(("GEST_STORAGE__DATA_DIR", other.into_os_string()));
            }
            "older" => execute(&repo, older),
            "login" => env.push(("GOSSIL_USER", OsString::from_vec(vec![0xff]))),
            "binding" => {
                assert!(gos(home, &["i", "init"]).status.success());
                execute(&repo, "UPDATE g_project_binding SET root=root||'/x'");
            }
            "unbound" => store_project(&repo),
            _ => std::fs::rename(&repo, home.join("moved")).unwrap(),
        }
        let kept = if case == "missing" {
            home.join("moved")
        } else {
            repo.clone()
        };
        let files = file_bytes(&kept);
        let args: Vec<&OsStr> = args.iter().map(OsString::as_os_str).collect();
        let out = run_env(env!("CARGO_BIN_EXE_gos"), home, &root, &args, &env);
        assert_eq!(out.status.code(), Some(i32::from(status)), "{case}");
        assert!(out.stdout.is_empty(), "{case}");
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert!(stderr.contains(message), "{case}: {stderr}");
        assert_eq!(file_bytes(&kept), files, "{case}");
        if case != "binding" {
            assert!(
                rows(&kept, "SELECT * FROM g_project_binding").is_empty(),
                "{case}"
            );
        }
    }
}

#[test]
/// A configured data directory that holds no `gossil.db` names other storage, so it
/// conflicts with the locator and gives 78, as GEST-ROOT-01 asks.
fn an_empty_configured_data_directory_conflicts() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = open_checkout(home);
    let other = home.join("other");
    std::fs::create_dir(&other).unwrap();
    let files = file_bytes(&home.join("repo"));
    let out = run_env(
        env!("CARGO_BIN_EXE_gos"),
        home,
        &root,
        &["i".as_ref(), "init".as_ref()],
        &[("GEST_STORAGE__DATA_DIR", other.into_os_string())],
    );
    assert_eq!(out.status.code(), Some(78));
    assert_eq!(
        String::from_utf8(out.stderr).unwrap(),
        "configured storage conflicts with explicit repository\n"
    );
    assert_eq!(file_bytes(&home.join("repo")), files);
}

#[test]
/// When stdout is closed, `init` keeps the committed project, says so on stderr and
/// exits 74, without a panic.
fn a_closed_stdout_keeps_the_project_and_exits_74() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = open_checkout(home);
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_gos"))
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_DATA_HOME", home)
        .env("GOSSIL_USER", "tester")
        .current_dir(&root)
        .args(["i", "init"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(74));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.starts_with("gossil: issue init kept the project, and stdout failed: "),
        "{stderr}"
    );
    bound_project(&home.join("repo"), &root);
}

#[test]
#[ignore = "requires explicit pinned native Fossil; run just issue-ticket-interop"]
/// Stock Fossil 2.29 checks a repository after `v init`, `v open`, `i init` and a
/// commit. `test-integrity` finds no error, and `ticket show` gives the fields of the
/// project ticket.
fn stock_fossil_reads_a_repository_after_issue_init() {
    let fossil =
        std::env::var_os("GOSSIL_FOSSIL_REFERENCE").expect("explicit reference executable");
    let run = |args: &[&OsStr]| {
        let output = std::process::Command::new("jlog")
            .arg("run")
            .arg(&fossil)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "fossil {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let root = open_checkout(home);
    assert!(gos(home, &["i", "init"]).status.success());
    commit(home, 1);
    let repo = home.join("repo");
    let id = bound_project(&repo, &root);
    let r = OsStr::new("-R");
    let integrity = run(&[
        OsStr::new("test-integrity"),
        OsStr::new("--parse"),
        r,
        repo.as_os_str(),
    ]);
    assert!(integrity.contains("0 errors"), "{integrity}");
    let k = rows(&repo, "SELECT tkt_uuid FROM ticket");
    let shown = run(&[
        OsStr::new("ticket"),
        OsStr::new("show"),
        OsStr::new("0"),
        OsStr::new(&format!("tkt_uuid='{}'", text(&k[0][0]))),
        OsStr::new("--quote"),
        OsStr::new("--user"),
        OsStr::new("tester"),
        r,
        repo.as_os_str(),
    ]);
    let mut lines = shown.lines();
    let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
    let row: Vec<&str> = lines.next().unwrap().split('\t').collect();
    let shown: std::collections::BTreeMap<&str, &str> = header.into_iter().zip(row).collect();
    assert_eq!(shown["gossil_kind"], "project");
    assert_eq!(shown["gest_id"], format!("\"{id}\""));
    assert_eq!(shown["archived_at"], "null");
}
