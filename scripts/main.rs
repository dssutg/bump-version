//! Project scripts: build release binaries, clean the build directory and
//! run the combined format/lint/test check.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, exit};

/// Release targets in build order: build/<outdir> output directory, the Rust
/// target triple and the produced binary name. A triple whose toolchain is
/// not installed is skipped with a warning; add it with `rustup target add
/// <triple>`.
const TARGETS: &[(&str, &str, &str)] = &[
    ("linux-amd64", "x86_64-unknown-linux-gnu", "bump-version"),
    ("macos-amd64", "x86_64-apple-darwin", "bump-version"),
    ("macos-arm64", "aarch64-apple-darwin", "bump-version"),
    (
        "windows-amd64",
        "x86_64-pc-windows-msvc",
        "bump-version.exe",
    ),
];

fn main() {
    // The repository root, resolved from this script's own location so that
    // cargo commands always run against the main crate.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("scripts directory must have a parent")
        .to_path_buf();

    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("command not specified");
        exit(1);
    }

    match args[0].as_str() {
        "build" => {
            let installed = installed_targets();
            for (out_dir, triple, binary) in TARGETS {
                if installed.contains(&(*triple).to_owned()) {
                    build_target(&root, out_dir, triple, binary);
                } else {
                    eprintln!(
                        "Skipping {out_dir}: target {triple} is not installed; \
                         add it with `rustup target add {triple}`"
                    );
                }
            }
        }
        "clean" => {
            must(fs::remove_dir_all(root.join("build")));
        }
        "check" => {
            must_run(&root, &["cargo", "fmt", "--all"]);
            must_run(&root, &["cargo", "clippy", "--workspace", "--all-targets"]);
            must_run(&root, &["cargo", "test", "--workspace"]);
        }
        _ => {
            eprintln!("invalid command");
            exit(1);
        }
    }
}

/// The target toolchains available in this rustup installation.
fn installed_targets() -> Vec<String> {
    let output = match Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
    {
        Ok(output) => output,
        // Without rustup no extra targets are available.
        Err(_) => return Vec::new(),
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Builds the main crate for one target and installs the binary in the
/// build/ tree.
fn build_target(root: &Path, out_dir: &str, triple: &str, binary: &str) {
    let destination = root.join("build").join(out_dir).join(binary);
    println!("Building {}", destination.display());

    must(fs::create_dir_all(
        destination.parent().expect("target has a parent"),
    ));

    // --message-format=json locates the produced binary independently of
    // where the target directory points.
    let build = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--package",
            "bump-version",
            "--target",
            triple,
            "--message-format=json",
        ])
        .current_dir(root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .output()
        .unwrap_or_else(|err| panic!("could not run cargo: {err}"));
    if !build.status.success() {
        eprintln!("cargo build for {triple} failed");
        exit(1);
    }

    let executable: String = String::from_utf8_lossy(&build.stdout)
        .lines()
        .rev()
        .find_map(parse_artifact_executable)
        .unwrap_or_else(|| panic!("the built binary for {triple} was not reported"));
    must(fs::copy(&executable, &destination));
}

/// Extracts the executable path from a cargo compiler-artifact JSON message.
fn parse_artifact_executable(line: &str) -> Option<String> {
    let message: serde_json::Value = serde_json::from_str(line).ok()?;
    if message.get("reason")?.as_str()? != "compiler-artifact" {
        return None;
    }
    if message.get("target")?.get("name")?.as_str()? != "bump-version" {
        return None;
    }
    message
        .get("executable")?
        .as_str()
        .map(|path| path.to_owned())
}

/// Runs a command at `root` with inherited standard streams.
fn must_run(root: &Path, argv: &[&str]) {
    let status = Command::new(argv[0])
        .args(&argv[1..])
        .current_dir(root)
        .status()
        .unwrap_or_else(|err| panic!("could not run {}: {err}", argv[0]));
    must(status);
}

/// Exits with a fatal report when an operation failed.
fn must(result: impl IntoFallible) {
    if let Err(err) = result.into_fallible() {
        eprintln!("{err}");
        exit(1);
    }
}

/// Values either infallible or convertible to an io::Result, so `must` can
/// accept both `fs` calls and `ExitStatus`.
trait IntoFallible {
    type Value;
    fn into_fallible(self) -> std::io::Result<Self::Value>;
}

impl IntoFallible for std::io::Result<()> {
    type Value = ();
    fn into_fallible(self) -> std::io::Result<()> {
        self
    }
}

impl IntoFallible for std::io::Result<u64> {
    type Value = u64;
    fn into_fallible(self) -> std::io::Result<u64> {
        self
    }
}

impl IntoFallible for std::process::ExitStatus {
    type Value = ();
    fn into_fallible(self) -> std::io::Result<()> {
        if self.success() {
            Ok(())
        } else {
            Err(std::io::Error::other("command failed".to_owned()))
        }
    }
}
