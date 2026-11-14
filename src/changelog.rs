//! Changelog rendering: release sections, remote compare links, credential
//! stripping and case-insensitive record sorting.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::commit::{
    Commit, CommitStats, KIND_BUILD, KIND_CHORE, KIND_CI, KIND_DOCS, KIND_FEAT, KIND_FIX,
    KIND_PERF, KIND_REFACTOR, KIND_STYLE, KIND_TEST,
};
use crate::config::{Config, write_file_with_mode};
use crate::error::Result;
use crate::utils::{file_exists, git, warn};
use crate::version::{Version, version_to_git_tag};

/// associates a commit kind with the changelog section that renders it.
/// Sections are rendered in the order they are listed.
struct ChangelogSection {
    name: &'static str,
    kind: &'static str,
}

/// changelog_sections returns the changelog kind sections in their canonical
/// render order. Kinds without a dedicated section fall back to Chores.
fn changelog_sections() -> &'static [ChangelogSection] {
    const SECTIONS: &[ChangelogSection] = &[
        ChangelogSection {
            name: "Features",
            kind: KIND_FEAT,
        },
        ChangelogSection {
            name: "Bug Fixes",
            kind: KIND_FIX,
        },
        ChangelogSection {
            name: "Performance",
            kind: KIND_PERF,
        },
        ChangelogSection {
            name: "Refactoring",
            kind: KIND_REFACTOR,
        },
        ChangelogSection {
            name: "Documentation",
            kind: KIND_DOCS,
        },
        ChangelogSection {
            name: "Tests",
            kind: KIND_TEST,
        },
        ChangelogSection {
            name: "Build",
            kind: KIND_BUILD,
        },
        ChangelogSection {
            name: "Continuous Integration",
            kind: KIND_CI,
        },
        ChangelogSection {
            name: "Styling",
            kind: KIND_STYLE,
        },
        ChangelogSection {
            name: "Chores",
            kind: KIND_CHORE,
        },
    ];
    SECTIONS
}

/// generate_changelog renders the release section for the given versions,
/// prepends it to the existing changelog content and writes the result to
/// the configured changelog file.
pub fn generate_changelog(
    repo: &Path,
    current_version: &Version,
    new_version: &Version,
    commit_stats: &CommitStats,
    cfg: &Config,
    url_base: &str,
) -> Result<()> {
    let changelog_path = repo.join(&cfg.change_log_filename);

    let mut new_change_log =
        generate_new_changelog_head(current_version, new_version, commit_stats, cfg, url_base);

    // Merge with the current Changelog if it exists
    if file_exists(&changelog_path) {
        new_change_log.push('\n');

        // Read old changelog file and split into lines (the trailing
        // overflow entry preserves any final newline of the source file)
        let mut change_log_lines: Vec<String> = std::fs::read_to_string(&changelog_path)?
            .split('\n')
            .map(str::to_owned)
            .collect();

        // Remove old header if exists
        if change_log_lines
            .first()
            .is_some_and(|first| is_old_changelog_header(first))
        {
            change_log_lines.remove(0);
        }

        // Remove all leading empty lines
        while change_log_lines
            .first()
            .is_some_and(|line| line.trim().is_empty())
        {
            change_log_lines.remove(0);
        }

        // Append all the rest lines of the old Changelog to the updated changelog
        for line in change_log_lines {
            new_change_log.push_str(&line);
            new_change_log.push('\n');
        }
    }

    // Write updated Changelog
    write_file_with_mode(&changelog_path, new_change_log.as_bytes(), 0o600)?;

    Ok(())
}

/// Matches the old `^\s*#\s*Changelog\s*$` header of a previously generated
/// changelog for removal.
fn is_old_changelog_header(line: &str) -> bool {
    match line.trim().strip_prefix('#') {
        Some(rest) => rest.trim() == "Changelog",
        None => false,
    }
}

/// generate_new_changelog_head renders the release section that is prepended
/// to the changelog. When url_base is known, the release header links to the
/// remote compare view and commit hashes link to their commits.
pub fn generate_new_changelog_head(
    current_version: &Version,
    new_version: &Version,
    commit_stats: &CommitStats,
    cfg: &Config,
    url_base: &str,
) -> String {
    let cur_date_str = today_utc();

    let mut new_change_log = String::new();

    // Write Changelog header
    if !url_base.is_empty() {
        new_change_log.push_str(&format!(
            "# Changelog\n\n## [{}]({}/compare/{}...{}) ({})\n",
            new_version,
            url_base,
            version_to_git_tag(current_version, &cfg.version_tag_format),
            version_to_git_tag(new_version, &cfg.version_tag_format),
            cur_date_str,
        ));
    } else {
        new_change_log.push_str(&format!(
            "# Changelog\n\n## [{}] ({})\n",
            new_version, cur_date_str
        ));
    }

    // Write kind sections in canonical order; collect kinds that have no
    // dedicated section so they can be reported under Chores
    let sections = changelog_sections();
    let known_kinds: HashSet<&str> = sections.iter().map(|section| section.kind).collect();

    for section in sections {
        let mut commits: Vec<&Commit> = commit_stats
            .by_kind
            .get(section.kind)
            .map_or_default(|commits| commits.iter().collect());
        if section.kind == KIND_CHORE {
            for (kind, kind_commits) in &commit_stats.by_kind {
                if !known_kinds.contains(kind.as_str()) {
                    commits.extend(kind_commits.iter());
                }
            }
        }
        write_changelog_section(&mut new_change_log, section.name, &commits, url_base);
    }

    write_changelog_section(
        &mut new_change_log,
        "Reverted Changes",
        &commit_stats.reverted_pre_tag.iter().collect::<Vec<_>>(),
        url_base,
    );

    new_change_log
}

/// write_changelog_section appends a named section with its sorted commit
/// records to new_change_log, or nothing when the section has no commits.
fn write_changelog_section(
    new_change_log: &mut String,
    section_name: &str,
    commits: &[&Commit],
    url_base: &str,
) {
    if commits.is_empty() {
        return;
    }

    // Write header
    new_change_log.push_str("\n\n### ");
    new_change_log.push_str(section_name);
    new_change_log.push_str("\n\n");

    // Stringify and sort list
    let mut sorted: Vec<String> = commits
        .iter()
        .map(|commit| commit_to_changelog_record(commit, url_base))
        .collect();
    sort_changelog_records(&mut sorted);

    // Write record list
    for record in sorted {
        new_change_log.push_str(&record);
        new_change_log.push('\n');
    }
}

/// commit_to_changelog_record renders one changelog bullet for c, linking the
/// short hash to the remote commit view when url_base is known.
fn commit_to_changelog_record(commit: &Commit, url_base: &str) -> String {
    let hash = commit.short_hash();
    let hash = if url_base.is_empty() {
        hash
    } else {
        format!("[{hash}]({url_base}/commit/{})", commit.hash)
    };

    let breaking_change = if commit.breaking {
        " [BREAKING CHANGE]"
    } else {
        ""
    };
    if commit.scope.is_empty() {
        format!("*{breaking_change} {} {hash}", commit.description)
    } else {
        format!(
            "* **{}:**{breaking_change} {} {hash}",
            commit.scope, commit.description
        )
    }
}

/// resolve_url_base returns the web base URL of the given Git remote for
/// changelog links. When the remote is missing or cannot be interpreted it
/// warns once on stderr and returns "", which omits all links.
pub fn resolve_url_base(repo: &Path, remote: &str) -> String {
    let out = match git(repo, &["remote", "get-url", remote]) {
        Ok(out) => out,
        Err(err) => {
            warn(&format!(
                "WARNING: could not get URL of remote {remote:?}: {err}; changelog links will be omitted"
            ));
            return String::new();
        }
    };
    if !out.success {
        let mut detail = out.stderr.trim().to_owned();
        if detail.is_empty() {
            detail = format!("exit status {}", out.exit_code);
        }
        warn(&format!(
            "WARNING: could not get URL of remote {remote:?}: {detail}; changelog links will be omitted"
        ));
        return String::new();
    }

    let base = without_embedded_credentials(&normalize_remote_url(&out.stdout));
    if base.is_empty() {
        warn(&format!(
            "WARNING: could not interpret URL of remote {remote:?}; changelog links will be omitted"
        ));
        return String::new();
    }
    base
}

/// normalize_remote_url converts a Git remote URL into a repository base URL
/// suitable for changelog links. HTTP(S) URLs pass through, SSH and SCP-like
/// (git@host:path) URLs are rewritten to HTTPS, and scheme-less host/owner/repo
/// forms gain an https:// prefix. It returns "" for anything that cannot be
/// interpreted as a repository URL.
pub fn normalize_remote_url(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() || raw.contains(' ') || raw.contains('\t') {
        return String::new();
    }

    if !raw.contains("://") {
        // SCP-like syntax: [user@]host:path
        if let Some(colon) = raw.find(':').filter(|colon| !raw[..*colon].contains('/')) {
            {
                let mut host = &raw[..colon];
                if let Some((_, host_without_user)) = raw[..colon].split_once('@') {
                    host = host_without_user;
                }
                let raw_path = &raw[colon + 1..];
                let path = raw_path
                    .trim_start_matches('/')
                    .strip_suffix(".git")
                    .unwrap_or_else(|| raw_path.trim_start_matches('/'));
                if host.is_empty() || path.is_empty() {
                    return String::new();
                }
                return format!("https://{host}/{path}");
            }
        }

        // Scheme-less host/owner/repo form
        if !raw.contains('/') {
            return String::new();
        }
        return format!("https://{}", raw.trim_start_matches('/'));
    }

    // <scheme>://<authority><path>
    let (scheme, rest) = raw
        .split_once("://")
        .expect("scheme delimiter was checked above");
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end]; // user[:pass]@host[:port]
    let after = &rest[authority_end..];

    // The user info never takes part in the rendered links; keep only the
    // host part of the authority (everything after the last '@').
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if host.is_empty() {
        return String::new();
    }
    match scheme {
        "http" | "https" | "ssh" => {}
        _ => return String::new(),
    }

    let path = after
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if path.is_empty() {
        return String::new();
    }
    if scheme == "ssh" {
        return format!("https://{host}/{path}");
    }
    format!("{scheme}://{host}/{path}")
}

/// without_embedded_credentials removes the userinfo part (user:token@) from an
/// HTTP(S) URL so that credentials embedded in a remote URL never reach the
/// changelog.
pub fn without_embedded_credentials(raw: &str) -> String {
    let Some(scheme_end) = raw.find("://") else {
        return raw.to_owned();
    };
    let scheme = &raw[..scheme_end];
    if scheme != "http" && scheme != "https" {
        return raw.to_owned();
    }

    let authority = &raw[scheme_end + 3..];
    let Some(at) = authority.find('@') else {
        return raw.to_owned();
    };
    let slash = authority.find('/');
    if slash.is_some_and(|slash| at > slash) {
        return raw.to_owned();
    }

    format!("{}{}", &raw[..scheme_end + 3], &authority[at + 1..])
}

/// The current date in UTC as YYYY-MM-DD. UTC is used so that the stamp is
/// reproducible and does not depend on the machine's timezone.
fn today_utc() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64);
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}")
}

/// Converts a count of days since the Unix epoch into a civil date
/// (Howard Hinnant's civil_from_days algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097); // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153; // [0, 11]
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1; // [1, 31]
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    }; // [1, 12]
    let year = year + i64::from(month <= 2);
    (year, month as u32, day as u32)
}

/// sort_changelog_records orders changelog records case-insensitively,
/// ignoring asterisk bullet prefixes so they do not influence the order.
pub fn sort_changelog_records(records: &mut [String]) {
    records.sort_by(|a, b| compare_changelog_records(a, b));
}

/// The stable ordering key of two changelog records: compare rune by rune,
/// skipping '*' characters, lowercasing each rune; ties break on the byte
/// length of the remaining suffixes.
fn compare_changelog_records(a: &str, b: &str) -> Ordering {
    // skip '*' characters and compare by case-folded runes
    let (mut ai, mut bi) = (0usize, 0usize);
    while ai < a.len() && bi < b.len() {
        // advance to next non-asterisk rune in a
        if a[ai..].starts_with('*') {
            ai += 1;
            continue;
        }
        // advance to next non-asterisk rune in b
        if b[bi..].starts_with('*') {
            bi += 1;
            continue;
        }

        let ra = a[ai..].chars().next().expect("non-empty remainder");
        let rb = b[bi..].chars().next().expect("non-empty remainder");

        let fa = ra.to_lowercase().next().expect("non-empty lowering");
        let fb = rb.to_lowercase().next().expect("non-empty lowering");
        if fa != fb {
            return fa.cmp(&fb);
        }

        ai += ra.len_utf8();
        bi += rb.len_utf8();
    }

    // skip remaining '*' in either string
    while ai < a.len() && a[ai..].starts_with('*') {
        ai += 1;
    }
    while bi < b.len() && b[bi..].starts_with('*') {
        bi += 1;
    }

    // shorter (remaining) string is less
    (a.len() - ai).cmp(&(b.len() - bi))
}

#[cfg(test)]
mod tests;
