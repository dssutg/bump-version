//! Command bump-version automates semantic versioning, changelog
//! generation, Conventional Commits validation and Git releases.
//!
//! The codebase is split by responsibility:
//!
//!   - main.rs: CLI entry point, option and command dispatch, help.
//!   - bump.rs: release planning and the guards around applying a release.
//!   - version.rs: strict SemVer parsing, incrementing, comparison and tag
//!     formats.
//!   - commit.rs: Conventional Commits parsing and release statistics from
//!     the Git history.
//!   - changelog.rs: changelog rendering, remote compare links and
//!     credential stripping.
//!   - package_json.rs: optional package.json "version" field bump.
//!   - config.rs: bump-version.cfg parsing, defaults and validation.
//!   - color.rs: POSIX terminal color detection and painting.
//!   - utils.rs: Git helpers, prompting and process exit helpers.
//!   - error.rs: the shared error type.

use std::fs;
use std::io::{self, ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::process;

use bump::BumpOptions;
use changelog::generate_new_changelog_head;
use color::{ANSI_GREEN, ANSI_YELLOW, color_enabled, paint};
use commit::{get_git_commit_lines, parse_conventional_commit};
use config::{
    DEFAULT_CONFIG_FILENAME, get_default_config, load_config, save_config, write_file_with_mode,
};
use error::{Error, Result};
use utils::{
    EXIT_FAILURE, EXIT_USAGE, GIT_COMMAND, PROGRAM_NAME, ask_yes_no, command_output, fatal,
    fatal_on_err, file_exists, unwrap_fatal, warn,
};
use version::{
    ReleaseType, get_current_version_by_git_tag, get_current_version_tag, parse_release_type,
    version_to_git_tag,
};

mod bump;
mod changelog;
mod color;
mod commit;
mod config;
mod error;
mod package_json;
mod utils;
mod version;

/// Prints a line to standard output while ignoring write errors, so a
/// closed downstream pipe cannot crash the tool.
macro_rules! outln {
    () => {{
        let _ = writeln!(std::io::stdout());
    }};
    ($($arg:tt)*) => {{
        let _ = writeln!(std::io::stdout(), $($arg)*);
    }};
}

/// Prints a line to standard error (diagnostics belong there), also tolerant
/// of write errors.
macro_rules! errln {
    ($($arg:tt)*) => {{
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

/// The program version, embedded at compile time from the VERSION file.
const PROGRAM_VERSION: &str = include_str!("../VERSION");

/// The commit-msg hook file, relative to the repository root.
const COMMIT_MSG_HOOK_PATH: &str = ".git/hooks/commit-msg";

/// main parses options and commands, locates the repository root and runs
/// the selected command inside it.
fn main() {
    // Make sure all relative paths of the configuration are resolved
    // against the Git repository root
    let repo = unwrap_fatal(get_repo_abs_path());

    let mut config_filename: Option<String> = None; // not determined yet

    let mut force_mode = false;
    let mut dry_run = false;
    let mut forced_release_type: Option<ReleaseType> = None;

    let args: Vec<String> = std::env::args().skip(1).collect(); // get rid of executable name from args
    let mut index = 0;

    // Process all options; they must precede the command
    while index < args.len() && args[index].starts_with('-') {
        match args[index].as_str() {
            "-config" => {
                if index + 1 >= args.len() {
                    errln!("{PROGRAM_NAME}: option -config needs filename argument");
                    process::exit(i32::from(EXIT_USAGE));
                }
                config_filename = Some(args[index + 1].clone());
                index += 2;
            }
            "-force" => {
                force_mode = true;
                index += 1;
            }
            "-dry-run" => {
                dry_run = true;
                index += 1;
            }
            "-type" => {
                if index + 1 >= args.len() {
                    errln!("{PROGRAM_NAME}: option -type needs a release type argument");
                    process::exit(i32::from(EXIT_USAGE));
                }
                let release_type = match parse_release_type(&args[index + 1]) {
                    Ok(release_type) => release_type,
                    Err(err) => {
                        errln!("{PROGRAM_NAME}: {err}\n");
                        print_help_and_exit();
                    }
                };
                forced_release_type = Some(release_type);
                index += 2;
            }
            _ => {
                errln!("{PROGRAM_NAME}: unknown option {}", args[index]);
                print_help_and_exit();
            }
        }
    }

    let bump_opts = BumpOptions {
        force: force_mode,
        dry_run,
        release_type: forced_release_type,
        stdin: None,
        stdout: None,
    };

    let commands = &args[index..];

    // If no command specified, default to command `bump`
    let command = match commands.first() {
        Some(command) => command,
        None => {
            let cfg = unwrap_fatal(load_config(&repo, config_filename.as_deref()));
            fatal_on_err(bump::bump_version(&repo, &cfg, bump_opts));
            return;
        }
    };

    // Dispatch commands
    match command.as_str() {
        "bump" => {
            let cfg = unwrap_fatal(load_config(&repo, config_filename.as_deref()));
            fatal_on_err(bump::bump_version(&repo, &cfg, bump_opts));
        }

        "preview-changelog" => {
            let cfg = unwrap_fatal(load_config(&repo, config_filename.as_deref()));

            let plan = unwrap_fatal(bump::plan_release(&repo, &cfg, &bump_opts));
            if plan.release_type == ReleaseType::None {
                return;
            }

            let new_changelog = generate_new_changelog_head(
                &plan.current,
                &plan.next,
                &plan.stats,
                &cfg,
                &plan.url_base,
            );
            outln!("{new_changelog}");
        }

        "add-hook" => add_hook(&repo, force_mode),

        "remove-hook" => remove_hook(&repo),

        "lint-commit" => {
            let cfg = unwrap_fatal(load_config(&repo, config_filename.as_deref()));

            if commands.len() < 2 {
                errln!("{PROGRAM_NAME}: lint-commit needs a commit message as its argument");
                process::exit(i32::from(EXIT_USAGE));
            }

            if let Err(err) =
                parse_conventional_commit(&commands[1], false, &cfg.allowed_commit_kinds)
            {
                fatal(&format!("invalid commit message: {err}"));
            }
        }

        "lint" | "lint-all" => {
            let cfg = unwrap_fatal(load_config(&repo, config_filename.as_deref()));

            let git_commits = if command == "lint-all" {
                unwrap_fatal(get_git_commit_lines(&repo, ""))
            } else {
                let current_version = unwrap_fatal(get_current_version_by_git_tag(
                    &repo,
                    &cfg.version_tag_format,
                ));
                let cur_tag = version_to_git_tag(&current_version, &cfg.version_tag_format);
                unwrap_fatal(get_git_commit_lines(&repo, &cur_tag))
            };

            let mut has_invalid_commits = false;

            // Iterate over the chosen commits and validate them
            for message in &git_commits {
                if let Err(err) =
                    parse_conventional_commit(message, true, &cfg.allowed_commit_kinds)
                {
                    has_invalid_commits = true;
                    warn(&format!("invalid commit message: {message}. Reason: {err}"));
                }
            }

            if has_invalid_commits {
                process::exit(i32::from(EXIT_FAILURE));
            }
        }

        "my-version" => {
            let cfg = unwrap_fatal(load_config(&repo, config_filename.as_deref()));
            let current_version = unwrap_fatal(get_current_version_by_git_tag(
                &repo,
                &cfg.version_tag_format,
            ));
            outln!("{current_version}");
        }

        "next-version" => {
            let cfg = unwrap_fatal(load_config(&repo, config_filename.as_deref()));

            let plan = unwrap_fatal(bump::plan_release(&repo, &cfg, &bump_opts));
            if plan.release_type == ReleaseType::None {
                return;
            }

            outln!("{}", plan.next);
        }

        "init-config" => {
            let default_config = get_default_config();

            // If the config file already exists, ask the user to confirm
            // overwriting it
            let config_path = repo.join(DEFAULT_CONFIG_FILENAME);
            if !force_mode && file_exists(&config_path) {
                let question = format!(
                    "Config file {DEFAULT_CONFIG_FILENAME} already exists. Do you want to overwrite it?"
                );
                if !ask(&question, false) {
                    outln!("Canceled.");
                    return;
                }
            }

            // Write the default config file
            unwrap_fatal(save_config(&config_path, &default_config, 0o644));
        }

        "cancel" => {
            let cfg = unwrap_fatal(load_config(&repo, config_filename.as_deref()));
            let version_tag = unwrap_fatal(get_current_version_tag(&repo));

            // Files changed by the release that have to be restored
            let mut restore_files = vec![cfg.change_log_filename.clone()];
            restore_files.extend(cfg.version_filenames.iter().cloned());
            if !cfg.package_json_filename.is_empty() {
                restore_files.push(cfg.package_json_filename.clone());
            }

            let mut commands_to_run: Vec<Vec<String>> = vec![
                // Undo the last commit
                vec![
                    GIT_COMMAND.to_owned(),
                    "reset".to_owned(),
                    "--mixed".to_owned(),
                    "HEAD~1".to_owned(),
                ],
                // Remove the newest version tag
                vec![
                    GIT_COMMAND.to_owned(),
                    "tag".to_owned(),
                    "-d".to_owned(),
                    version_tag,
                ],
            ];

            // Remove the recent changes made to the Changelog and the version files
            for filename in &restore_files {
                commands_to_run.push(vec![
                    GIT_COMMAND.to_owned(),
                    "restore".to_owned(),
                    filename.clone(),
                ]);
            }

            if !force_mode {
                outln!(
                    "{}",
                    paint(
                        color_enabled(&io::stdout()),
                        ANSI_YELLOW,
                        "WARNING: the following commands will be run:"
                    )
                );
                outln!();

                for command_args in &commands_to_run {
                    outln!("\t{}", command_args.join(" "));
                }

                outln!();

                if !ask("Are you sure you want to run them?", false) {
                    outln!("Canceled.");
                    return;
                }
            }

            for command_args in &commands_to_run {
                let args_vec: Vec<&str> = command_args.iter().map(String::as_str).collect();
                let out = command_output(Some(&repo), args_vec[0], &args_vec[1..])
                    .unwrap_or_else(|err| fatal(&err.to_string()));
                if !out.success {
                    fatal(&out.stderr);
                }
            }
        }

        "version" => {
            outln!("{PROGRAM_VERSION}");
        }

        "help" => print_help_and_exit(),

        _ => {
            errln!("{PROGRAM_NAME}: unknown command {command}");
            print_help_and_exit();
        }
    }

    // NOTE: Commands may return early, so nothing must follow the dispatch.
}

/// print_help_and_exit prints the usage text and exits with EXIT_USAGE.
fn print_help_and_exit() -> ! {
    let help = format!(
        r#"Usage: {PROGRAM_NAME} [command] [command options...]

Commands:

  preview-changelog    show this release changelog (not writing to the
                       Changelog file)
  add-hook             add Git hook to validate commit messages
  remove-hook          remove Git hook validating commit messages
  lint                 list commits violating Conventional Commits since last bump
  lint-all             list commits violating Conventional Commits since Git Init
  lint-commit message  lint the provided commit message
  my-version           show the current version of your program and exit
  next-version         show the next version of your program and exit
  init-config          add a config file bump-version.cfg to the project root
  cancel               cancel the recent version bump (if bump done too early)
  version              show this program version and exit
  help                 show this help and exit

Common options:

  -config filename  use another config filename instead of default
                    (default config name: bump-version.cfg)
  -force            suppress the prompts like "Are you sure you want to
                    overwrite this file?" and the release confirmation
  -dry-run          (bump) print the release plan and the changelog
                    without changing anything
  -type type        (bump) force the release type: major, minor, patch or
                    prerelease; by default the type is derived from commits
If no command is specified, the default command to run is bump.
"#
    );

    let _ = io::stderr().write_all(help.as_bytes());
    process::exit(i32::from(EXIT_USAGE));
}

/// ask prompts the user on the standard terminals; on EOF the answer is
/// `default_val`.
fn ask(question: &str, default_val: bool) -> bool {
    let mut reader = io::stdin();
    let mut writer = io::stdout();
    ask_yes_no(&mut reader, &mut writer, question, default_val)
}

/// add-hook writes the commit-msg hook that validates commit messages.
fn add_hook(repo: &Path, force_mode: bool) {
    // Grant the permission to execute
    const EXEC_PERM: u32 = 0o755;

    let hook_path = repo.join(COMMIT_MSG_HOOK_PATH);

    // Create hooks directory if missing
    if let Err(err) = fs::create_dir_all(hook_path.parent().expect("hook path always has a parent"))
    {
        fatal(&format!(
            "could not create the directory for the hook: {err}"
        ));
    }

    let hook_script = format!("#!/bin/sh\n{PROGRAM_NAME} lint-commit \"$(cat \"$1\")\"\n");

    // If the hook file already exists, ask the user to confirm overwriting it
    if !force_mode && file_exists(&hook_path) {
        let question = "The hook file already exists. Do you want to overwrite it?";
        if !ask(question, false) {
            outln!("Canceled.");
            return;
        }
    }

    // Write the hook file
    if let Err(err) = write_file_with_mode(&hook_path, hook_script.as_bytes(), EXEC_PERM) {
        fatal(&format!("could not write the hook file: {err}"));
    }

    // Ensure executable permission
    if let Err(err) = make_executable(&hook_path, EXEC_PERM) {
        fatal(&format!("could not ensure the hook is executable: {err}"));
    }

    outln!(
        "{}",
        paint(color_enabled(&io::stdout()), ANSI_GREEN, "Git Hook added")
    );
}

/// remove-hook deletes the commit-msg hook file; an absent hook is fine.
fn remove_hook(repo: &Path) {
    let hook_path = repo.join(COMMIT_MSG_HOOK_PATH);
    if let Err(err) = fs::remove_file(&hook_path) {
        // Removing an absent hook is not an error.
        if err.kind() != ErrorKind::NotFound {
            fatal(&format!("could not remove the hook file: {err}"));
        }
    }

    outln!(
        "{}",
        paint(color_enabled(&io::stdout()), ANSI_GREEN, "Git Hook removed")
    );
}

/// Marks `path` executable with the given POSIX mode on Unix platforms.
#[cfg(unix)]
fn make_executable(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

/// get_repo_abs_path returns the absolute path of the Git repository root that
/// contains the current working directory.
fn get_repo_abs_path() -> Result<PathBuf> {
    let out =
        command_output(None, GIT_COMMAND, &["rev-parse", "--show-toplevel"]).map_err(|err| {
            Error::msg(format!(
                "could not get the absolute repo path from Git: {err}"
            ))
        })?;
    if !out.success {
        return Err(Error::msg(format!(
            "could not get the absolute repo path from Git: {}",
            out.stderr
        )));
    }

    Ok(PathBuf::from(
        out.stdout.strip_suffix('\n').unwrap_or(&out.stdout),
    ))
}
