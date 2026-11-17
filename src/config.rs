//! The bump-version.cfg configuration file: parsing on top of the built-in
//! defaults, validation, serialization.

use std::fs;
use std::path::Path;

use crate::error::{Error, Result};
use crate::utils::file_exists;

pub const DEFAULT_CONFIG_FILENAME: &str = "bump-version.cfg";
/// Substitute for an actual version in tag and commit templates.
pub const VERSION_TAG_FORMAT_SUBST: &str = "{version}";

/// Config is the parsed content of a bump-version.cfg file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub version: String,
    pub version_filenames: Vec<String>,
    pub package_json_filename: String,
    pub change_log_filename: String,
    pub ignore_invalid_commits: bool,
    pub version_tag_format: String,
    pub allowed_commit_kinds: Vec<String>,
    pub bump_version_commit: String,
    pub should_push_to_origin: bool,
}

/// get_default_config returns the default config as read-only.
pub fn get_default_config() -> Config {
    Config {
        version: "1.0.0".to_owned(),
        version_filenames: Vec::new(),
        package_json_filename: String::new(),
        change_log_filename: "CHANGELOG.md".to_owned(),
        ignore_invalid_commits: false,
        version_tag_format: "v{version}".to_owned(),
        allowed_commit_kinds: [
            "BREAKING CHANGE",
            "build",
            "chore",
            "ci",
            "docs",
            "feat",
            "fix",
            "perf",
            "refactor",
            "revert",
            "style",
            "test",
        ]
        .iter()
        .map(|kind| (*kind).to_owned())
        .collect(),
        bump_version_commit: "chore(release): {version}".to_owned(),
        should_push_to_origin: false,
    }
}

/// load_config parses name on top of the built-in defaults. An absent name
/// reads the default config file from the repository root when it exists and
/// otherwise yields the defaults untouched.
pub fn load_config(repo: &Path, name: Option<&str>) -> Result<Config> {
    let default_config = get_default_config();

    // If the config file name is empty, read the default config file, but
    // even if that file does not exist, use the default config object.
    let name = match name {
        Some(name) => name.to_owned(),
        None => {
            if !file_exists(repo.join(DEFAULT_CONFIG_FILENAME)) {
                return Ok(default_config);
            }
            DEFAULT_CONFIG_FILENAME.to_owned()
        }
    };

    // Read config from file
    let path = repo.join(&name);
    let cfg_data = fs::read_to_string(&path)?;

    // Base off the default config
    let mut cfg = default_config;

    for (line_number, line) in cfg_data.lines().enumerate() {
        let line = line.trim(); // ignore space
        if line.is_empty() || line.starts_with('#') {
            // Ignore empty line or comments
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            // Only key value pairs expected
            return Err(Error::msg(format!(
                "invalid config line {} format, must be =-separated key-value pair",
                line_number + 1
            )));
        };

        // No spaces around equals
        let key = key.trim();
        let value = value.trim();

        // Overwrite the default parameters (unknown keys are ignored)
        match key {
            "version" => cfg.version = value.to_owned(),
            "versionFilenames" => cfg.version_filenames = split_list(value),
            "packageJSONFilename" => cfg.package_json_filename = value.to_owned(),
            "changeLogFilename" => cfg.change_log_filename = value.to_owned(),
            "ignoreInvalidCommits" => cfg.ignore_invalid_commits = value == "true",
            "versionTagFormat" => {
                cfg.version_tag_format = value.to_owned();
                // Validate version tag format
                if cfg
                    .version_tag_format
                    .matches(VERSION_TAG_FORMAT_SUBST)
                    .count()
                    != 1
                {
                    return Err(Error::msg(format!(
                        "version tag format {:?} expected to have one occurrence of \
                         {VERSION_TAG_FORMAT_SUBST}",
                        cfg.version_tag_format
                    )));
                }
            }
            "allowedCommitKinds" => cfg.allowed_commit_kinds = split_list(value),
            "bumpVersionCommit" => cfg.bump_version_commit = value.to_owned(),
            "shouldPushToOrigin" => cfg.should_push_to_origin = value == "true",
            _ => {}
        }
    }

    Ok(cfg)
}

/// Splits a comma-separated config list into trimmed items. An empty value
/// yields no items, and so do empty items between the commas.
fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

/// save_config serialises config as key=value lines and writes them to
/// config_path with the given permissions.
pub fn save_config(config_path: &Path, config: &Config, permissions: u32) -> Result<()> {
    let mut out = String::new();
    out.push_str(&format!("version={}\n", config.version));
    out.push_str(&format!(
        "versionFilenames={}\n",
        config.version_filenames.join(",")
    ));
    out.push_str(&format!(
        "packageJSONFilename={}\n",
        config.package_json_filename
    ));
    out.push_str(&format!(
        "changeLogFilename={}\n",
        config.change_log_filename
    ));
    out.push_str(&format!(
        "ignoreInvalidCommits={}\n",
        config.ignore_invalid_commits
    ));
    out.push_str(&format!("versionTagFormat={}\n", config.version_tag_format));
    out.push_str(&format!(
        "allowedCommitKinds={}\n",
        config.allowed_commit_kinds.join(",")
    ));
    out.push_str(&format!(
        "bumpVersionCommit={}\n",
        config.bump_version_commit
    ));
    out.push_str(&format!(
        "shouldPushToOrigin={}\n",
        config.should_push_to_origin
    ));

    write_file_with_mode(config_path, out.as_bytes(), permissions)
}

/// Writes `data` to `path` creating the file when it is missing, with the
/// given POSIX permissions.
pub(crate) fn write_file_with_mode(path: &Path, data: &[u8], mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(mode)
            .open(path)?;
        file.write_all(data)?;
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        fs::write(path, data)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
