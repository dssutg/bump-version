//! Release planning and the guards around applying a release: computing the
//! next version from the Git history, showing the plan, then writing the
//! release files, commit and tag.

use std::io::Read;
use std::path::Path;

use crate::changelog::{generate_changelog, generate_new_changelog_head};
use crate::color::{ANSI_GREEN, Stream, color_enabled, paint};
use crate::commit::warn_no_commits_that_can_increment_current_version;
use crate::config::{Config, VERSION_TAG_FORMAT_SUBST, write_file_with_mode};
use crate::error::{Error, Result};
use crate::package_json::update_package_json_version;
use crate::utils::{ask_yes_no, git, warn};
use crate::version::{ReleaseType, Version, get_current_version_by_git_tag, version_to_git_tag};

/// BumpOptions carries the command line switches that influence the release
/// workflow.
#[derive(Default)]
pub struct BumpOptions {
    pub force: bool,   // skip the confirmation prompt (-force)
    pub dry_run: bool, // write nothing, only print the plan (-dry-run)
    /// Some: derived from commits; otherwise the -type override
    pub release_type: Option<ReleaseType>,
    /// input for the confirmation prompt (None: the standard input)
    pub stdin: Option<Box<dyn Read>>,
    /// destination for release output (None: the standard output)
    pub stdout: Option<Box<dyn Stream>>,
}

/// releasePlan is everything needed to apply a release. It is computed by
/// plan_release before anything is written.
#[derive(Debug, Default)]
pub struct ReleasePlan {
    pub current: Version,
    pub next: Version,
    pub release_type: ReleaseType,
    pub tag_name: String,
    pub stats: crate::commit::CommitStats,
    pub url_base: String,
}

/// plan_release computes the next release from the Git history and never
/// writes anything. A plan with release_type == `None` means there is
/// nothing to release; the reason has already been warned on stderr.
pub fn plan_release(repo: &Path, cfg: &Config, opts: &BumpOptions) -> Result<ReleasePlan> {
    let current_version = get_current_version_by_git_tag(repo, &cfg.version_tag_format)?;

    let cur_tag = version_to_git_tag(&current_version, &cfg.version_tag_format);

    let stats = crate::commit::process_commit_messages(
        repo,
        &cur_tag,
        cfg.ignore_invalid_commits,
        &cfg.allowed_commit_kinds,
    )?;

    let release_type = match opts.release_type {
        Some(forced) => forced,
        None => stats.determine_release_type(),
    };

    // An empty release window refuses to release even with a forced type.
    if stats.in_window_commits == 0 {
        warn("nothing to release");
        return Ok(ReleasePlan {
            current: current_version,
            stats,
            release_type: ReleaseType::None,
            ..ReleasePlan::default()
        });
    }
    if release_type == ReleaseType::None {
        warn_no_commits_that_can_increment_current_version();
        return Ok(ReleasePlan {
            current: current_version,
            stats,
            release_type: ReleaseType::None,
            ..ReleasePlan::default()
        });
    }

    let mut next = current_version.clone();
    next.increment_to(release_type);

    let tag_name = version_to_git_tag(&next, &cfg.version_tag_format);

    Ok(ReleasePlan {
        current: current_version,
        next,
        release_type,
        tag_name,
        stats,
        url_base: crate::changelog::resolve_url_base(repo, "origin"),
    })
}
impl ReleasePlan {
    /// print_plan writes the human-readable release plan to w.
    pub fn print_plan(&self, w: &mut dyn Stream) {
        let color = color_enabled(w);
        let _ = writeln!(w, "Release:     {}", self.release_type);
        let _ = writeln!(
            w,
            "Version:     {}",
            paint(
                color,
                ANSI_GREEN,
                &format!("{} -> {}", self.current, self.next)
            )
        );
        let _ = writeln!(
            w,
            "Tag:         {}",
            paint(color, ANSI_GREEN, &self.tag_name)
        );
    }
}

/// bump_version plans a release, shows it to the user and applies it. A dry
/// run stops after printing the plan; without `force` the release is
/// confirmed interactively.
pub fn bump_version(repo: &Path, cfg: &Config, mut opts: BumpOptions) -> Result<()> {
    let plan = plan_release(repo, cfg, &opts)?;
    if plan.release_type == ReleaseType::None {
        return Ok(());
    }

    let mut stdout: Box<dyn Stream> = opts
        .stdout
        .take()
        .unwrap_or_else(|| Box::new(std::io::stdout()));
    let mut stdin: Box<dyn Read> = opts
        .stdin
        .take()
        .unwrap_or_else(|| Box::new(std::io::stdin()));

    plan.print_plan(&mut *stdout);

    // Show what will be committed, then leave everything untouched on a dry run.
    let _ = writeln!(stdout);
    let head =
        generate_new_changelog_head(&plan.current, &plan.next, &plan.stats, cfg, &plan.url_base);
    let _ = write!(stdout, "{head}");

    if opts.dry_run {
        let _ = writeln!(stdout, "Dry run: nothing was modified.");
        let _ = stdout.flush();
        return Ok(());
    }

    // Guards run before the prompt so that a doomed release fails fast.
    if tag_exists(repo, &plan.tag_name)? {
        return Err(Error::msg(format!(
            "release tag {} already exists",
            plan.tag_name
        )));
    }

    require_clean_worktree(repo)?;

    if !opts.force {
        let question = format!("Release {}?", plan.next);
        if !ask_yes_no(&mut *stdin, &mut *stdout, &question, false) {
            let _ = writeln!(stdout, "Aborted.");
            let _ = stdout.flush();
            return Ok(());
        }
    }

    apply_release_plan(repo, cfg, plan, &mut *stdout)
}

/// apply_release_plan writes the changelog and the version files, commits them
/// and tags the release. There is deliberately no rollback: if something fails
/// midway, the `cancel` command undoes the release.
pub fn apply_release_plan(
    repo: &Path,
    cfg: &Config,
    plan: ReleasePlan,
    stdout: &mut dyn Stream,
) -> Result<()> {
    let new_version_str = plan.next.to_string();

    let _ = writeln!(
        stdout,
        "{}",
        paint(
            color_enabled(stdout),
            ANSI_GREEN,
            &format!("Bumping next release version {new_version_str}...")
        )
    );
    let _ = stdout.flush();

    generate_changelog(
        repo,
        &plan.current,
        &plan.next,
        &plan.stats,
        cfg,
        &plan.url_base,
    )?;

    for name in &cfg.version_filenames {
        write_file_with_mode(&repo.join(name), new_version_str.as_bytes(), 0o600)
            .map_err(|err| Error::msg(format!("could not write version file {name}: {err}")))?;
    }

    let mut files_to_add = vec![cfg.change_log_filename.as_str()];
    files_to_add.extend(cfg.version_filenames.iter().map(String::as_str));
    if !cfg.package_json_filename.is_empty() {
        update_package_json_version(&repo.join(&cfg.package_json_filename), &new_version_str)?;
        files_to_add.push(cfg.package_json_filename.as_str());
    }

    let mut add_args = vec!["add", "--"];
    add_args.extend(files_to_add.iter().copied());
    let out = git(repo, &add_args)?;
    if !out.success {
        return Err(Error::msg(format!(
            "error staging files to Git: {}",
            out.stderr
        )));
    }

    let commit_message = cfg
        .bump_version_commit
        .replace(VERSION_TAG_FORMAT_SUBST, &new_version_str);
    let out = git(repo, &["commit", "-m", commit_message.as_str()])?;
    if !out.success {
        return Err(Error::msg(format!(
            "error committing changes to Git: {}",
            out.stderr
        )));
    }

    let tag_message = format!("Release {new_version_str}");
    let out = git(
        repo,
        &[
            "tag",
            "-a",
            plan.tag_name.as_str(),
            "-m",
            tag_message.as_str(),
        ],
    )?;
    if !out.success {
        return Err(Error::msg(format!(
            "error adding Git tag for this release: {}",
            out.stderr
        )));
    }

    if cfg.should_push_to_origin {
        let out = git(repo, &["push", "--follow-tags", "origin"])?;
        if !out.success {
            return Err(Error::msg(format!(
                "error pushing to remote Git repository: {}",
                out.stderr
            )));
        }
        return Ok(());
    }

    let branch = current_git_branch(repo).unwrap_or_else(|_| "HEAD".to_owned());
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Next steps:");
    let _ = writeln!(stdout, "  git push origin {branch} --follow-tags");
    let _ = stdout.flush();

    Ok(())
}

/// tag_exists reports whether the Git tag already exists. Only an unknown
/// revision counts as "does not exist"; real Git failures are returned.
fn tag_exists(repo: &Path, tag_name: &str) -> Result<bool> {
    let out = git(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/tags/{tag_name}"),
        ],
    )?;
    if !out.success {
        if out.stderr.trim().is_empty() {
            return Ok(false);
        }
        return Err(Error::msg(format!(
            "error checking Git tag {tag_name}: {}",
            out.stderr.trim()
        )));
    }
    Ok(true)
}

/// require_clean_worktree refuses to release over uncommitted work: staged
/// changes would be swept into the release commit and a dirty tree would leave
/// the release half-applied.
pub fn require_clean_worktree(repo: &Path) -> Result<()> {
    let out = git(repo, &["status", "--porcelain"])?;
    if !out.success {
        return Err(Error::msg(format!(
            "error checking the worktree status: {}",
            out.stderr
        )));
    }

    for line in out.stdout.split('\n') {
        if line.len() < 2 {
            continue;
        }
        // In porcelain format the first column is the index status; "??" and
        // "!!" mark untracked and ignored files, which are not staged.
        let index_status = line.as_bytes()[0];
        if index_status != b' ' && index_status != b'?' && index_status != b'!' {
            return Err(Error::msg(
                "refusing to release: changes are already staged - commit or reset them first",
            ));
        }
    }

    if !out.stdout.trim().is_empty() {
        return Err(Error::msg(
            "working tree is not clean - commit your changes before releasing",
        ));
    }

    Ok(())
}

/// current_git_branch returns the name of the currently checked-out branch.
pub fn current_git_branch(repo: &Path) -> Result<String> {
    let out = git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if !out.success {
        return Err(Error::msg(format!(
            "error getting the current branch from Git: {}",
            out.stderr
        )));
    }
    Ok(out.stdout.trim().to_owned())
}

#[cfg(test)]
mod tests;
