//! Release workflow tests against throwaway Git repositories.

use std::cell::RefCell;
use std::io::{self, Cursor, Write};
use std::path::Path;
use std::rc::Rc;

use super::*;
use crate::color::Stream;
use crate::config::get_default_config;
use crate::utils::testing::{TempDir, commit_test_message, init_git_repo, run_git, try_git};
use crate::version::ReleaseType;

const TAG_V100: &str = "v1.0.0";
const TAG_V110: &str = "v1.1.0";
const TAG_REL_V100: &str = "rel-1.0.0";
const TAG_REL_V110: &str = "rel-1.1.0";

const INITIAL_COMMIT_MSG: &str = "chore: initial import";
const FEAT_WIDGET_MSG: &str = "feat: add widget";
const RELEASE_COMMIT_MSG: &str = "chore(release): 1.1.0";

/// A captured output stream: tests write through a release run and inspect
/// the text afterwards.
#[derive(Clone, Default)]
struct CapturedOutput(Rc<RefCell<Vec<u8>>>);

impl Write for CapturedOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Stream for CapturedOutput {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl CapturedOutput {
    /// The text written so far.
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.borrow()).into_owned()
    }
}

/// Force-mode options answering `input` on the confirmation prompt and
/// capturing the release run output.
fn force_options(
    input: &str,
    output: &CapturedOutput,
    release_type: Option<ReleaseType>,
) -> BumpOptions {
    BumpOptions {
        force: true,
        dry_run: false,
        release_type,
        stdin: Some(Box::new(Cursor::new(input.as_bytes().to_vec()))),
        stdout: Some(Box::new(output.clone())),
    }
}

/// A throwaway repository whose first commit carries the given tag, followed
/// by the given unreleased commits.
struct ReleaseRepo(TempDir);

impl ReleaseRepo {
    fn new(tag: &str, post_tag_commits: &[&str]) -> ReleaseRepo {
        let dir = TempDir::new("release-repo");
        init_git_repo(dir.path());
        commit_test_message(dir.path(), INITIAL_COMMIT_MSG);
        run_git(dir.path(), &["tag", tag]);
        for message in post_tag_commits {
            commit_test_message(dir.path(), message);
        }
        ReleaseRepo(dir)
    }

    fn path(&self) -> &Path {
        self.0.path()
    }
}

fn changelog_path(repo: &Path) -> std::path::PathBuf {
    repo.join("CHANGELOG.md")
}

#[test]
fn bump_version_success() {
    let repo = TempDir::new("bump-success");
    let repo = repo.path();
    init_git_repo(repo);

    std::fs::write(repo.join("VERSION"), "1.0.0\n").expect("could not write VERSION");
    run_git(repo, &["add", "VERSION"]);
    commit_test_message(repo, INITIAL_COMMIT_MSG);
    run_git(repo, &["tag", TAG_V100]);
    commit_test_message(repo, FEAT_WIDGET_MSG);

    let mut cfg = get_default_config();
    cfg.version_filenames = vec!["VERSION".to_owned()];

    let captured = CapturedOutput::default();
    let opts = force_options("", &captured, None);

    bump_version(repo, &cfg, opts).expect("bumpVersion() returned error");

    let version_file =
        std::fs::read_to_string(repo.join("VERSION")).expect("could not read VERSION");
    assert_eq!(version_file, "1.1.0");

    let changelog =
        std::fs::read_to_string(changelog_path(repo)).expect("could not read CHANGELOG.md");
    assert!(
        changelog.contains("## [1.1.0]"),
        "CHANGELOG.md lacks the 1.1.0 section:\n{changelog}"
    );
    assert!(
        changelog.contains("add widget"),
        "CHANGELOG.md lacks the feat commit"
    );

    assert_eq!(
        run_git(repo, &["log", "-1", "--format=%s"]).trim(),
        RELEASE_COMMIT_MSG
    );

    assert_eq!(run_git(repo, &["cat-file", "-t", TAG_V110]).trim(), "tag");

    let tag_ref = format!("refs/tags/{TAG_V110}");
    assert_eq!(
        run_git(repo, &["for-each-ref", "--format=%(contents)", &tag_ref]).trim(),
        "Release 1.1.0"
    );

    assert_eq!(run_git(repo, &["status", "--porcelain"]).trim(), "");

    let output_text = captured.text();
    assert!(
        output_text.contains("Release:     minor"),
        "release plan output missing type:\n{output_text}"
    );
    assert!(
        output_text.contains(TAG_V110),
        "release plan output missing tag:\n{output_text}"
    );
}

#[test]
fn bump_version_updates_package_json() {
    let repo = TempDir::new("bump-package-json");
    let repo = repo.path();
    init_git_repo(repo);

    let pkg_json = "{\n  \"name\": \"app\",\n  \"version\": \"1.0.0\"\n}\n";
    std::fs::write(repo.join("package.json"), pkg_json).expect("could not write package.json");
    run_git(repo, &["add", "package.json"]);
    commit_test_message(repo, INITIAL_COMMIT_MSG);
    run_git(repo, &["tag", TAG_V100]);
    commit_test_message(repo, FEAT_WIDGET_MSG);

    let mut cfg = get_default_config();
    cfg.package_json_filename = "package.json".to_owned();

    bump_version(
        repo,
        &cfg,
        force_options("", &CapturedOutput::default(), None),
    )
    .expect("bumpVersion() returned error");

    let data =
        std::fs::read_to_string(repo.join("package.json")).expect("could not read package.json");
    assert!(
        data.contains("\"version\": \"1.1.0\""),
        "package.json misses the new version:\n{data}"
    );
    assert!(
        !data.contains("\"version\": \"1.0.0\""),
        "package.json kept the old version:\n{data}"
    );
    assert_eq!(run_git(repo, &["status", "--porcelain"]).trim(), "");
}

#[test]
fn bump_version_leaves_package_json_when_disabled() {
    let repo = TempDir::new("bump-package-json-disabled");
    let repo = repo.path();
    init_git_repo(repo);

    let pkg_json = "{\n  \"name\": \"app\",\n  \"version\": \"1.0.0\"\n}\n";
    std::fs::write(repo.join("package.json"), pkg_json).expect("could not write package.json");
    run_git(repo, &["add", "package.json"]);
    commit_test_message(repo, INITIAL_COMMIT_MSG);
    run_git(repo, &["tag", TAG_V100]);
    commit_test_message(repo, FEAT_WIDGET_MSG);

    let cfg = get_default_config();
    bump_version(
        repo,
        &cfg,
        force_options("", &CapturedOutput::default(), None),
    )
    .expect("bumpVersion() returned error");

    let data =
        std::fs::read_to_string(repo.join("package.json")).expect("could not read package.json");
    assert_eq!(
        data, pkg_json,
        "package.json changed although the feature is disabled"
    );
}

#[test]
fn bump_version_dry_run() {
    let repo = ReleaseRepo::new(TAG_V100, &[FEAT_WIDGET_MSG]);
    let repo = repo.path();

    let cfg = get_default_config();
    let captured = CapturedOutput::default();
    let opts = BumpOptions {
        force: true,
        dry_run: true,
        release_type: None,
        stdin: Some(Box::new(Cursor::new(Vec::new()))),
        stdout: Some(Box::new(captured.clone())),
    };

    bump_version(repo, &cfg, opts).expect("bumpVersion() returned error");

    let output = captured.text();
    assert!(
        output.contains("1.1.0"),
        "dry run output lacks next version:\n{output}"
    );
    assert!(
        output.contains(TAG_V110),
        "dry run output lacks the tag:\n{output}"
    );
    assert!(
        output.contains("Dry run: nothing was modified."),
        "dry run output misses the notice:\n{output}"
    );

    assert!(
        !crate::utils::file_exists(changelog_path(repo)),
        "dry run wrote CHANGELOG.md"
    );
    assert_eq!(run_git(repo, &["status", "--porcelain"]).trim(), "");
    assert_eq!(run_git(repo, &["tag", "-l"]).trim(), TAG_V100);
    assert_eq!(
        run_git(repo, &["log", "-1", "--format=%s"]).trim(),
        FEAT_WIDGET_MSG
    );
}

#[test]
fn bump_version_refuses_dirty_worktree() {
    let repo = ReleaseRepo::new(TAG_V100, &[FEAT_WIDGET_MSG]);
    let repo = repo.path();

    std::fs::write(repo.join("scratch.txt"), "wip").expect("could not create untracked file");

    let cfg = get_default_config();
    let err = bump_version(
        repo,
        &cfg,
        force_options("", &CapturedOutput::default(), None),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("working tree is not clean"), "error = {err}");

    assert!(
        !crate::utils::file_exists(changelog_path(repo)),
        "refused release wrote CHANGELOG.md"
    );
    assert_eq!(run_git(repo, &["tag", "-l"]).trim(), TAG_V100);
}

#[test]
fn bump_version_refuses_staged_changes() {
    let repo = ReleaseRepo::new(TAG_V100, &[FEAT_WIDGET_MSG]);
    let repo = repo.path();

    std::fs::write(repo.join("scratch.txt"), "wip").expect("could not create file");
    run_git(repo, &["add", "scratch.txt"]);

    let cfg = get_default_config();
    let err = bump_version(
        repo,
        &cfg,
        force_options("", &CapturedOutput::default(), None),
    )
    .unwrap_err()
    .to_string();
    assert!(
        err.contains(
            "refusing to release: changes are already staged - commit or reset them first"
        ),
        "error = {err}"
    );

    assert!(
        !crate::utils::file_exists(changelog_path(repo)),
        "refused release wrote CHANGELOG.md"
    );
}

#[test]
fn bump_version_prompt_declined() {
    let repo = ReleaseRepo::new(TAG_V100, &[FEAT_WIDGET_MSG]);
    let repo = repo.path();

    let cfg = get_default_config();
    let captured = CapturedOutput::default();
    let opts = BumpOptions {
        force: false,
        dry_run: false,
        release_type: None,
        stdin: Some(Box::new(Cursor::new(b"n\n".to_vec()))),
        stdout: Some(Box::new(captured.clone())),
    };

    bump_version(repo, &cfg, opts).expect("bumpVersion() returned error");

    let output = captured.text();
    assert!(
        output.contains("Release 1.1.0? [y/N]: "),
        "confirmation prompt not shown:\n{output:?}"
    );
    assert!(
        output.contains("Aborted."),
        "declined prompt did not abort:\n{output}"
    );

    assert!(
        !crate::utils::file_exists(changelog_path(repo)),
        "declined release wrote CHANGELOG.md"
    );
    assert_eq!(run_git(repo, &["tag", "-l"]).trim(), TAG_V100);
}

#[test]
fn bump_version_refuses_existing_next_tag() {
    let repo = ReleaseRepo::new(TAG_V100, &[FEAT_WIDGET_MSG]);
    let repo = repo.path();

    // Occupy the next tag name on a commit that is not reachable from HEAD so
    // that the current version stays 1.0.0 while the next tag already exists.
    commit_test_message(repo, "chore: throwaway");
    let throwaway_hash = run_git(repo, &["rev-parse", "HEAD"]).trim().to_owned();
    run_git(repo, &["reset", "--hard", "HEAD~1"]);
    run_git(repo, &["tag", TAG_V110, &throwaway_hash]);

    let cfg = get_default_config();
    let err = bump_version(
        repo,
        &cfg,
        force_options("", &CapturedOutput::default(), None),
    )
    .unwrap_err()
    .to_string();
    assert!(
        err.contains(&format!("release tag {TAG_V110} already exists")),
        "error = {err}"
    );

    assert!(
        !crate::utils::file_exists(changelog_path(repo)),
        "refused release wrote CHANGELOG.md"
    );
    assert_eq!(
        run_git(repo, &["log", "-1", "--format=%s"]).trim(),
        FEAT_WIDGET_MSG
    );
}

#[test]
fn bump_version_nothing_to_release() {
    let repo = ReleaseRepo::new(TAG_V100, &["chore: tidy up"]);
    let repo = repo.path();

    let cfg = get_default_config();
    bump_version(
        repo,
        &cfg,
        force_options("", &CapturedOutput::default(), None),
    )
    .expect("bumpVersion() returned error");

    assert!(
        !crate::utils::file_exists(changelog_path(repo)),
        "no-op release wrote CHANGELOG.md"
    );
    assert_eq!(run_git(repo, &["tag", "-l"]).trim(), TAG_V100);
}

#[test]
fn bump_version_forced_release_type() {
    let repo = ReleaseRepo::new(TAG_V100, &["chore: tidy up"]);
    let repo = repo.path();

    let cfg = get_default_config();
    bump_version(
        repo,
        &cfg,
        force_options("", &CapturedOutput::default(), Some(ReleaseType::Minor)),
    )
    .expect("bumpVersion() returned error");

    run_git(
        repo,
        &["rev-parse", "--verify", &format!("refs/tags/{TAG_V110}")],
    );
    assert_eq!(
        run_git(repo, &["log", "-1", "--format=%s"]).trim(),
        RELEASE_COMMIT_MSG
    );
}

#[test]
fn bump_version_forced_release_type_refuses_empty_window() {
    let repo = TempDir::new("bump-empty-window");
    let repo = repo.path();
    init_git_repo(repo);
    commit_test_message(repo, INITIAL_COMMIT_MSG);
    run_git(repo, &["tag", TAG_V100]);

    let cfg = get_default_config();
    bump_version(
        repo,
        &cfg,
        force_options("", &CapturedOutput::default(), Some(ReleaseType::Minor)),
    )
    .expect("bumpVersion() returned error");

    assert!(
        !crate::utils::file_exists(changelog_path(repo)),
        "empty window release wrote CHANGELOG.md"
    );
    assert_eq!(run_git(repo, &["tag", "-l"]).trim(), TAG_V100);
}

#[test]
fn bump_version_custom_tag_format() {
    let repo = ReleaseRepo::new(TAG_REL_V100, &[FEAT_WIDGET_MSG]);
    let repo = repo.path();

    let mut cfg = get_default_config();
    cfg.version_tag_format = "rel-{version}".to_owned();

    bump_version(
        repo,
        &cfg,
        force_options("", &CapturedOutput::default(), None),
    )
    .expect("bumpVersion() returned error");

    run_git(
        repo,
        &[
            "rev-parse",
            "--verify",
            &format!("refs/tags/{TAG_REL_V110}"),
        ],
    );

    let (_, _, ok) = try_git(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/tags/{TAG_V110}"),
        ],
    );
    assert!(!ok, "release also created the unexpected tag {TAG_V110}");
}

#[test]
fn bump_version_pushes_to_origin() {
    let repo = ReleaseRepo::new(TAG_V100, &[FEAT_WIDGET_MSG]);
    let repo = repo.path();
    let origin = TempDir::new("bump-origin");

    run_git(origin.path(), &["init", "--bare"]);
    run_git(
        repo,
        &[
            "remote",
            "add",
            "origin",
            origin.path().to_str().expect("utf-8 origin path"),
        ],
    );

    let branch = run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_owned();
    let upstream = format!("{branch}:{branch}");
    run_git(repo, &["push", "--set-upstream", "origin", &upstream]);

    let mut cfg = get_default_config();
    cfg.should_push_to_origin = true;

    bump_version(
        repo,
        &cfg,
        force_options("", &CapturedOutput::default(), None),
    )
    .expect("bumpVersion() returned error");

    let git_dir = format!("--git-dir={}", origin.path().display());
    let remote_tags = run_git(repo, &[&git_dir, "tag"]);
    assert!(
        remote_tags.contains(TAG_V110),
        "remote tags {remote_tags:?} lack {TAG_V110}"
    );

    let remote_head = run_git(repo, &[&git_dir, "log", "-1", "--format=%s"])
        .trim()
        .to_owned();
    assert_eq!(remote_head, RELEASE_COMMIT_MSG);
}

#[test]
fn bump_version_prints_next_steps_without_push() {
    let repo = ReleaseRepo::new(TAG_V100, &[FEAT_WIDGET_MSG]);
    let repo = repo.path();

    let cfg = get_default_config();
    let captured = CapturedOutput::default();
    bump_version(repo, &cfg, force_options("", &captured, None))
        .expect("bumpVersion() returned error");

    let branch = run_git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_owned();
    let output = captured.text();
    assert!(
        output.contains("Next steps:"),
        "release output misses next steps:\n{output}"
    );
    assert!(
        output.contains(&format!("git push origin {branch} --follow-tags")),
        "release output misses the push command:\n{output}"
    );
}

#[test]
fn plan_release_forced_type_overrides_derived_type() {
    let repo = ReleaseRepo::new(TAG_V100, &["chore: tidy up"]);
    let repo = repo.path();

    let cfg = get_default_config();
    let opts = BumpOptions {
        force: false,
        dry_run: false,
        release_type: Some(ReleaseType::Patch),
        stdin: None,
        stdout: None,
    };

    let plan = plan_release(repo, &cfg, &opts).expect("planRelease() returned error");

    assert_eq!(plan.release_type, ReleaseType::Patch);
    assert_eq!(plan.next.to_string(), "1.0.1");
    assert_eq!(plan.tag_name, "v1.0.1");
}

#[test]
fn plan_release_nothing_to_release() {
    let repo = ReleaseRepo::new(TAG_V100, &["chore: tidy up"]);
    let repo = repo.path();

    let cfg = get_default_config();
    let plan =
        plan_release(repo, &cfg, &BumpOptions::default()).expect("planRelease() returned error");
    assert_eq!(plan.release_type, ReleaseType::None);
}

#[test]
fn require_clean_worktree_checks() {
    let repo = TempDir::new("worktree-clean");
    let repo = repo.path();
    init_git_repo(repo);

    assert!(require_clean_worktree(repo).is_ok(), "clean repository");

    std::fs::write(repo.join("scratch.txt"), "x").expect("could not write file");
    let err = require_clean_worktree(repo).unwrap_err().to_string();
    assert!(
        err.contains("working tree is not clean"),
        "untracked file error = {err}"
    );

    run_git(repo, &["add", "scratch.txt"]);
    let err = require_clean_worktree(repo).unwrap_err().to_string();
    assert!(err.contains("already staged"), "staged file error = {err}");
}
