//! Tests for the configuration file handling.

use super::*;
use crate::commit::{KIND_FEAT, KIND_FIX};
use crate::utils::testing::TempDir;
use std::path::Path;

const REL_TAG_FORMAT: &str = "rel-{version}";

fn load(repo: &Path, name: Option<&str>) -> Result<Config> {
    load_config(repo, name)
}

#[test]
fn load_config_uses_defaults_when_file_absent() {
    let dir = TempDir::new("config-absent");

    let cfg = load(dir.path(), None).expect("loadConfig() must succeed with no file");
    assert_eq!(cfg, get_default_config());
}

#[test]
fn load_config_full_parse() {
    let dir = TempDir::new("config-full");
    let path = dir.path().join(DEFAULT_CONFIG_FILENAME);
    let content = "# project configuration\n\
                   version=2.3.4\n\
                   \n\
                   versionFilenames=a.txt, b.txt\n\
                   packageJSONFilename=web/package.json\n\
                   changeLogFilename=HISTORY.md\n\
                   ignoreInvalidCommits=true\n\
                   versionTagFormat=rel-{version}\n\
                   allowedCommitKinds= feat, fix , style\n\
                   bumpVersionCommit=release: {version}\n\
                   shouldPushToOrigin=true\n";
    std::fs::write(&path, content).expect("could not write config");

    let cfg = load(dir.path(), None).expect("loadConfig() must succeed");

    assert_eq!(cfg.version, "2.3.4");
    assert_eq!(cfg.version_filenames, ["a.txt", "b.txt"]);
    assert_eq!(cfg.package_json_filename, "web/package.json");
    assert_eq!(cfg.change_log_filename, "HISTORY.md");
    assert!(cfg.ignore_invalid_commits);
    assert_eq!(cfg.version_tag_format, REL_TAG_FORMAT);
    assert_eq!(
        cfg.allowed_commit_kinds,
        [
            KIND_FEAT.to_owned(),
            KIND_FIX.to_owned(),
            "style".to_owned()
        ]
    );
    assert_eq!(cfg.bump_version_commit, "release: {version}");
    assert!(cfg.should_push_to_origin);
}

#[test]
fn load_config_rejects_invalid_line() {
    let dir = TempDir::new("config-invalid-line");
    let path = dir.path().join(DEFAULT_CONFIG_FILENAME);
    std::fs::write(&path, "this is not a key value pair\n").expect("could not write config");

    let err = load(dir.path(), None).unwrap_err().to_string();
    assert!(
        err.contains("invalid config line 1 format"),
        "error = {err}"
    );
}

#[test]
fn load_config_validates_version_tag_format() {
    for value in ["vX.Y.Z", "{version}.{version}"] {
        let dir = TempDir::new("config-tag-format");
        let path = dir.path().join(DEFAULT_CONFIG_FILENAME);
        std::fs::write(&path, format!("versionTagFormat={value}\n"))
            .expect("could not write config");

        let err = load(dir.path(), None).unwrap_err().to_string();
        assert!(
            err.contains("expected to have one occurrence"),
            "error = {err}"
        );
    }
}

#[test]
fn load_config_ignores_unknown_keys() {
    let dir = TempDir::new("config-unknown-keys");
    let path = dir.path().join(DEFAULT_CONFIG_FILENAME);
    std::fs::write(&path, "unknownKey=whatever\nversion=9.9.9\n").expect("could not write config");

    let cfg = load(dir.path(), None).expect("loadConfig() must succeed");
    assert_eq!(cfg.version, "9.9.9");
}

#[test]
fn save_load_config_roundtrip() {
    let dir = TempDir::new("config-roundtrip");
    let path = dir.path().join(DEFAULT_CONFIG_FILENAME);

    let cfg = Config {
        version: "3.2.1".to_owned(),
        version_filenames: vec!["a.txt".to_owned(), "b.txt".to_owned()],
        package_json_filename: "package.json".to_owned(),
        change_log_filename: "HISTORY.md".to_owned(),
        ignore_invalid_commits: true,
        version_tag_format: REL_TAG_FORMAT.to_owned(),
        allowed_commit_kinds: vec![KIND_FEAT.to_owned(), KIND_FIX.to_owned()],
        bump_version_commit: "release: {version}".to_owned(),
        should_push_to_origin: true,
    };

    save_config(&path, &cfg, 0o600).expect("saveConfig() must succeed");
    let loaded =
        load(dir.path(), Some(&path.display().to_string())).expect("loadConfig() must succeed");
    assert_eq!(loaded, cfg);
}

#[test]
fn load_config_missing_custom_file_fails() {
    let dir = TempDir::new("config-missing");
    let path = dir.path().join("absent.cfg");

    match load(dir.path(), Some(&path.display().to_string())) {
        Err(Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {}
        other => panic!("loadConfig() error = {other:?}, want a not-exist error"),
    }
}

/// An empty list value in the config file must not produce an empty filename.
#[test]
fn load_config_empty_list_yields_no_items() {
    let dir = TempDir::new("config-empty-lists");
    let path = dir.path().join(DEFAULT_CONFIG_FILENAME);
    std::fs::write(&path, "versionFilenames=\nallowedCommitKinds=\n")
        .expect("could not write config");

    let cfg = load(dir.path(), None).expect("loadConfig() must succeed");
    assert!(
        cfg.version_filenames.is_empty(),
        "versionFilenames = {:?}",
        cfg.version_filenames
    );
    assert!(
        cfg.allowed_commit_kinds.is_empty(),
        "allowedCommitKinds = {:?}",
        cfg.allowed_commit_kinds
    );
}

/// Empty items inside a list — a bare comma or two commas around an item —
/// must not reach the version-file writer as an empty filename either.
#[test]
fn load_config_empty_items_in_list_yield_no_items() {
    let dir = TempDir::new("config-empty-items");
    let path = dir.path().join(DEFAULT_CONFIG_FILENAME);
    std::fs::write(
        &path,
        "versionFilenames=, a.txt , , b.txt,\nallowedCommitKinds= feat,, fix ,\n",
    )
    .expect("could not write config");

    let cfg = load(dir.path(), None).expect("loadConfig() must succeed");
    assert_eq!(
        cfg.version_filenames,
        ["a.txt", "b.txt"],
        "versionFilenames = {:?}",
        cfg.version_filenames
    );
    assert_eq!(
        cfg.allowed_commit_kinds,
        [KIND_FEAT.to_owned(), KIND_FIX.to_owned()],
        "allowedCommitKinds = {:?}",
        cfg.allowed_commit_kinds
    );
}
