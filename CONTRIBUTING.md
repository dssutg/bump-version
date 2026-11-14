# Contributing

This document explains project conventions and how to contribute.

## Read README first

This document contains essential information about the tool, including how to commit and what commit messages should look like. The tool uses itself for releases.

## Maintainers

Contact the maintainers with questions not answered here:

- Daniil Stepanov <dstepanov485@gmail.com>

## Stack

- Rust via [rustup](https://rustup.rs) (stable toolchain, edition 2024)

Optional tools:

- rustfmt (formatter, ships with rustup)
- clippy (linter, ships with rustup)
- extra cross targets for release builds (`rustup target add <triple>`)

## Languages

This project supports both English and Russian for documentation.

For code, use only Rust. This keeps the project consistent.

## Distribution

Keep distribution simple with no installation required. Only use Rust and ship self-contained binaries.

## Before submitting

Run the combined check (formatting, linting, tests):

```bash
cd scripts && cargo run -- check
```

Or run the steps individually: `cargo fmt --all -- --check` (clean), `cargo clippy --workspace --all-targets -- -D warnings` (0 warnings), `cargo test --workspace`.

Git must be available; the tests exercise release flows in throwaway Git repositories.

## Questions and Answers

### Why are there no shell scripts or Makefiles?

Everything is written in Rust, including scripts. Introducing another language adds platform dependency and complexity. Cargo is portable, reliable, and readable.

The `scripts/` directory contains all project scripts (as a plain Rust binary). Build with:

```bash
cd scripts && cargo run -- build
```

Cargo's incremental compilation makes repeated builds fast. No Makefile is needed.

Release artifacts for the four published platforms are built when their target toolchains are installed (`rustup target add x86_64-apple-darwin`, etc.); `build` skips missing targets with a hint.

### Can I use library X?

No, with one exception: the single JSON dependency (`serde_json`) that the `package.json` bump is built on. Everything else uses the Rust standard library.
