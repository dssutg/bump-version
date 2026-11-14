//! Shared helpers: Git command execution, interactive prompting, fatal and
//! warning diagnostics and small file checks.

use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::color::{ANSI_BOLD, ANSI_RED, ANSI_YELLOW, Stream, color_enabled, paint};
use crate::error::Result;

/// The Git program all repository operations run through.
pub const GIT_COMMAND: &str = "git";

/// The name the program reports itself with in diagnostics.
pub const PROGRAM_NAME: &str = "bump-version";

/// Program exit status codes.
pub const EXIT_SUCCESS: u8 = 0; // there may be warnings, but no error occurred
pub const EXIT_FAILURE: u8 = 1; // an error occurred
pub const EXIT_USAGE: u8 = 2; // invalid usage of the program

// EXIT_SUCCESS is the normal (implicit) end of the program; it is part of the
// documented exit code trio.
#[allow(dead_code)]
const _EXIT_SUCCESS_USED_FOR_DOCUMENTATION: u8 = EXIT_SUCCESS;

/// Result of a spawned command: both output streams and whether the program
/// ran and exited successfully. Call sites build context messages from these
/// fields, including ones that need the captured stderr of a failed run.
#[derive(Debug, Default, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    /// false when the program could not be run or exited with a failure
    /// status
    pub success: bool,
    /// The child process exit code; -1 when the program could not be run.
    pub exit_code: i32,
}

/// command_output runs program with args inside `dir` (or the current working
/// directory when `dir` is none) and captures both output streams. The child
/// inherits the standard input.
pub fn command_output<S: AsRef<str>>(
    dir: Option<&Path>,
    program: &str,
    args: &[S],
) -> io::Result<CommandOutput> {
    let mut command = Command::new(program);
    command.args(args.iter().map(|arg| arg.as_ref()));
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    command.stdin(Stdio::inherit());

    let out = command.output()?;

    Ok(CommandOutput {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        success: out.status.success(),
        exit_code: out.status.code().unwrap_or(-1),
    })
}

/// git runs the Git program inside the repository directory.
pub fn git<S: AsRef<str>>(repo: &Path, args: &[S]) -> io::Result<CommandOutput> {
    command_output(Some(repo), GIT_COMMAND, args)
}

/// ask_yes_no prompts the user with the given question and returns true for
/// "yes" and false for "no". It accepts "y", "yes", "n", "no" (any case). On
/// EOF or another read error it returns `default_val`.
pub fn ask_yes_no(
    reader: &mut dyn Read,
    writer: &mut dyn Stream,
    question: &str,
    default_val: bool,
) -> bool {
    let mut reader = BufReader::new(reader);

    loop {
        let default_prompt = if default_val { "Y/n" } else { "y/N" };
        let _ = write!(
            writer,
            "{} [{default_prompt}]: ",
            paint(color_enabled(writer), ANSI_BOLD, question)
        );
        let _ = writer.flush();

        let mut line = String::new();
        match reader.read_line(&mut line) {
            // EOF or a read error answers with the default...
            Err(_) | Ok(0) => return default_val,
            // ...and so does an unterminated last line, which the reading
            // of a prompt line treats as an error.
            Ok(_) if !line.ends_with('\n') => return default_val,
            Ok(_) => {}
        }

        let response = line.trim().to_lowercase();
        if response.is_empty() {
            return default_val;
        }

        match response.as_str() {
            "y" | "yes" => return true,
            "n" | "no" => return false,
            _ => {
                let _ = writeln!(writer, "Please answer yes or no (y/n).");
                let _ = writer.flush();
            }
        }
    }
}

/// fatal_on_err exits with a fatal error message when the result failed.
pub fn fatal_on_err(result: Result<()>) {
    if let Err(err) = result {
        fatal(&err.to_string());
    }
}

/// unwrap_fatal returns the payload of a result, or exits with a fatal error
/// when the result failed.
pub fn unwrap_fatal<T>(result: Result<T>) -> T {
    match result {
        Ok(value) => value,
        Err(err) => fatal(&err.to_string()),
    }
}

/// fatal prints a red error line to standard error and exits with
/// EXIT_FAILURE.
pub fn fatal(message: &str) -> ! {
    colored_stderr(ANSI_RED, &format!("{PROGRAM_NAME}: {message}"));
    std::process::exit(i32::from(EXIT_FAILURE));
}

/// warn prints a yellow warning line to standard error.
pub fn warn(message: &str) {
    colored_stderr(ANSI_YELLOW, &format!("{PROGRAM_NAME}: {message}"));
}

/// Prints one colored line to standard error when colors are enabled.
fn colored_stderr(code: &str, message: &str) {
    let tinted = color_enabled(&io::stderr());
    let _ = writeln!(io::stderr(), "{}", paint(tinted, code, message));
}

/// file_exists reports whether path exists; permission problems count as
/// existence to stay on the safe side.
pub fn file_exists(path: impl AsRef<Path>) -> bool {
    match fs::metadata(path) {
        Ok(_) => true,
        Err(err) if err.kind() == io::ErrorKind::NotFound => false,
        Err(_) => true,
    }
}

/// has_leading_whitespace reports whether s begins with a Unicode whitespace
/// character.
pub fn has_leading_whitespace(s: &str) -> bool {
    s.chars().next().is_some_and(char::is_whitespace)
}

/// has_trailing_whitespace reports whether s ends with a Unicode whitespace
/// character.
pub fn has_trailing_whitespace(s: &str) -> bool {
    s.chars().next_back().is_some_and(char::is_whitespace)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod testing {
    //! Throwaway Git repositories for the tests that touch Git.

    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A temporary directory deleted when the value drops.
    pub(crate) struct TempDir(PathBuf);

    impl TempDir {
        pub(crate) fn new(name: &str) -> TempDir {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "{}-{}-{}-{}",
                name,
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.subsec_millis())
            ));
            std::fs::create_dir_all(&path).expect("could not create temp dir");
            TempDir(path)
        }

        pub(crate) fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Runs git inside `dir` with a pristine configuration and returns all
    /// three outputs.
    pub(crate) fn try_git(dir: &Path, args: &[&str]) -> (String, String, bool) {
        let mut command = Command::new(crate::utils::GIT_COMMAND);
        command
            .args(args)
            .current_dir(dir)
            // Keep the host's git configuration out of test runs.
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null");
        let out = command.output().expect("git must be runnable");
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
            out.status.success(),
        )
    }

    /// Runs git inside `dir` and returns its stdout, panicking on failure.
    pub(crate) fn run_git(dir: &Path, args: &[&str]) -> String {
        let (out, err_out, ok) = try_git(dir, args);
        if !ok {
            panic!("git {} failed: {}", args.join(" "), err_out);
        }
        out
    }

    /// Creates an empty Git repository with a test identity in `dir`.
    pub(crate) fn init_git_repo(dir: &Path) {
        run_git(dir, &["init"]);
        run_git(dir, &["config", "user.name", "Test User"]);
        run_git(dir, &["config", "user.email", "test@example.com"]);
        run_git(dir, &["config", "commit.gpgsign", "false"]);
    }

    /// Commits an empty commit with the given message in `dir`.
    pub(crate) fn commit_test_message(dir: &Path, message: &str) {
        run_git(dir, &["commit", "--allow-empty", "-m", message]);
    }
}
