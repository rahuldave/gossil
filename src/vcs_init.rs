//! Adapt Fossil `init` and `new` commands to native storage creation and admission.
//!
//! ## Recommendations
//! Use `init` for a fresh empty repository. Checkout commands remain future work.
use crate::storage::{InitializeOptions, LocalOwnerRepository};
use libsql::Value;
use std::{ffi::OsString, path::Path};

/// A diagnostic or retained native engine/storage failure.
type Error = Box<dyn std::error::Error>;
/// Exact source option-removal grammar from `main.c:find_option`.
/// # Arguments
/// * `args` - Mutable command suffix; a matched option and value are removed.
/// * `long` - Exact inherited long option name without a hyphen.
/// * `short` - Optional exact short option name.
/// * `value` - Consume the next argument or an equals value when true.
/// # Returns
/// The removed option value or flag spelling, or `None` when no complete match precedes `--`.
fn option(
    args: &mut Vec<OsString>,
    long: &str,
    short: Option<&str>,
    value: bool,
) -> Option<OsString> {
    use std::os::unix::ffi::OsStringExt;
    for i in 0..args.len() {
        let arg = args[i].as_encoded_bytes();
        if arg == b"--" {
            break;
        }
        let Some(name) = arg
            .strip_prefix(b"-")
            .map(|s| s.strip_prefix(b"-").unwrap_or(s))
        else {
            continue;
        };
        if value && name.starts_with(long.as_bytes()) && name.get(long.len()) == Some(&b'=') {
            let result = OsString::from_vec(name[long.len() + 1..].to_vec());
            args.remove(i);
            return Some(result);
        }
        if name == long.as_bytes() || short.is_some_and(|s| name == s.as_bytes()) {
            if value && i + 1 >= args.len() {
                break;
            }
            let result = args.remove(i);
            return Some(if value { args.remove(i) } else { result });
        }
    }
    None
}
/// Reject a non-UTF-8 date expression; native identity/configuration fields stay byte-oriented.
/// # Arguments
/// * `value` - Optional raw command-line value.
/// * `name` - Option name used in a UTF-8 conversion diagnostic.
/// # Returns
/// The optional UTF-8 string.
/// # Errors
/// Rejects an option value that cannot be represented as UTF-8.
fn text(value: Option<OsString>, name: &str) -> Result<Option<String>, Error> {
    value
        .map(|s| {
            s.into_string()
                .map_err(|_| format!("{name} must be UTF-8").into())
        })
        .transpose()
}
/// Generate native identities and password draws with the selected SQLite RNG.
/// Use a private in-memory connection; no preference database is consulted.
/// # Arguments
/// * `source` - Private in-memory connection used for native SQLite randomness.
/// # Returns
/// The project code, local owner identity and initial password sampled with the native alphabet.
/// # Errors
/// Returns SQL, missing randomness or unexpected value-class failures.
pub(crate) async fn entropy(
    source: &libsql::Connection,
) -> Result<(String, String, String), Error> {
    let mut rows = source
        .query(
            "SELECT lower(hex(randomblob(20))),lower(hex(randomblob(20))),randomblob(40)",
            (),
        )
        .await?;
    let row = rows.next().await?.ok_or("native randomness unavailable")?;
    let project: String = row.get(0)?;
    let owner: String = row.get(1)?;
    let password = password_draws(row.get_value(2)?)?;
    Ok((project, owner, password))
}
/// Decode native SQLite password draws using the pinned sampling alphabet.
/// # Errors
/// Returns native-randomness unavailability when SQLite supplies a non-BLOB value.
/// # Arguments
/// * `value` - SQLite random-byte result used by the native password policy.
/// # Returns
/// The initial remote-access password encoded from admitted random draws.
fn password_draws(value: Value) -> Result<String, Error> {
    let Value::Blob(draws) = value else {
        return Err("native randomness unavailable".into());
    };
    // util.c:fossil_random_password samples without replacement from this exact alphabet.
    let mut alphabet = b"23456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ".to_vec();
    let mut password = String::new();
    for draw in draws.chunks_exact(4) {
        let index = u32::from_ne_bytes(draw.try_into().unwrap()) as usize % alphabet.len();
        password.push(alphabet[index] as char);
        alphabet.swap_remove(index);
    }
    Ok(password)
}
/// Select the first nonempty environment login without accessing preferences.
/// # Returns
/// Raw environment bytes in `GOSSIL_USER`, `USER`, `LOGNAME`, `USERNAME` order, or `root`.
fn default_administrator() -> Vec<u8> {
    administrator_from(|key| std::env::var_os(key))
}
/// Keep environment precedence deterministic without process environment mutation.
/// # Arguments
/// * `lookup` - Environment lookup supplied by the caller.
/// # Returns
/// First nonempty login bytes, or `root` when all candidates are absent or empty.
fn administrator_from(mut lookup: impl FnMut(&str) -> Option<OsString>) -> Vec<u8> {
    ["GOSSIL_USER", "USER", "LOGNAME", "USERNAME"]
        .into_iter()
        .filter_map(&mut lookup)
        .find(|value| !value.is_empty())
        .map(|value| value.as_encoded_bytes().to_vec())
        .unwrap_or_else(|| b"root".to_vec())
}
/// Execute native initialization after parsing all source options, before any repository write.
/// # Arguments
/// * `args` - Command suffix after `vcs init` or `vcs new`.
/// # Returns
/// Success after complete native creation and admission.
/// # Errors
/// Returns option, path, native date, engine or admission failures.
pub(super) fn run(args: &[OsString]) -> Result<(), Error> {
    let mut args = args.to_vec();
    if option(&mut args, "help", Some("h"), false).is_some() {
        use std::io::Write;
        writeln!(
            std::io::stdout().lock(),
            "Usage: gossil vcs init ?OPTIONS? FILENAME\n       gossil vcs new ?OPTIONS? FILENAME\n\nCreate an empty repository without a checkout.\n\nOptions:\n  -A, --admin-user USERNAME\n  --date-override DATETIME (default: now)\n  --sha1\n  -h, --help\n  --  End option recognition\n\nThe destination must be new. No preference file is read or created."
        )?;
        return Ok(());
    }
    let date = text(
        option(&mut args, "date-override", None, true),
        "date-override",
    )?
    .unwrap_or_else(|| "now".into());
    let user =
        option(&mut args, "admin-user", Some("A"), true).map(|s| s.as_encoded_bytes().to_vec());
    let sha1 = option(&mut args, "sha1", None, false).is_some();
    let end = args.iter().position(|s| s == "--").unwrap_or(args.len());
    if let Some(arg) = args[..end]
        .iter()
        .find(|s| s.as_encoded_bytes().starts_with(b"-"))
    {
        return Err(format!(
            "unrecognized command-line option or missing argument: {}",
            arg.to_string_lossy()
        )
        .into());
    }
    if end < args.len() {
        args.remove(end);
    }
    if args.len() != 1 {
        return Err("Usage: gossil vcs init ?OPTIONS? FILENAME".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    runtime.block_on(async {
        let path = Path::new(&args[0]);
        if std::fs::symlink_metadata(path).is_ok() {
            return Err(format!("file already exists: {}", path.display()).into());
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        if !std::fs::metadata(parent)?.is_dir() {
            return Err("repository parent is not a directory".into());
        }
        let user = user.unwrap_or_else(default_administrator);
        // This is intentionally SQLite's internal memory database, not a filename.
        let entropy_db = libsql::Builder::new_local(":memory:").build().await?;
        let entropy_connection = entropy_db.connect()?;
        let (project, owner, password) = entropy(&entropy_connection).await?;
        let mut repository = LocalOwnerRepository::initialize(
            Path::new(&args[0]),
            InitializeOptions {
                repository_id: &project,
                owner_id: &owner,
                date: &date,
                user: &user,
                password: &password,
                sha1,
                content_limits: crate::storage::Limits {
                    max_stored_bytes: 1024 * 1024,
                    max_output_bytes: 1024 * 1024,
                    max_chain_depth: 1,
                    max_total_stored_bytes: 1024 * 1024,
                    max_total_decoded_bytes: 1024 * 1024,
                },
                parse_limits: crate::artifact::ParseLimits {
                    input_bytes: 1024 * 1024,
                    cards: 32,
                    owned_bytes: 4 * 1024 * 1024,
                    entries: 32,
                },
            },
        )
        .await?;
        let (project, server) = repository.initialization_identity().await?;
        write_report(
            &mut std::io::stdout().lock(),
            &project,
            &server,
            &user,
            &password,
        )?;
        Ok(())
    })
}
/// Write the committed initialization receipt while preserving I/O failures.
/// # Errors
/// Returns the original writer error, including after partial output; reporting
/// failure does not undo the already committed initialization.
/// # Arguments
/// * `output` - Receipt destination, which may fail after receiving a prefix.
/// * `project` - Committed native project identifier.
/// * `server` - Committed native server identifier.
/// * `user` - Exact administrator-name bytes.
/// * `password` - Generated initial remote-access password.
/// # Returns
/// Unit after writing the complete committed initialization receipt.
fn write_report(
    output: &mut impl std::io::Write,
    project: &str,
    server: &str,
    user: &[u8],
    password: &str,
) -> std::io::Result<()> {
    write!(
        output,
        "project-id: {project}\nserver-id:  {server}\nadmin-user: "
    )?;
    output.write_all(user)?;
    writeln!(
        output,
        " (initial remote-access password is \"{password}\")"
    )?;
    Ok(())
}
/// Check native repository invariants through ordinary bounded fixtures.
#[cfg(test)]
mod tests {
    /// Empty candidates fall through in order and raw environment bytes are preserved.
    #[test]
    fn administrator_environment_precedence_and_bytes() {
        use std::os::unix::ffi::OsStringExt;
        let keys = ["GOSSIL_USER", "USER", "LOGNAME", "USERNAME"];
        for start in 0..=keys.len() {
            let actual = administrator_from(|key| {
                keys.iter()
                    .position(|name| *name == key)
                    .filter(|index| *index >= start)
                    .map(|_| OsString::from(key))
            });
            assert_eq!(
                actual,
                keys.get(start).copied().unwrap_or("root").as_bytes()
            );
        }
        assert_eq!(administrator_from(|_| Some(OsString::new())), b"root");
        assert_eq!(
            administrator_from(|key| (key == "USER").then(|| OsString::from_vec(vec![0xff]))),
            [0xff]
        );
    }
    /// Native NUL filenames fail with the original OS cause still in the storage chain.
    #[test]
    fn nul_path_retains_original_cause() {
        use std::os::unix::ffi::OsStringExt;
        let error = run(&[OsString::from_vec(b"bad\0name".to_vec())]).unwrap_err();
        let mut cause: &(dyn std::error::Error + 'static) = error.as_ref();
        let mut found = false;
        loop {
            if let Some(io) = cause.downcast_ref::<std::io::Error>() {
                found |= io.kind() == std::io::ErrorKind::InvalidInput;
            }
            match cause.source() {
                Some(next) => cause = next,
                None => break,
            }
        }
        assert!(found, "{error}");
    }
    /// Receipt failures retain exact preceding bytes and the writer's I/O kind.
    #[test]
    fn report_writer_errors_preserve_prefix() {
        let mut expected = Vec::new();
        write_report(&mut expected, "project", "server", b"admin", "password").unwrap();
        for limit in [0, expected.len() - 2] {
            let mut bytes = vec![0; limit];
            let mut writer = bytes.as_mut_slice();
            let error =
                write_report(&mut writer, "project", "server", b"admin", "password").unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::WriteZero);
            assert!(writer.is_empty());
            assert_eq!(bytes, expected[..limit]);
        }
    }
    /// Native draws sample without replacement and reject non-blob projections.
    #[test]
    fn native_password_draw_decoder() {
        assert_eq!(
            password_draws(Value::Blob(vec![0; 40])).unwrap(),
            "2ZYXWVUTSR"
        );
        assert_eq!(
            password_draws(Value::Integer(40)).unwrap_err().to_string(),
            "native randomness unavailable"
        );
    }
    use super::*;
    /// Preserve single/double hyphens, equals, source-order removal and the `--` boundary.
    #[test]
    fn native_option_removal_grammar() {
        let mut args = [
            "repo",
            "-date-override=source",
            "--admin-user",
            "a",
            "--sha1",
            "--",
            "--admin-user",
            "literal",
        ]
        .map(OsString::from)
        .to_vec();
        assert_eq!(
            option(&mut args, "date-override", None, true),
            Some("source".into())
        );
        assert_eq!(
            option(&mut args, "admin-user", Some("A"), true),
            Some("a".into())
        );
        assert!(option(&mut args, "sha1", None, false).is_some());
        assert!(option(&mut args, "admin-user", None, true).is_none());
        assert_eq!(
            args,
            ["repo", "--", "--admin-user", "literal"].map(OsString::from)
        );
        let mut repeated = ["--sha1", "--sha1"].map(OsString::from).to_vec();
        assert!(option(&mut repeated, "sha1", None, false).is_some());
        assert_eq!(repeated, [OsString::from("--sha1")]);
        let mut missing = vec![OsString::from("-A")];
        assert!(option(&mut missing, "admin-user", Some("A"), true).is_none());
        assert_eq!(missing, [OsString::from("-A")]);
    }
    /// Non-UTF-8 filesystem arguments do not stop later option recognition.
    #[test]
    fn native_paths_remain_os_strings() {
        use std::os::unix::ffi::OsStringExt;
        let mut args = vec![OsString::from_vec(vec![0xff]), "--admin-user=x".into()];
        assert_eq!(
            option(&mut args, "admin-user", None, true),
            Some("x".into())
        );
        let mut raw = vec![OsString::from_vec(b"--admin-user=raw\xff".to_vec())];
        assert_eq!(
            option(&mut raw, "admin-user", None, true)
                .unwrap()
                .as_encoded_bytes(),
            b"raw\xff"
        );

        assert!(text(Some(OsString::from_vec(vec![0xff])), "field").is_err());
    }
}
