//! Pinned Gest configuration layering with the adopted Gossil repository namespace.
//! Adapted from Gest eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8.
//!
//! ## Recommendations
//! Load inherited configuration before filesystem or database mutation, including explicit opens.
/*
Gest source attribution

Upstream: https://github.com/aaronmallen/gest
Adaptation source: https://github.com/rahuldave/gest
Pinned source: eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8

The inherited CLI declarations/oracle fixtures and adapted configuration/output
mechanisms retain the following upstream notice and permission.

The MIT License (MIT)

Copyright (c) 2026 Aaron Allen

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
*/
use crate::checkout_locator::Error;
use std::path::{Path, PathBuf};
use toml::{Value, value::Table};
/// Effective selection inputs, with explicit override distinguished from defaults.
pub(crate) struct Config {
    /// Usable explicit absolute data directory.
    pub explicit: Option<PathBuf>,
}
/// Merge source layers recursively, as the pinned Gest loader does.
/// # Arguments
/// * `base` - Effective table updated in place with each source layer.
/// * `overlay` - Later source table whose values override or recursively merge with the base.
/// # Returns
/// No value; the base table retains the merged source values.
fn merge(base: &mut Table, overlay: Table) {
    for (key, value) in overlay {
        if let Value::Table(table) = &value
            && let Some(Value::Table(existing)) = base.get_mut(&key)
        {
            merge(existing, table.clone());
        } else {
            base.insert(key, value);
        }
    }
}
/// Load one existing source TOML layer without creating anything.
/// # Arguments
/// * `path` - Optional existing TOML configuration filename.
/// * `base` - Effective table that receives the parsed layer.
/// # Returns
/// Success after merging an existing file, or without changes when the path is not a file.
/// # Errors
/// Return typed I/O or TOML causes under safe configuration diagnostics when an existing layer cannot be read or parsed.
fn read(path: &Path, base: &mut Table) -> Result<(), Error> {
    if path.is_file() {
        let text = std::fs::read_to_string(path)
            .map_err(|error| Error::config_source("cannot read Gest configuration", error))?;
        merge(
            base,
            toml::from_str(&text)
                .map_err(|error| Error::config_source("invalid Gest configuration TOML", error))?,
        );
    }
    Ok(())
}
/// Apply inherited double-underscore environment nesting, returning errors instead of panics.
/// # Arguments
/// * `base` - Effective configuration table updated in place.
/// * `key` - Environment suffix after GEST_, with double underscores separating nested keys.
/// * `value` - UTF-8 environment value retained as a TOML string.
/// # Returns
/// Success after inserting the value under lowercase nested keys.
/// # Errors
/// Reject nested environment keys when an intermediate value is a scalar instead of a table.
fn environment(base: &mut Table, key: &str, value: String) -> Result<(), Error> {
    let parts = key.split("__").map(str::to_lowercase).collect::<Vec<_>>();
    let mut table = base;
    for part in &parts[..parts.len() - 1] {
        table = table
            .entry(part.clone())
            .or_insert_with(|| Value::Table(Table::new()))
            .as_table_mut()
            .ok_or_else(|| {
                Error::config("configuration environment nesting conflicts with scalar")
            })?;
    }
    table.insert(parts.last().unwrap().clone(), Value::String(value));
    Ok(())
}
/// Resolve source layers and explicit path precedence before marker or database mutation.
/// # Arguments
/// * `cwd` - Canonical invocation directory whose ancestors supply local configuration layers.
/// # Returns
/// Effective configuration with the usable explicit absolute data directory, if one remains after all layers.
/// # Errors
/// Reject unavailable global configuration location, layer read/parse failures, invalid value types, non-UTF-8 environment values and configured database URL/host routes.
pub(crate) fn load(cwd: &Path) -> Result<Config, Error> {
    let mut table = Table::new();
    let global = std::env::var_os("GEST_CONFIG")
        .map(PathBuf::from)
        .or_else(|| dir_spec::config_home().map(|p| p.join("gest/config.toml")))
        .ok_or_else(|| Error::config("cannot resolve config directory"))?;
    read(&global, &mut table)?;
    let mut ancestors = cwd.ancestors().collect::<Vec<_>>();
    ancestors.reverse();
    for path in ancestors {
        for name in [".config/gest.toml", ".gest.toml"] {
            read(&path.join(name), &mut table)?;
        }
    }
    // A usable exact-uppercase native absolute path has explicit priority without UTF-8 loss.
    let native = std::env::var_os("GEST_STORAGE__DATA_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute());
    for (key, value) in std::env::vars_os() {
        let Some(key) = key.to_str() else { continue };
        if !key.starts_with("GEST_") || key == "GEST_CONFIG" {
            continue;
        }
        if key == "GEST_STORAGE__DATA_DIR" && value.to_str().is_none() {
            // The adopted native-byte extension cannot be represented in TOML. Preserve its
            // raw absolute selection above; an unusable relative value still replaces the
            // earlier effective field rather than reviving a pre-environment file snapshot.
            let storage = table
                .entry("storage")
                .or_insert_with(|| Value::Table(Table::new()))
                .as_table_mut()
                .ok_or_else(|| Error::config("storage configuration must be a table"))?;
            storage.remove("data_dir");
            continue;
        }
        let value = value
            .into_string()
            .map_err(|_| Error::config("configuration environment value is not UTF-8"))?;
        environment(&mut table, &key[5..], value)?;
    }
    if table.get("storage").is_some_and(|value| !value.is_table()) {
        return Err(Error::config("storage configuration must be a table"));
    }
    let configured = table
        .get("storage")
        .and_then(Value::as_table)
        .and_then(|storage| storage.get("data_dir"))
        .filter(|_| native.is_none())
        .map(|value| {
            value
                .as_str()
                .map(PathBuf::from)
                .ok_or_else(|| Error::config("storage.data_dir must be a path string"))
        })
        .transpose()?;
    if let Some(database) = table.get("database") {
        let database = database
            .as_table()
            .ok_or_else(|| Error::config("database configuration must be a table"))?;
        for key in ["url", "host"] {
            if let Some(value) = database.get(key) {
                if !value.is_str() {
                    return Err(Error::config("database route must be a string"));
                }
                return Err(Error::config(
                    "configured database URL/host provider is unsupported",
                ));
            }
        }
    }
    let explicit = native.or_else(|| configured.filter(|p| p.is_absolute()));
    Ok(Config { explicit })
}
/// Test pure merge/environment contracts without changing process environment.
#[cfg(test)]
mod tests {
    use super::*;
    /// Source layers and case-insensitive nesting retain highest-precedence values.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn merge_and_nested_environment_preserve_source_order() {
        let mut table = toml::from_str::<Table>("[storage]\ndata_dir='/first'\n").unwrap();
        merge(
            &mut table,
            toml::from_str("[storage]\ndata_dir='/second'\n").unwrap(),
        );
        environment(&mut table, "STORAGE__DATA_DIR", "/third".into()).unwrap();
        assert_eq!(table["storage"]["data_dir"].as_str(), Some("/third"));
        table.insert("database".into(), Value::String("scalar".into()));
        assert!(environment(&mut table, "DATABASE__HOST", "host".into()).is_err());
    }
    /// Native-byte environment paths retain exact bytes, and relative overrides never revive file selection.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn native_byte_environment_priority_and_relative_ignore_survive_fresh_process() {
        use std::os::unix::ffi::OsStringExt;
        for absolute in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let config = temp.path().join("gest.toml");
            std::fs::write(
                &config,
                "[storage]\ndata_dir='/file-selection-must-not-revive'\n",
            )
            .unwrap();
            let mut bytes = if absolute {
                temp.path().as_os_str().as_encoded_bytes().to_vec()
            } else {
                b"relative".to_vec()
            };
            bytes.extend_from_slice(b"/native-\xff");
            let native = std::ffi::OsString::from_vec(bytes);
            let mut child = std::process::Command::new(std::env::current_exe().unwrap());
            child
                .args([
                    "--exact",
                    "checkout_config::tests::native_environment_child",
                    "--nocapture",
                ])
                .current_dir(temp.path())
                .env("HOME", temp.path())
                .env("XDG_CONFIG_HOME", temp.path());
            remove_inherited_gest(&mut child, std::env::vars_os().map(|(key, _)| key));
            let result = child
                .env("GOSSIL_CONFIG_TEST_CHILD", "native-bytes")
                .env("GEST_CONFIG", &config)
                .env("GEST_STORAGE__DATA_DIR", &native)
                .output()
                .unwrap();
            let diagnostic = String::from_utf8_lossy(&result.stderr);
            assert!(result.status.success(), "{diagnostic}");
            assert_eq!(
                std::fs::read_to_string(&config).unwrap(),
                "[storage]\ndata_dir='/file-selection-must-not-revive'\n"
            );
            assert!(!temp.path().join(".gossil").exists());
        }
    }
    /// Test-only process protocol; no production configuration or fault hooks are introduced.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn native_environment_child() {
        if std::env::var_os("GOSSIL_CONFIG_TEST_CHILD").is_none() {
            return;
        }
        let root = std::env::current_dir().unwrap().canonicalize().unwrap();
        let requested = PathBuf::from(std::env::var_os("GEST_STORAGE__DATA_DIR").unwrap());
        let selected = load(&root).unwrap();
        if requested.is_absolute() {
            assert_eq!(selected.explicit, Some(requested));
        } else {
            assert!(selected.explicit.is_none());
        }
    }

    /// Real configuration read and TOML errors retain typed sources without printing provider contents.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn typed_configuration_causes_preserve_safe_outer_diagnostics() {
        use std::error::Error as _;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("gest.toml");
        std::fs::write(&path, [255]).unwrap();
        let error = read(&path, &mut Table::new()).unwrap_err();
        assert!(
            error
                .source()
                .and_then(|source| source.downcast_ref::<std::io::Error>())
                .is_some(),
            "configuration read must retain its actual I/O cause"
        );
        std::fs::write(&path, "private_provider_token = [\n").unwrap();
        let error = read(&path, &mut Table::new()).unwrap_err();
        assert!(
            error
                .source()
                .and_then(|source| source.downcast_ref::<toml::de::Error>())
                .is_some()
        );
        assert!(
            error
                .to_string()
                .contains("invalid Gest configuration TOML")
        );
        assert!(!error.to_string().contains("private_provider_token"));
    }
    /// Construct child removals from an explicit inventory without changing shared process environment.
    fn remove_inherited_gest(
        command: &mut std::process::Command,
        keys: impl IntoIterator<Item = std::ffi::OsString>,
    ) {
        for key in keys {
            if key.to_str().is_some_and(|key| key.starts_with("GEST_")) {
                command.env_remove(key);
            }
        }
    }
    #[test]
    /// Check that explicit inherited Gest keys are removed while unrelated keys remain untouched.
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn child_configuration_removes_exact_inherited_gest_keys() {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        remove_inherited_gest(
            &mut command,
            [
                "GEST_CONFIG".into(),
                "GEST_STORAGE__DATA_DIR".into(),
                "OTHER".into(),
            ],
        );
        for key in ["GEST_CONFIG", "GEST_STORAGE__DATA_DIR"] {
            assert!(
                command
                    .get_envs()
                    .any(|(name, value)| name == key && value.is_none())
            );
        }
        assert!(!command.get_envs().any(|(name, _)| name == "OTHER"));
    }
}
