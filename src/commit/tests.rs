//! Tests for Conventional Commits parsing and the release statistics.

use super::*;
use crate::error::Error;
use crate::utils::testing::{TempDir, commit_test_message, init_git_repo, run_git};
use crate::version::ReleaseType;

/// Expectations of the parse table; only the non-empty fields are checked.
#[derive(Default)]
struct Want {
    err_contains: String,
    kind: String,
    scope: String,
    hash: String,
    description: String,
    body: String,
    refers_to: Vec<String>,
    breaking: bool,
}

#[allow(clippy::too_many_arguments)]
fn want(
    err_contains: &str,
    kind: &str,
    scope: &str,
    hash: &str,
    description: &str,
    body: &str,
    refers_to: &[&str],
    breaking: bool,
) -> Want {
    Want {
        err_contains: err_contains.to_owned(),
        kind: kind.to_owned(),
        scope: scope.to_owned(),
        hash: hash.to_owned(),
        description: description.to_owned(),
        body: body.to_owned(),
        refers_to: refers_to.iter().map(|hash| (*hash).to_owned()).collect(),
        breaking,
    }
}

#[test]
fn parse_conventional_commit_table() {
    let cases: Vec<(&str, &str, bool, Vec<String>, Want)> = vec![
        (
            "leading whitespace",
            " leading: desc",
            false,
            vec![KIND_FEAT.to_owned()],
            want(
                "commit message line number 1 (` leading: desc`) should not have leading or trailing whitespace",
                "",
                "",
                "",
                "",
                "",
                &[],
                false,
            ),
        ),
        (
            "trailing whitespace",
            "trailing: desc ",
            false,
            vec![KIND_FEAT.to_owned()],
            want("trailing whitespace", "", "", "", "", "", &[], false),
        ),
        (
            "empty message",
            "",
            false,
            vec![KIND_FEAT.to_owned()],
            want("empty commit message", "", "", "", "", "", &[], false),
        ),
        (
            "empty header",
            "\nbody",
            false,
            vec![KIND_FEAT.to_owned()],
            want(
                "empty commit message header",
                "",
                "",
                "",
                "",
                "",
                &[],
                false,
            ),
        ),
        (
            "valid hash prefix",
            "0123456 feat: add stuff",
            true,
            vec![KIND_FEAT.to_owned()],
            want("", KIND_FEAT, "", "0123456", "add stuff", "", &[], false),
        ),
        (
            "invalid hash content",
            "ZZZZZZZ feat: x",
            true,
            vec![KIND_FEAT.to_owned()],
            want("lowercase hexadecimal", "", "", "", "", "", &[], false),
        ),
        (
            "missing space after hash",
            "0123456feat: x",
            true,
            vec![KIND_FEAT.to_owned()],
            want(
                "commit hash length must be either 7 or 40 characters long",
                "",
                "",
                "",
                "",
                "",
                &[],
                false,
            ),
        ),
        (
            "kind scope bang",
            "feat(scope)!: do something",
            false,
            vec![KIND_FEAT.to_owned()],
            want("", KIND_FEAT, "scope", "", "do something", "", &[], true),
        ),
        (
            "missing colon",
            "feat scope do",
            false,
            vec![KIND_FEAT.to_owned()],
            want("must have colon", "", "", "", "", "", &[], false),
        ),
        (
            "space before paren",
            "feat (scope): x",
            false,
            vec![KIND_FEAT.to_owned()],
            want("spaces before `('", "", "", "", "", "", &[], false),
        ),
        (
            "empty scope",
            "feat(): x",
            false,
            vec![KIND_FEAT.to_owned()],
            want("scope cannot be empty", "", "", "", "", "", &[], false),
        ),
        (
            "no space after colon",
            "feat:do",
            false,
            vec![KIND_FEAT.to_owned()],
            want("space after colon", "", "", "", "", "", &[], false),
        ),
        (
            "description ends with period",
            "feat: finishes.",
            false,
            vec![KIND_FEAT.to_owned()],
            want("should not end with period", "", "", "", "", "", &[], false),
        ),
        (
            "kind not allowed",
            "chore: x",
            false,
            vec![KIND_FEAT.to_owned(), KIND_FIX.to_owned()],
            want("not allowed", "", "", "", "", "", &[], false),
        ),
        (
            "body and refs",
            "fix: something broke\n\nthis is the body line 1\nline 2\n\nRefs: 0123456, 89abcde",
            false,
            vec![KIND_FIX.to_owned()],
            want(
                "",
                "",
                "",
                "",
                "",
                "this is the body line 1\nline 2",
                &["0123456", "89abcde"],
                false,
            ),
        ),
        (
            "colons without space in body prose",
            "fix: stop the forms plugin from drawing a blue focus ring\n\nthe browser renders: a \
             white background, and a blue.600 box-shadow\npublish:dry\nfocus ring that \
             contradicts *:focus { outline: none }",
            false,
            vec![KIND_FIX.to_owned()],
            want(
                "",
                "",
                "",
                "",
                "",
                "the browser renders: a white background, and a blue.600 box-shadow\npublish:dry\nfocus ring that contradicts *:focus { outline: none }",
                &[],
                false,
            ),
        ),
        (
            "non refs colon line is ignored",
            "fix: something broke\n\nSigned-off-by:someone",
            false,
            vec![KIND_FIX.to_owned()],
            want("", "", "", "", "", "Signed-off-by:someone", &[], false),
        ),
        (
            "breaking in footer",
            "feat: add x\n\nNotes\n\nBREAKING CHANGE: API changed",
            false,
            vec![KIND_FEAT.to_owned()],
            want("", "", "", "", "", "", &[], true),
        ),
        (
            "refs wrong case",
            "fix: x\n\nrefs: 0123456",
            false,
            vec![KIND_FIX.to_owned()],
            want("consistent case", "", "", "", "", "", &[], false),
        ),
        (
            "bad hash in refs",
            "fix: x\n\nRefs: badhash",
            false,
            vec![KIND_FIX.to_owned()],
            want("invalid hash", "", "", "", "", "", &[], false),
        ),
        (
            "no space after colon in end",
            "fix: x\n\nRefs:0123456",
            false,
            vec![KIND_FIX.to_owned()],
            want("one space after colon", "", "", "", "", "", &[], false),
        ),
        (
            "comma spacing in refs",
            "fix: x\n\nRefs: 0123456,89abcdef",
            false,
            vec![KIND_FIX.to_owned()],
            want(
                "delimited by comma and a single space",
                "",
                "",
                "",
                "",
                "",
                &[],
                false,
            ),
        ),
        (
            "revert requires refs",
            "revert: undo",
            false,
            vec!["revert".to_owned()],
            want(
                "revert commits must have hashes of the commits they revert; add `Refs: ` attribute to commit footer",
                "",
                "",
                "",
                "",
                "",
                &[],
                false,
            ),
        ),
        (
            "valid revert with refs",
            "revert: undo\n\nRefs: 0123456",
            false,
            vec!["revert".to_owned()],
            want("", "", "", "", "", "", &["0123456"], false),
        ),
    ];

    for (name, message, has_hash, allowed_kinds, want) in cases {
        let parsed = parse_conventional_commit(message, has_hash, &allowed_kinds);

        if !want.err_contains.is_empty() {
            let err = match parsed {
                Err(err) => err.to_string(),
                Ok(_) => panic!("{name}: expected an error, got a commit"),
            };
            assert!(
                err.contains(&want.err_contains),
                "{name}: expected error containing {want:?}, got err={err:?}",
                want = want.err_contains
            );
            continue;
        }

        let commit = match parsed {
            Ok(commit) => commit,
            Err(err) => panic!("{name}: unexpected error: {err}"),
        };
        if !want.kind.is_empty() {
            assert_eq!(commit.kind, want.kind, "{name}: kind");
        }
        if !want.scope.is_empty() {
            assert_eq!(commit.scope, want.scope, "{name}: scope");
        }
        if !want.hash.is_empty() {
            assert_eq!(commit.hash, want.hash, "{name}: hash");
        }
        if !want.description.is_empty() {
            assert_eq!(commit.description, want.description, "{name}: description");
        }
        if !want.body.is_empty() {
            assert_eq!(commit.body, want.body, "{name}: body");
        }
        assert_eq!(commit.breaking, want.breaking, "{name}: breaking");
        if !want.refers_to.is_empty() {
            assert_eq!(commit.refers_to, want.refers_to, "{name}: refers_to");
        }
    }
}

#[test]
fn commit_stats_determine_release_type() {
    let stats_with_kind = |kind: &str, breaking: bool| {
        let mut stats = CommitStats {
            has_breaking_change: breaking,
            ..CommitStats::default()
        };
        stats
            .by_kind
            .entry(kind.to_owned())
            .or_default()
            .push(Commit::default());
        stats
    };

    let cases = [
        (
            "no commits is none",
            CommitStats::default(),
            ReleaseType::None,
        ),
        (
            "breaking change is major",
            stats_with_kind(KIND_FEAT, true),
            ReleaseType::Major,
        ),
        (
            "feat is minor",
            stats_with_kind(KIND_FEAT, false),
            ReleaseType::Minor,
        ),
        (
            "fix is patch",
            stats_with_kind(KIND_FIX, false),
            ReleaseType::Patch,
        ),
        (
            "perf is patch",
            stats_with_kind(KIND_PERF, false),
            ReleaseType::Patch,
        ),
        (
            "chore is none",
            stats_with_kind(KIND_CHORE, false),
            ReleaseType::None,
        ),
        (
            "breaking wins over feat",
            stats_with_kind(KIND_FIX, true),
            ReleaseType::Major,
        ),
    ];

    for (name, stats, want) in cases {
        assert_eq!(stats.determine_release_type(), want, "{name}");
    }
}

#[test]
fn process_commit_messages_determines_release_types() {
    let cases: Vec<(&str, Vec<&str>, ReleaseType)> = vec![
        (
            "feat is minor",
            vec!["feat: add gadget"],
            ReleaseType::Minor,
        ),
        (
            "fix is patch",
            vec!["fix: repair widget"],
            ReleaseType::Patch,
        ),
        (
            "perf is patch",
            vec!["perf: speed up search"],
            ReleaseType::Patch,
        ),
        ("chore is none", vec!["chore: tidy up"], ReleaseType::None),
        (
            "body colons are valid",
            vec![
                "fix: repair the focus ring\n\nthe browser renders: a white background,\nand *:focus { outline: none } contradicted `publish:dry`",
            ],
            ReleaseType::Patch,
        ),
        (
            "breaking footer is major",
            vec!["feat: add api\n\nelaborated details\n\nBREAKING CHANGE: api changed"],
            ReleaseType::Major,
        ),
        (
            "bang is major",
            vec!["feat!: drop old api"],
            ReleaseType::Major,
        ),
    ];

    for (name, commits, want) in cases {
        let dir = TempDir::new("commits-release-types");
        let repo = dir.path();
        init_git_repo(repo);
        for message in &commits {
            commit_test_message(repo, message);
        }

        let allowed_kinds = crate::config::get_default_config().allowed_commit_kinds;
        let stats = process_commit_messages(repo, "", false, &allowed_kinds)
            .unwrap_or_else(|err| panic!("{name}: processCommitMessages() returned error: {err}"));

        assert_eq!(stats.determine_release_type(), want, "{name}");
        assert_eq!(
            stats.in_window_commits,
            commits.len(),
            "{name}: in-window count"
        );
    }
}

#[test]
fn process_commit_messages_since_tag_excludes_pre_tag_commits() {
    let dir = TempDir::new("commits-since-tag");
    let repo = dir.path();
    init_git_repo(repo);

    commit_test_message(repo, "chore: initial setup");
    run_git(repo, &["tag", "v1.0.0"]);
    commit_test_message(repo, "fix: repair widget");
    commit_test_message(repo, "feat: add gadget");

    let stats = process_commit_messages(
        repo,
        "v1.0.0",
        false,
        &crate::config::get_default_config().allowed_commit_kinds,
    )
    .expect("processCommitMessages() returned error");

    assert_eq!(stats.in_window_commits, 2);
    assert_eq!(stats.by_kind.len(), 2, "ByKind = {:?}", stats.by_kind);
    assert!(
        stats
            .by_kind
            .get(KIND_CHORE)
            .is_none_or(|commits| commits.is_empty()),
        "pre-tag chore commit leaked into ByKind"
    );
    assert_eq!(stats.determine_release_type(), ReleaseType::Minor);
    assert!(!stats.has_breaking_change);
}

#[test]
fn process_commit_messages_invalid_commits() {
    let dir = TempDir::new("commits-invalid");
    let repo = dir.path();
    init_git_repo(repo);

    commit_test_message(repo, "feat: valid work");
    commit_test_message(repo, "this message is invalid");

    let allowed_kinds = crate::config::get_default_config().allowed_commit_kinds;

    match process_commit_messages(repo, "", false, &allowed_kinds) {
        Err(Error::InvalidCommits) => {}
        other => panic!("strict mode error = {other:?}, want ErrFoundInvalidCommits"),
    }

    let stats = process_commit_messages(repo, "", true, &allowed_kinds)
        .expect("lenient mode returned error");
    assert_eq!(stats.in_window_commits, 1);
    assert_eq!(stats.determine_release_type(), ReleaseType::Minor);
}

#[test]
fn process_commit_messages_reverted_pre_tag_commit() {
    let dir = TempDir::new("commits-revert");
    let repo = dir.path();
    init_git_repo(repo);

    commit_test_message(repo, "feat!: remove old api");
    let full_hash = run_git(repo, &["rev-parse", "HEAD"]).trim().to_owned();
    run_git(repo, &["tag", "v1.0.0"]);
    commit_test_message(
        repo,
        &format!("revert: remove old api\n\nRefs: {full_hash}"),
    );

    let stats = process_commit_messages(
        repo,
        "v1.0.0",
        false,
        &crate::config::get_default_config().allowed_commit_kinds,
    )
    .expect("processCommitMessages() returned error");

    assert_eq!(stats.in_window_commits, 1);
    assert!(
        stats.by_kind.is_empty(),
        "ByKind = {:?}, want no kind sections (revert commits are excluded)",
        stats.by_kind
    );
    assert_eq!(stats.reverted_pre_tag.len(), 1);
    assert_eq!(stats.reverted_pre_tag[0].hash, full_hash);
    assert!(
        stats.has_breaking_change,
        "HasBreakingChange = false, want true (reverted pre-tag breaking commit)"
    );
    assert_eq!(stats.determine_release_type(), ReleaseType::Major);
}
