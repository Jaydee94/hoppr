//! YAML file persistence shared by the config, inventory, history and
//! favorites stores: tolerant loading and crash-safe (write-then-rename)
//! saving.

use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};

/// Permissions applied to a file written by [`save_yaml`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Access {
    /// Readable and writable by the owner only (`0600` on Unix; a no-op
    /// elsewhere).
    OwnerOnly,
    /// Whatever the process umask yields.
    Inherit,
}

/// Parse the YAML file at `path`. An empty or whitespace-only file yields
/// `T::default()`. `kind` names the file in error messages ("config",
/// "inventory", …).
pub(crate) fn load_yaml<T>(path: &Path, kind: &str) -> Result<T>
where
    T: DeserializeOwned + Default,
{
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read {kind} file: {}", path.display()))?;

    if content.trim().is_empty() {
        return Ok(T::default());
    }

    yaml_serde::from_str(&content)
        .with_context(|| format!("invalid YAML in {kind} file: {}", path.display()))
}

/// Serialize `value` to `path`, creating parent directories as needed.
///
/// The YAML is staged in a sibling `<name>.tmp` file and renamed over the
/// target, so a crash mid-write never truncates the existing file.
pub(crate) fn save_yaml<T>(path: &Path, value: &T, kind: &str, access: Access) -> Result<()>
where
    T: Serialize + ?Sized,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {kind} dir: {}", parent.display()))?;
    }
    let yaml =
        yaml_serde::to_string(value).with_context(|| format!("failed to serialize {kind}"))?;

    let tmp = staging_path(path, kind);
    write_staged(&tmp, &yaml, access)
        .with_context(|| format!("failed to stage {kind} file: {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("failed to write {kind} file: {}", path.display()))
}

fn staging_path(path: &Path, kind: &str) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map_or_else(|| OsString::from(format!("{kind}.yaml")), OsString::from);
    let mut tmp = parent.join(file_name);
    tmp.as_mut_os_string().push(".tmp");
    tmp
}

/// An owner-only file is created with its final mode, so its contents are
/// never readable by others, not even between the write and a later chmod.
fn write_staged(tmp: &Path, yaml: &str, access: Access) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    if access == Access::OwnerOnly {
        // The mode only applies to a newly created file; a staging file
        // left behind by a crash would keep whatever mode it had.
        match fs::remove_file(tmp) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
            _ => {}
        }
        restrict_to_owner(&mut options);
    }
    options.open(tmp)?.write_all(yaml.as_bytes())
}

#[cfg(unix)]
fn restrict_to_owner(options: &mut fs::OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn restrict_to_owner(_options: &mut fs::OpenOptions) {}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn save_creates_parents_and_leaves_no_staging_file() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("nested").join("data.yaml");

        save_yaml(&path, &vec!["a", "b"], "test", Access::Inherit).expect("save");

        let loaded: Vec<String> = load_yaml(&path, "test").expect("load");
        assert_eq!(loaded, ["a", "b"]);
        assert!(!staging_path(&path, "test").exists());
    }

    #[test]
    fn save_replaces_existing_content() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("data.yaml");
        save_yaml(&path, &vec!["old"], "test", Access::Inherit).expect("first save");
        save_yaml(&path, &vec!["new"], "test", Access::Inherit).expect("second save");

        let loaded: Vec<String> = load_yaml(&path, "test").expect("load");
        assert_eq!(loaded, ["new"]);
    }

    #[test]
    fn load_treats_blank_file_as_default() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("blank.yaml");
        fs::write(&path, " \n\n").expect("write");

        let loaded: Vec<String> = load_yaml(&path, "test").expect("load");
        assert!(loaded.is_empty());
    }

    #[test]
    fn load_errors_name_the_file() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("broken.yaml");
        fs::write(&path, "[unterminated").expect("write");

        let err = load_yaml::<Vec<String>>(&path, "test").expect_err("invalid yaml");
        assert!(format!("{err:#}").contains("broken.yaml"), "{err:#}");
    }

    #[cfg(unix)]
    #[test]
    fn owner_only_access_restricts_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("secret.yaml");
        save_yaml(&path, &vec!["x"], "test", Access::OwnerOnly).expect("save");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn owner_only_access_does_not_inherit_a_stale_staging_file_mode() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("secret.yaml");
        let stale = staging_path(&path, "test");
        fs::write(&stale, "left over").expect("write stale staging file");
        fs::set_permissions(&stale, fs::Permissions::from_mode(0o644)).expect("chmod");

        save_yaml(&path, &vec!["x"], "test", Access::OwnerOnly).expect("save");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
