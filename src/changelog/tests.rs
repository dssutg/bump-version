//! Tests for changelog rendering, remote URL handling and sorting.

use super::*;
use crate::commit::{Commit, CommitStats, KIND_CHORE, KIND_FEAT, KIND_FIX, KIND_PERF, KIND_STYLE};
use crate::utils::testing::TempDir;

const REPO: &str = "https://github.com/acme/app";

const HASH_FULL: &str = "0123456789abcdef0123456789abcdef01234567";
const HASH_FIX: &str = "bbbb222";
const HASH_PERF: &str = "cccc333";
const HASH_CHORE: &str = "dddd444";
const HASH_CUSTOM: &str = "eeee555";
const HASH_REVERT: &str = "ffff666";
const HASH_STYLE: &str = "6543210";
const HASH_OLD: &str = "abc1234";
const HASH_CREATED: &str = "def5678";

/// today's rendered date, shared with the expected changelog output below.
fn date() -> String {
    today_utc()
}

fn commit(hash: &str, scope: &str, description: &str, breaking: bool) -> Commit {
    Commit {
        hash: hash.to_owned(),
        scope: scope.to_owned(),
        description: description.to_owned(),
        breaking,
        ..Commit::default()
    }
}

/// An independent reimplementation of the expected release header used to
/// build expected changelog output in tests.
fn want_changelog_header(version: &str, compare_url: &str, date: &str) -> String {
    if compare_url.is_empty() {
        format!("# Changelog\n\n## [{version}] ({date})\n")
    } else {
        format!("# Changelog\n\n## [{version}]({compare_url}) ({date})\n")
    }
}

#[test]
fn generate_new_changelog_head_with_remote_links() {
    let date = date();
    let cfg = Config {
        version_tag_format: "v{version}".to_owned(),
        ..Config::default()
    };

    let mut by_kind = CommitStats::default();
    by_kind
        .by_kind
        .entry(KIND_FEAT.to_owned())
        .or_default()
        .push(commit(HASH_FULL, "ui", "add button", true));
    by_kind
        .by_kind
        .entry(KIND_FIX.to_owned())
        .or_default()
        .push(commit(HASH_FIX, "", "fix crash", false));
    by_kind
        .by_kind
        .entry(KIND_PERF.to_owned())
        .or_default()
        .push(commit(HASH_PERF, "", "speed up parsing", false));
    by_kind
        .by_kind
        .entry(KIND_CHORE.to_owned())
        .or_default()
        .push(commit(HASH_CHORE, "", "update deps", false));
    by_kind
        .by_kind
        .entry("custom".to_owned())
        .or_default()
        .push(commit(HASH_CUSTOM, "", "do thing", false));
    by_kind.reverted_pre_tag = vec![commit(HASH_REVERT, "", "old feature", false)];

    let current = Version {
        minor: 1,
        ..Version::default()
    };
    let next = Version {
        minor: 2,
        ..Version::default()
    };

    let got = generate_new_changelog_head(&current, &next, &by_kind, &cfg, REPO);

    let commit_link = |h: &str| format!("[{h}]({REPO}/commit/{h})");
    let want = want_changelog_header("0.2.0", &format!("{REPO}/compare/v0.1.0...v0.2.0"), &date)
        + "\n\n### Features\n\n"
        + &format!(
            "* **ui:** [BREAKING CHANGE] add button [{}]({REPO}/commit/{HASH_FULL})\n",
            &HASH_FULL[..7]
        )
        + "\n\n### Bug Fixes\n\n"
        + &format!("* fix crash {}\n", commit_link(HASH_FIX))
        + "\n\n### Performance\n\n"
        + &format!("* speed up parsing {}\n", commit_link(HASH_PERF))
        + "\n\n### Chores\n\n"
        + &format!("* do thing {}\n", commit_link(HASH_CUSTOM))
        + &format!("* update deps {}\n", commit_link(HASH_CHORE))
        + "\n\n### Reverted Changes\n\n"
        + &format!("* old feature {}\n", commit_link(HASH_REVERT));

    assert_eq!(got, want);
}

#[test]
fn generate_new_changelog_head_without_remote() {
    let date = date();
    let cfg = Config {
        version_tag_format: "v{version}".to_owned(),
        ..Config::default()
    };

    let mut stats = CommitStats::default();
    stats
        .by_kind
        .entry(KIND_STYLE.to_owned())
        .or_default()
        .push(commit(HASH_STYLE, "", "align widgets", false));

    let got = generate_new_changelog_head(
        &Version {
            minor: 2,
            ..Version::default()
        },
        &Version {
            minor: 3,
            ..Version::default()
        },
        &stats,
        &cfg,
        "",
    );

    let want = want_changelog_header("0.3.0", "", &date)
        + "\n\n### Styling\n\n"
        + &format!("* align widgets {HASH_STYLE}\n");

    assert_eq!(got, want);
}

#[test]
fn generate_new_changelog_head_with_no_commits() {
    let date = date();
    let cfg = Config {
        version_tag_format: "v{version}".to_owned(),
        ..Config::default()
    };

    let got = generate_new_changelog_head(
        &Version {
            major: 1,
            ..Version::default()
        },
        &Version {
            major: 2,
            ..Version::default()
        },
        &CommitStats::default(),
        &cfg,
        REPO,
    );

    let want = want_changelog_header("2.0.0", &format!("{REPO}/compare/v1.0.0...v2.0.0"), &date);
    assert_eq!(got, want);
}

#[test]
fn generate_changelog_merges_existing_file() {
    let date = date();
    let dir = TempDir::new("changelog-merge");
    let path = dir.path().join("CHANGELOG.md");
    let cfg = Config {
        change_log_filename: path.display().to_string(),
        version_tag_format: "v{version}".to_owned(),
        ..Config::default()
    };

    let old_content = format!(
        "# Changelog\n\n## [0.9.0](https://example.com/compare/v0.8.0...v0.9.0) (2026-01-01)\n\n\n### Styling\n\n* tidy old docs {HASH_OLD}\n"
    );
    std::fs::write(&path, &old_content).expect("could not write existing changelog");

    let mut stats = CommitStats::default();
    stats
        .by_kind
        .entry(KIND_FIX.to_owned())
        .or_default()
        .push(commit(HASH_CREATED, "", "fix new thing", false));

    generate_changelog(
        dir.path(),
        &Version {
            minor: 9,
            ..Version::default()
        },
        &Version {
            minor: 10,
            ..Version::default()
        },
        &stats,
        &cfg,
        "",
    )
    .expect("generateChangelog() must succeed");

    let got = std::fs::read_to_string(&path).expect("could not read changelog");
    let want = want_changelog_header("0.10.0", "", &date)
        + "\n\n### Bug Fixes\n\n"
        + &format!("* fix new thing {HASH_CREATED}\n")
        + "\n"
        + "## [0.9.0](https://example.com/compare/v0.8.0...v0.9.0) (2026-01-01)\n"
        + "\n\n### Styling\n\n"
        + &format!("* tidy old docs {HASH_OLD}\n")
        + "\n";

    assert_eq!(got, want);
}

#[test]
fn generate_changelog_creates_missing_file() {
    let date = date();
    let dir = TempDir::new("changelog-create");
    let path = dir.path().join("CHANGELOG.md");
    let cfg = Config {
        change_log_filename: path.display().to_string(),
        version_tag_format: "v{version}".to_owned(),
        ..Config::default()
    };

    let mut stats = CommitStats::default();
    stats
        .by_kind
        .entry(KIND_STYLE.to_owned())
        .or_default()
        .push(commit(HASH_STYLE, "", "align widgets", false));

    generate_changelog(
        dir.path(),
        &Version {
            minor: 1,
            ..Version::default()
        },
        &Version {
            minor: 2,
            ..Version::default()
        },
        &stats,
        &cfg,
        "",
    )
    .expect("generateChangelog() must succeed");

    let got = std::fs::read_to_string(&path).expect("could not read changelog");
    let want = want_changelog_header("0.2.0", "", &date)
        + "\n\n### Styling\n\n"
        + &format!("* align widgets {HASH_STYLE}\n");

    assert_eq!(got, want);
}

#[test]
fn without_embedded_credentials_table() {
    let cases = [
        // token userinfo
        (
            "https://x-access-token:ghp_secret123@github.com/acme/app",
            REPO,
        ),
        // user and password
        (
            "https://user:pass@example.com/repo",
            "https://example.com/repo",
        ),
        // user without password
        ("https://user@example.com/repo", "https://example.com/repo"),
        // credential free untouched
        (REPO, REPO),
        // at sign in path untouched
        ("https://example.com/repo@v2", "https://example.com/repo@v2"),
        // no scheme untouched
        (
            "git@example.com:acme/app.git",
            "git@example.com:acme/app.git",
        ),
        // non http scheme untouched
        ("ftp://user@example.com/repo", "ftp://user@example.com/repo"),
    ];

    for (input, want) in cases {
        assert_eq!(
            without_embedded_credentials(input),
            want,
            "withoutEmbeddedCredentials({input:?})"
        );
    }
}

#[test]
fn normalize_remote_url_table() {
    let cases: Vec<(&str, String, String)> = vec![
        (
            "https strips git suffix",
            format!("{REPO}.git"),
            REPO.to_owned(),
        ),
        (
            "http passes through",
            "http://gitlab.com/acme/app.git".to_owned(),
            "http://gitlab.com/acme/app".to_owned(),
        ),
        (
            "scp-like with user",
            "git@github.com:acme/app.git".to_owned(),
            REPO.to_owned(),
        ),
        (
            "scp-like without user",
            "github.com:acme/app".to_owned(),
            REPO.to_owned(),
        ),
        (
            "ssh url",
            "ssh://git@github.com/acme/app.git".to_owned(),
            REPO.to_owned(),
        ),
        (
            "scheme-less",
            "github.com/acme/app".to_owned(),
            REPO.to_owned(),
        ),
        ("empty", String::new(), String::new()),
        ("whitespace", "  ".to_owned(), String::new()),
        ("garbage with spaces", "not a url".to_owned(), String::new()),
        ("single word", "garbage".to_owned(), String::new()),
        (
            "unsupported scheme",
            "ftp://example.com/app".to_owned(),
            String::new(),
        ),
        (
            "missing path",
            "https://github.com".to_owned(),
            String::new(),
        ),
        ("missing path scp", "github.com:".to_owned(), String::new()),
    ];

    for (name, input, want) in cases {
        assert_eq!(
            normalize_remote_url(&input),
            want,
            "normalize_remote_url({name})"
        );
    }
}

#[test]
fn sort_changelog_records_case_insensitive_with_scopes() {
    let mut records = vec![
        format!("* zulu report {HASH_OLD}"),
        format!("* alpha report {HASH_CREATED}"),
        format!("* **scope:** Beta report {HASH_FIX}"),
    ];
    let want = vec![
        format!("* alpha report {HASH_CREATED}"),
        format!("* **scope:** Beta report {HASH_FIX}"),
        format!("* zulu report {HASH_OLD}"),
    ];

    sort_changelog_records(&mut records);
    assert_eq!(records, want);
}

#[test]
fn sort_changelog_records_leading_asterisks_are_skipped() {
    let mut records = vec!["*b two".to_owned(), "a one".to_owned()];
    sort_changelog_records(&mut records);
    assert_eq!(records, vec!["a one".to_owned(), "*b two".to_owned()]);
}

#[test]
fn civil_date_helper() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    // 1_000_000_000 seconds after the epoch:
    assert_eq!(civil_from_days(1_000_000_000 / 86_400), (2001, 9, 9));
    // 2_000_000_000 seconds after the epoch:
    assert_eq!(civil_from_days(2_000_000_000 / 86_400), (2033, 5, 18));
    // the well-known 1_234_567_890-second moment was 2009-02-13:
    assert_eq!(civil_from_days(1_234_567_890 / 86_400), (2009, 2, 13));
}

#[test]
fn today_utc_renders_iso_date() {
    let today = today_utc();
    let bytes = today.as_bytes();
    assert_eq!(today.len(), 10, "today = {today:?}");
    assert_eq!(bytes[4], b'-');
    assert_eq!(bytes[7], b'-');
    assert!(
        bytes[..4].iter().all(|b| b.is_ascii_digit())
            && bytes[5..7].iter().all(|b| b.is_ascii_digit())
            && bytes[8..10].iter().all(|b| b.is_ascii_digit()),
        "today = {today:?}"
    );
}
