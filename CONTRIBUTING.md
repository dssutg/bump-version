# Contributing

This document explains project conventions and how to contribute.

## Read README first

This document contains essential information about the tool, including how to commit and what commit messages should look like. The tool uses itself for releases.

## Maintainers

Contact the maintainers with questions not answered here:

- Daniil Stepanov <dstepanov485@gmail.com>

## Stack

- Go 1.26 or newer

Optional tools:

- golangci-lint (Go linter)
- gofumpt (Go formatter)

## Languages

This project supports both English and Russian for documentation.

For code, use only Go. This keeps the project consistent.

## Distribution

Keep distribution simple with no installation required. Only use Go and link binaries statically.

## Before submitting

Run the combined check (formatting, linting, tests):

```bash
cd scripts && go run . check
```

Or run the steps individually: `gofumpt -l .` (no output), `golangci-lint run` (0 findings), `go test ./...`.

## Questions and Answers

### Why are there no shell scripts or Makefiles?

Everything is written in Go, including scripts. Introducing another language adds platform dependency and complexity. Go scripts are portable, reliable, and readable.

The `scripts/` directory contains all project scripts. Build with:

```bash
cd scripts && go run . build
```

Go's build cache makes incremental builds fast. No Makefile is needed.

### Can I use library X?

No. This project avoids third-party dependencies to prevent dependency hell. Use only the Go standard library.
