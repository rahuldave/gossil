//! The script `scripts/git-reader-packages.py` of `just git-reader-packages` counts the packages
//! that the six gix crates add, and fails when it finds none.
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

/// Run Python through `uv` in the repository root with `args`, or panic with a not-run status.
fn python(root: &Path, args: &[&str]) -> Output {
    let output = Command::new("uv")
        .args(["run", "--no-project", "--quiet", "python"])
        .args(args)
        .current_dir(root)
        .output();
    match output {
        Ok(output) => output,
        Err(error) => panic!("not-run: uv is needed to run the package script: {error}"),
    }
}

/// The script lists the six gix crates of Cargo.toml, prints the added packages and their count,
/// and succeeds.
#[test]
fn package_script_counts_the_added_packages() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let script = fs::read_to_string(root.join("scripts/git-reader-packages.py")).unwrap();
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let crates: Vec<&str> = manifest
        .lines()
        .filter(|line| line.starts_with("gix-"))
        .map(|line| line.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(crates.len(), 6);
    for name in &crates {
        assert!(
            script.contains(&format!("\"{name}\"")),
            "the script prunes {name}"
        );
    }

    let output = python(root, &["scripts/git-reader-packages.py"]);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        output.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lines: Vec<&str> = stdout.lines().collect();
    let summary = lines.last().unwrap();
    let words: Vec<&str> = summary.split_whitespace().collect();
    assert_eq!(words[..2], ["added", "packages:"], "{summary}");
    let added: usize = words[2].parse().unwrap();
    let total: usize = words[4].parse().unwrap();
    assert_eq!(added, lines.len() - 1, "one line per added package");
    assert!(0 < added && added < total, "{summary}");
    for name in &crates {
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with(&format!("{name} "))),
            "{name} is added"
        );
    }
    assert!(
        !lines.iter().any(|line| line.starts_with("libsql ")),
        "libsql is not added by gix"
    );
}

/// The script fails, with a count of zero, when the pruned run has every package of the full run.
#[test]
fn package_script_fails_when_nothing_is_added() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let check = "import importlib.util, sys\n\
        spec = importlib.util.spec_from_file_location('m', 'scripts/git-reader-packages.py')\n\
        m = importlib.util.module_from_spec(spec)\n\
        spec.loader.exec_module(m)\n\
        sys.exit(10 + m.main(lambda prune: {'a 1', 'b 2'}))\n";
    let output = python(root, &["-c", check]);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(output.status.code(), Some(11), "{stdout}");
    assert_eq!(stdout, "added packages: 0 of 2 on the host target\n");
}
