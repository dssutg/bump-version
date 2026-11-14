# AGENTS.md

## Project Overview

*bump-version* — A CLI tool for automating:
- Semantic Versioning
- Changelog generation
- Conventional Commits validation
- Git release commits and annotated tags

## Language & Framework

- **Language:** Rust (edition 2024); JSON handling uses `serde_json`
- **Build:** `cd scripts && cargo run -- build` (outputs to `build/` directory)

## Commands

| Command | Description |
|---------|-------------|
| `bump-version` | Default command — plans, confirms and applies the release (version, CHANGELOG, Git commit, annotated tag) |
| `bump-version preview-changelog` | Show changelog without writing to file |
| `bump-version add-hook` | Add Git commit-msg hook for commit message validation |
| `bump-version remove-hook` | Remove Git commit-msg hook |
| `bump-version lint` | Validate commits since last release |
| `bump-version lint-all` | Validate all commits |
| `bump-version lint-commit "message"` | Validate a single commit message |
| `bump-version my-version` | Show current version |
| `bump-version next-version` | Show next version without making changes |
| `bump-version init-config` | Create config file |
| `bump-version cancel` | Cancel recent version bump |
| `bump-version version` | Show tool version |

**Options:**
- `-config <filename>` — Use custom config file (default: `bump-version.cfg`), before the command
- `-force` — Suppress confirmation prompts (overwrite prompts and the release confirmation; required for non-interactive releases)
- `-dry-run` — (bump) Print the release plan and changelog, then exit without changing anything
- `-type <major|minor|patch|prerelease>` — (bump) Force the release type instead of deriving it from commits

## Codebase Map

- `src/main.rs` — CLI entry point, option and command dispatch, help.
- `src/bump.rs` — release planning and the guards around applying a release.
- `src/version.rs` — strict SemVer parsing, incrementing, comparison and tag formats.
- `src/commit.rs` — Conventional Commits parsing and release statistics from the Git history.
- `src/changelog.rs` — changelog rendering, remote compare links and credential stripping.
- `src/package_json.rs` — optional package.json "version" field bump (serde_json).
- `src/config.rs` — bump-version.cfg parsing, defaults and validation.
- `src/color.rs` — POSIX terminal color detection and painting.
- `src/utils.rs` — Git helpers, prompting and process exit helpers.
- `src/error.rs` — the shared error type.
- `scripts/main.rs` — build/clean/check driver for the whole workspace.

## Version Bumping Logic

Based on Conventional Commits since last tag:
1. **MAJOR** — `BREAKING CHANGE` in commit body/footer, or `!` after type (e.g., `feat!: ...`)
2. **MINOR** — `feat:` commits present
3. **PATCH** — `fix:` or `perf:` commits present
4. **No change** — None of the above (prints `nothing to release` and exits without changes)

`-type` overrides the derived type, but the release window must still contain at least one commit.

## Safety Guards

Before applying a release the tool prints the plan (type, version, tag, changelog) and then:
- asks for confirmation — `Release X.Y.Z? [y/N]`, default *no* (pass `-force` in scripts and CI);
- refuses if changes are staged (commit or reset them first) or the working tree is dirty;
- refuses if the target tag already exists;
- creates an annotated tag (`Release X.Y.Z`) and, with `shouldPushToOrigin` enabled, pushes with `git push --follow-tags` (otherwise it prints the exact next steps);
- with `-dry-run` exits after printing the plan.

## Verification

Run before submitting:
- `cargo fmt --all -- --check` (must report nothing to reformat)
- `cargo clippy --workspace --all-targets -- -D warnings` (must report 0 warnings)
- `cargo test --workspace` (git is required; tests run in throwaway repositories)
- or everything in one go: `cd scripts && cargo run -- check`
