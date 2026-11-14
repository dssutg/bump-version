//! Strict Semantic Versioning: parsing, incrementing, comparison and the
//! Git tag formats built on top of versions.

use std::cmp::Ordering;
use std::fmt;

use crate::config::VERSION_TAG_FORMAT_SUBST;
use crate::error::{Error, Result};
use crate::utils;

/// Version represents a Semantic Versioning version: a numeric core with an
/// optional prerelease suffix. An empty prerelease means a final release.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: String,
}

/// ReleaseType describes how a version should be incremented.
///
/// `None` means "no release"; on the command line it is not accepted as a
/// forced type, because "nothing to release" is decided from the commits
/// instead.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReleaseType {
    Major,
    Minor,
    Patch,
    Prerelease,
    /// "no release"
    #[default]
    None,
}

impl fmt::Display for ReleaseType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ReleaseType::Major => "major",
            ReleaseType::Minor => "minor",
            ReleaseType::Patch => "patch",
            ReleaseType::Prerelease => "prerelease",
            ReleaseType::None => "none",
        })
    }
}

impl fmt::Display for Version {
    /// Renders the version in canonical X.Y.Z[-prerelease] form.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.prerelease.is_empty() {
            write!(f, "-{}", self.prerelease)?;
        }
        Ok(())
    }
}

impl Version {
    /// Advances the version according to the release type. Major, minor and
    /// patch increments reset the prerelease suffix; a prerelease increment
    /// starts a release at rc.0 and otherwise continues the existing suffix;
    /// `None` leaves the version unchanged.
    pub fn increment_to(&mut self, release_type: ReleaseType) {
        match release_type {
            ReleaseType::Major => {
                self.major += 1;
                self.minor = 0;
                self.patch = 0;
                self.prerelease.clear();
            }
            ReleaseType::Minor => {
                self.minor += 1;
                self.patch = 0;
                self.prerelease.clear();
            }
            ReleaseType::Patch => {
                self.patch += 1;
                self.prerelease.clear();
            }
            ReleaseType::Prerelease => {
                let current = std::mem::take(&mut self.prerelease);
                self.prerelease = next_prerelease(&current);
            }
            ReleaseType::None => {
                // Nothing to do: keep the version as-is.
            }
        }
    }
}

impl Ord for Version {
    /// Orders versions by Semantic Versioning precedence rules: a version
    /// without a prerelease always outranks one with a prerelease.
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
            .then_with(
                || match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
                    (true, true) => Ordering::Equal,
                    (true, false) => Ordering::Greater,
                    (false, true) => Ordering::Less,
                    (false, false) => compare_prereleases(&self.prerelease, &other.prerelease),
                },
            )
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// next_prerelease computes the next prerelease suffix: an empty suffix starts
/// at rc.0, a suffix without a dot gains .0, and a suffix ending in a numeric
/// identifier has that identifier incremented (rc.1 becomes rc.2).
fn next_prerelease(current: &str) -> String {
    if current.is_empty() {
        return "rc.0".to_owned();
    }

    let Some((prefix, counter)) = current.rsplit_once('.') else {
        return format!("{current}.0");
    };

    if !is_numeric_identifier(counter) {
        return format!("{current}.0");
    }

    match counter.parse::<u64>() {
        Ok(n) => format!("{prefix}.{}", n + 1),
        Err(_) => format!("{current}.0"),
    }
}

/// parse_release_type validates a forced release type as accepted on the
/// command line. `None` is deliberately not accepted here: "nothing to
/// release" is decided from the commits instead.
pub fn parse_release_type(release_type: &str) -> Result<ReleaseType> {
    match release_type {
        "major" => Ok(ReleaseType::Major),
        "minor" => Ok(ReleaseType::Minor),
        "patch" => Ok(ReleaseType::Patch),
        "prerelease" => Ok(ReleaseType::Prerelease),
        _ => Err(Error::msg(format!(
            "invalid release type {release_type:?}: must be one of major, minor, patch, prerelease"
        ))),
    }
}

/// parse_version parses a strict Semantic Versioning string: exactly three
/// dot-separated numbers without leading zeros, an optional non-empty
/// prerelease suffix, and no build metadata (the "+..." suffix is rejected).
pub fn parse_version(version: &str) -> Result<Version> {
    if version.contains('+') {
        return Err(Error::msg("build metadata is not supported"));
    }

    let (core, prerelease) = match version.split_once('-') {
        Some((core, prerelease)) => {
            validate_prerelease(prerelease)?;
            (core, prerelease)
        }
        None => (version, ""),
    };

    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return Err(Error::msg("version must have three dot-separated parts"));
    }

    let mut numbers = [0u64; 3];
    for (index, part) in parts.iter().enumerate() {
        numbers[index] = parse_version_number(part)?;
    }

    Ok(Version {
        major: numbers[0],
        minor: numbers[1],
        patch: numbers[2],
        prerelease: prerelease.to_owned(),
    })
}

/// parse_version_number parses one numeric core part: digits only, no leading
/// zeros, and within the range of u64.
fn parse_version_number(s: &str) -> Result<u64> {
    if s.is_empty() {
        return Err(Error::msg("version number must not be empty"));
    }
    if s.len() > 1 && s.starts_with('0') {
        return Err(Error::msg(format!(
            "version number {s:?} must not have leading zeros"
        )));
    }
    if !s.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::msg(format!(
            "version number {s:?} must contain digits only"
        )));
    }

    s.parse::<u64>()
        .map_err(|_| Error::msg(format!("version number {s:?} is out of range")))
}

/// validate_prerelease checks that the prerelease part is non-empty and made of
/// dot-separated identifiers of letters, digits and hyphens.
fn validate_prerelease(prerelease: &str) -> Result<()> {
    if prerelease.is_empty() {
        return Err(Error::msg("prerelease part must not be empty"));
    }

    for identifier in prerelease.split('.') {
        if identifier.is_empty() {
            return Err(Error::msg("prerelease identifiers must not be empty"));
        }
        for ch in identifier.chars() {
            if !ch.is_ascii_alphanumeric() && ch != '-' {
                return Err(Error::msg(format!(
                    "prerelease identifier {identifier:?} contains invalid character {ch:?}"
                )));
            }
        }
    }
    Ok(())
}

/// compare_prereleases compares two prerelease suffixes identifier by
/// identifier; a longer suffix that shares a prefix ranks higher.
fn compare_prereleases(a: &str, b: &str) -> Ordering {
    let a_parts: Vec<&str> = a.split('.').collect();
    let b_parts: Vec<&str> = b.split('.').collect();

    for i in 0..a_parts.len().min(b_parts.len()) {
        let ordering = compare_prerelease_identifiers(a_parts[i], b_parts[i]);
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    a_parts.len().cmp(&b_parts.len())
}

/// compare_prerelease_identifiers compares two prerelease identifiers: numeric
/// identifiers compare numerically and rank below alphanumeric ones.
fn compare_prerelease_identifiers(a: &str, b: &str) -> Ordering {
    match (is_numeric_identifier(a), is_numeric_identifier(b)) {
        (true, true) => compare_numeric_identifiers(a, b),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a.cmp(b),
    }
}

/// compare_numeric_identifiers compares digit strings numerically, tolerating
/// identifiers too large for u64 and identifiers with leading zeros.
fn compare_numeric_identifiers(a: &str, b: &str) -> Ordering {
    let a = a.trim_start_matches('0');
    let a = if a.is_empty() { "0" } else { a };
    let b = b.trim_start_matches('0');
    let b = if b.is_empty() { "0" } else { b };

    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// is_numeric_identifier reports whether s is a non-empty run of decimal
/// digits.
fn is_numeric_identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|byte| byte.is_ascii_digit())
}

/// version_to_git_tag renders the Git tag for version by substituting the
/// placeholder in tag_format.
pub fn version_to_git_tag(version: &Version, tag_format: &str) -> String {
    tag_format.replacen(VERSION_TAG_FORMAT_SUBST, &version.to_string(), 1)
}

/// get_current_version_tag returns the most recent tag reachable from HEAD,
/// or an error explaining that the repository has no version tags yet.
pub fn get_current_version_tag(repo: &std::path::Path) -> Result<String> {
    let out = utils::git(repo, &["describe", "--tags", "--abbrev=0"])?;
    if !out.success {
        if seems_no_tags_error(&out.stderr) {
            return Err(Error::msg(
                "no version tags found; mark the first version with `git tag v0.1.0`",
            ));
        }
        return Err(Error::msg(format!(
            "error getting current version from Git: {}",
            out.stderr
        )));
    }

    Ok(out.stdout.trim().to_owned())
}

/// seems_no_tags_error reports whether err_out is the git describe failure
/// produced by a repository without tags.
fn seems_no_tags_error(err_out: &str) -> bool {
    err_out
        .to_lowercase()
        .contains("no names found, cannot describe anything")
}

/// get_current_version_by_git_tag derives the current version from the most
/// recent Git tag according to version_tag_format.
pub fn get_current_version_by_git_tag(
    repo: &std::path::Path,
    version_tag_format: &str,
) -> Result<Version> {
    // Get current version tag from Git
    let current_version_tag = get_current_version_tag(repo)?;

    // Get current version from current tag
    let current_version_string =
        extract_version_string_from_tag(&current_version_tag, version_tag_format).map_err(
            |err| {
                Error::msg(format!(
                    "error extracting current version from Git tag: {err}"
                ))
            },
        )?;

    // Parse current version
    parse_version(&current_version_string)
        .map_err(|err| Error::msg(format!("error parsing current version from Git tag: {err}")))
}

/// extract_version_string_from_tag cuts the version out of the tag using the
/// placeholder in version_tag_format and fails when the tag does not match
/// that format.
pub fn extract_version_string_from_tag(
    version_tag: &str,
    version_tag_format: &str,
) -> Result<String> {
    // Get substrings before and after the version placeholder
    let Some((prefix, suffix)) = version_tag_format.split_once(VERSION_TAG_FORMAT_SUBST) else {
        // The config parser validates the placeholder count, so this is
        // unreachable through normal use.
        return Err(Error::msg(
            "version tag format must contain one {version} placeholder",
        ));
    };

    if !version_tag.starts_with(prefix) || !version_tag.ends_with(suffix) {
        return Err(Error::msg(format!(
            "version tag {version_tag:?} must have format {version_tag_format}"
        )));
    }

    // Get rid of both the prefix and suffix substrings to
    // extract the actual version number
    let length = version_tag.len() - prefix.len() - suffix.len();
    Ok(version_tag[prefix.len()..prefix.len() + length].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASE_VER: &str = "1.2.3";
    const PRERELEASE_RC0: &str = "1.2.3-rc.0";
    const PRERELEASE_RC1: &str = "1.2.3-rc.1";
    const V_TAG_FORMAT: &str = "v{version}";
    const REL_TAG_FORMAT: &str = "rel-{version}";

    fn must_parse_version(s: &str) -> Version {
        parse_version(s).unwrap_or_else(|err| panic!("parse_version({s:?}): {err}"))
    }

    #[test]
    fn parse_version_accepts_valid_versions() {
        for input in [
            "0.0.0",
            "1.2.3",
            "10.20.30",
            "1.7.0-rc.1",
            "1.7.0-alpha",
            "1.0.0-0",
            "1.0.0-a-b",
            "1.0.0-rc.0.1",
            "1.0.0-alpha.beta.2",
        ] {
            let version = parse_version(input)
                .unwrap_or_else(|err| panic!("parse_version({input:?}): {err}"));
            assert_eq!(version.to_string(), input, "round trip of {input:?}");
        }
    }

    #[test]
    fn parse_version_rejects_invalid_versions() {
        for input in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "v0.4.1",
            " 1.2.3",
            "1.2.3 ",
            "01.2.3",
            "1.02.3",
            "1.2.03",
            "-1.2.3",
            "1.2.3-",
            "1.2.3-rc..1",
            "1.2.3+build",
            "1.2.3-rc.1+build",
            "1.2.x",
            "1.2.3 rc",
            "99999999999999999999.0.0",
        ] {
            assert!(
                parse_version(input).is_err(),
                "parse_version({input:?}) must fail"
            );
        }
    }

    #[test]
    fn parse_version_rejects_build_metadata_with_explicit_error() {
        let err = parse_version("1.2.3+build").unwrap_err().to_string();
        assert!(err.contains("build metadata"), "error = {err}");
    }

    #[test]
    fn version_to_string() {
        assert_eq!(
            Version {
                major: 2,
                minor: 4,
                patch: 6,
                ..Version::default()
            }
            .to_string(),
            "2.4.6"
        );
        assert_eq!(
            Version {
                major: 2,
                minor: 4,
                patch: 6,
                prerelease: "rc.1".to_owned()
            }
            .to_string(),
            "2.4.6-rc.1"
        );
        assert_eq!(Version::default().to_string(), "0.0.0");
    }

    #[test]
    fn version_compare() {
        let cases = [
            ("3.1.0", "3.1.0", Ordering::Equal),
            ("1.4.2", "5.0.0", Ordering::Less),
            ("5.0.1", "1.4.1", Ordering::Greater),
            ("1.4.2", "1.5.0", Ordering::Less),
            ("1.8.9", "1.8.10", Ordering::Less),
            ("1.8.10", "1.8.9", Ordering::Greater),
            ("1.0.0-rc.1", "1.0.0", Ordering::Less),
            ("1.0.0", "1.0.0-rc.1", Ordering::Greater),
            ("1.0.0-rc.9", "1.0.0-rc.10", Ordering::Less),
            ("2.3.4-rc", "2.3.4-rc.1", Ordering::Less),
            ("1.0.0-1", "1.0.0-alpha", Ordering::Less),
            ("1.0.0-alpha", "1.0.0-beta", Ordering::Less),
            ("1.0.0-alpha.1", "1.0.0-alpha.beta", Ordering::Less),
            ("4.0.0-rc.1", "4.0.0-rc.1", Ordering::Equal),
        ];

        for (left, right, want) in cases {
            let (l, r) = (must_parse_version(left), must_parse_version(right));
            assert_eq!(l.cmp(&r), want, "{left}.cmp({right})");
        }
    }

    #[test]
    fn versions_sort_by_precedence() {
        let input = [
            "2.1.0-rc.2",
            "2.1.0-rc.10",
            "2.1.0-rc",
            "2.1.0",
            "2.0.9",
            "2.1.0-rc.1",
        ];
        let want = [
            "2.0.9",
            "2.1.0-rc",
            "2.1.0-rc.1",
            "2.1.0-rc.2",
            "2.1.0-rc.10",
            "2.1.0",
        ];

        let mut versions: Vec<Version> = input.iter().map(|s| must_parse_version(s)).collect();
        versions.sort();

        let got: Vec<String> = versions.iter().map(Version::to_string).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn version_increment_to() {
        let cases = [
            ("1.2.3", ReleaseType::Major, "2.0.0"),
            ("1.2.3", ReleaseType::Minor, "1.3.0"),
            ("1.2.3", ReleaseType::Patch, "1.2.4"),
            ("1.2.3", ReleaseType::Prerelease, PRERELEASE_RC0),
            ("1.2.3-rc.0", ReleaseType::Prerelease, PRERELEASE_RC1),
            ("1.2.3-rc", ReleaseType::Prerelease, PRERELEASE_RC0),
            ("1.2.3-alpha", ReleaseType::Prerelease, "1.2.3-alpha.0"),
            ("1.2.3-alpha.0", ReleaseType::Prerelease, "1.2.3-alpha.1"),
            ("1.2.3-rc.1", ReleaseType::Major, "2.0.0"),
            ("1.2.3-rc.1", ReleaseType::Minor, "1.3.0"),
            ("1.2.3-rc.1", ReleaseType::Patch, "1.2.4"),
            ("1.2.3", ReleaseType::None, RELEASE_VER),
            ("1.2.3-rc.1", ReleaseType::None, PRERELEASE_RC1),
        ];

        for (start, release_type, want) in cases {
            let mut version = must_parse_version(start);
            version.increment_to(release_type);
            assert_eq!(
                version.to_string(),
                want,
                "{start}.increment_to({release_type})"
            );
        }
    }

    #[test]
    fn parse_release_type_accepts_forcing_values() {
        for (input, want) in [
            ("major", ReleaseType::Major),
            ("minor", ReleaseType::Minor),
            ("patch", ReleaseType::Patch),
            ("prerelease", ReleaseType::Prerelease),
        ] {
            assert_eq!(parse_release_type(input).unwrap(), want);
        }
    }

    #[test]
    fn parse_release_type_rejects_other_values() {
        for input in ["", "none", "Major", "hotfix", "bump"] {
            assert!(
                parse_release_type(input).is_err(),
                "parse_release_type({input:?})"
            );
        }
    }

    #[test]
    fn version_to_git_tag_renders_tags() {
        let cases = [
            (
                Version {
                    major: 1,
                    minor: 2,
                    patch: 3,
                    ..Version::default()
                },
                V_TAG_FORMAT,
                "v1.2.3",
            ),
            (
                Version {
                    major: 1,
                    minor: 2,
                    patch: 3,
                    ..Version::default()
                },
                "{version}",
                "1.2.3",
            ),
            (
                Version {
                    major: 1,
                    minor: 2,
                    patch: 3,
                    ..Version::default()
                },
                REL_TAG_FORMAT,
                "rel-1.2.3",
            ),
            (
                Version {
                    major: 4,
                    minor: 0,
                    patch: 0,
                    prerelease: "alpha.1".to_owned(),
                },
                V_TAG_FORMAT,
                "v4.0.0-alpha.1",
            ),
        ];

        for (version, format, want) in cases {
            assert_eq!(version_to_git_tag(&version, format), want);
        }
    }

    #[test]
    fn extract_version_string_from_tag_examples() {
        assert_eq!(
            extract_version_string_from_tag("v1.2.3", V_TAG_FORMAT).unwrap(),
            RELEASE_VER
        );
        assert_eq!(
            extract_version_string_from_tag("rel-1.2.3", "rel-{version}").unwrap(),
            RELEASE_VER
        );

        let err = extract_version_string_from_tag(RELEASE_VER, V_TAG_FORMAT)
            .unwrap_err()
            .to_string();
        assert!(err.contains("must have format"), "error = {err}");

        assert!(extract_version_string_from_tag("v9.9.9", "rel-{version}").is_err());
    }
}
