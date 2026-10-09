// Command bump-version automates semantic versioning, changelog
// generation, Conventional Commits validation and Git releases.
//
// The codebase is split by responsibility:
//
//   - bump-version.go: CLI entry point, option and command dispatch, help.
//   - bump.go: release planning and the guards around applying a release.
//   - version.go: strict SemVer parsing, incrementing, comparison and tag
//     formats.
//   - commit.go: Conventional Commits parsing and release statistics from
//     the Git history.
//   - changelog.go: changelog rendering, remote compare links and
//     credential stripping.
//   - packagejson.go: optional package.json "version" field bump.
//   - config.go: bump-version.cfg parsing, defaults and validation.
//   - color.go: POSIX terminal color detection and painting.
//   - utils.go: Git helpers, prompting and process exit helpers.
package main

import (
	_ "embed"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

const programName = "bump-version"

const gitCommand = "git"

const commitMsgHookPath = ".git/hooks/commit-msg"

// Program exit status codes.
const (
	ExitSuccess = 0 // may be warnings but no error occurred
	ExitFailure = 1 // an error occurred
	ExitUsage   = 2 // invalid usage of the program
)

//go:embed VERSION
var programVersionStr string

// main parses options and commands, changes into the repository root and
// runs the selected command.
func main() {
	// Make sure the current working directory is the Git repo root
	repoAbsPath := fatalOnErr2(getRepoAbsPath())
	fatalOnErr(os.Chdir(repoAbsPath))

	configFilename := "" // not determined yet

	forceMode := false
	dryRun := false
	var forcedReleaseType ReleaseType

	args := os.Args[1:] // get rid of executable name from args

	// Process all options
	for len(args) > 0 && strings.HasPrefix(args[0], "-") {
		switch args[0] {
		case "-config":
			if len(args) < 2 {
				fmt.Fprintf(os.Stderr, "%s: option -config needs filename argument\n", programName)
				os.Exit(ExitUsage)
			}
			configFilename = args[1]
			args = args[2:]
		case "-force":
			forceMode = true
			args = args[1:]
		case "-dry-run":
			dryRun = true
			args = args[1:]
		case "-type":
			if len(args) < 2 {
				fmt.Fprintf(os.Stderr, "%s: option -type needs a release type argument\n", programName)
				os.Exit(ExitUsage)
			}
			releaseType, err := parseReleaseType(args[1])
			if err != nil {
				fmt.Fprintf(os.Stderr, "%s: %v\n\n", programName, err)
				printHelpAndExit()
			}
			forcedReleaseType = releaseType
			args = args[2:]
		default:
			fmt.Fprintf(os.Stderr, "%s: unknown option %s\n\n", programName, args[0])
			printHelpAndExit()
		}
	}

	bumpOpts := bumpOptions{
		Force:       forceMode,
		DryRun:      dryRun,
		ReleaseType: forcedReleaseType,
		Stdin:       os.Stdin,
		Stdout:      os.Stdout,
	}

	// If no command specified, default to command `bump`
	if len(args) < 1 {
		cfg := fatalOnErr2(loadConfig(configFilename))
		fatalOnErr(bumpVersion(&cfg, bumpOpts))
		return
	}

	command := args[0]

	// Dispatch commands
	switch command {
	case "bump":
		cfg := fatalOnErr2(loadConfig(configFilename))
		fatalOnErr(bumpVersion(&cfg, bumpOpts))

	case "preview-changelog":
		cfg := fatalOnErr2(loadConfig(configFilename))

		plan := fatalOnErr2(planRelease(&cfg, bumpOpts))
		if plan.releaseType == ReleaseTypeNone {
			return
		}

		newChangeLog := generateNewChangelogHead(plan.current, plan.next, plan.stats, &cfg, plan.urlBase)
		fmt.Println(newChangeLog.String())

	case "add-hook":
		// Grant the permission to execute
		execPerm := os.FileMode(0o755)

		// Create hooks directory if missing
		if err := os.MkdirAll(filepath.Dir(commitMsgHookPath), execPerm); err != nil {
			fatalf("could not create the directory for the hook: %v", err)
		}

		hookScript := fmt.Sprintf(`#!/bin/sh
%s lint-commit "$(cat "$1")"
`, programName)

		// If the hook file already exists, ask the user to confirm overwriting it
		if !forceMode && fileExists(commitMsgHookPath) {
			question := "The hook file already exists. Do you want to overwrite it?"
			if !askYesNo(os.Stdin, os.Stdout, question, false) {
				fmt.Println("Canceled.")
				return
			}
		}

		// Write the hook file
		if err := os.WriteFile(commitMsgHookPath, []byte(hookScript), execPerm); err != nil {
			fatalf("could not write the hook file: %v", err)
		}

		// Ensure executable permission
		if err := os.Chmod(commitMsgHookPath, execPerm); err != nil {
			fatalf("could not ensure the hook is executable: %v", err)
		}

		fmt.Println(paint(colorEnabled(os.Stdout), ansiGreen, "Git Hook added"))

	case "remove-hook":
		if err := os.RemoveAll(commitMsgHookPath); err != nil {
			fatalf("could not remove the hook file: %v", err)
		}

		fmt.Println(paint(colorEnabled(os.Stdout), ansiGreen, "Git Hook removed"))

	case "lint-commit":
		cfg := fatalOnErr2(loadConfig(configFilename))

		if len(args) < 2 {
			fmt.Fprintf(os.Stderr, "%s: lint-commit needs a commit message as its argument\n", programName)
			os.Exit(ExitUsage)
		}

		if _, err := ParseConventionalCommit(args[1], false, cfg.AllowedCommitKinds); err != nil {
			fatalf("invalid commit message: %v", err)
			os.Exit(ExitFailure)
		}

	case "lint", "lint-all":
		cfg := fatalOnErr2(loadConfig(configFilename))

		var gitCommits []string

		if command == "lint-all" {
			gitCommits = fatalOnErr2(getGitCommitLines(""))
		} else {
			currentVersion := fatalOnErr2(getCurrentVersionByGitTag(cfg.VersionTagFormat))
			curTag := versionToGitTag(currentVersion, cfg.VersionTagFormat)
			gitCommits = fatalOnErr2(getGitCommitLines(curTag))
		}

		hasInvalidCommits := false

		// Iterate over the chosen commits and validate them
		for _, message := range gitCommits {
			_, err := ParseConventionalCommit(message, true, cfg.AllowedCommitKinds)
			if err != nil {
				hasInvalidCommits = true
				warnf("invalid commit message: %s. Reason: %v", message, err)
			}
		}

		if hasInvalidCommits {
			os.Exit(ExitFailure)
		}

	case "my-version":
		cfg := fatalOnErr2(loadConfig(configFilename))
		currentVersion := fatalOnErr2(getCurrentVersionByGitTag(cfg.VersionTagFormat))
		fmt.Println(currentVersion.ToString())

	case "next-version":
		cfg := fatalOnErr2(loadConfig(configFilename))

		plan := fatalOnErr2(planRelease(&cfg, bumpOpts))
		if plan.releaseType == ReleaseTypeNone {
			return
		}

		fmt.Println(plan.next.ToString())

	case "init-config":
		defaultConfig := getDefaultConfig()

		// If the config file already exists, ask the user to confirm overwriting it
		if !forceMode && fileExists(defaultConfigFilename) {
			question := fmt.Sprintf(
				"Config file %s already exists. Do you want to overwrite it?",
				defaultConfigFilename,
			)
			if !askYesNo(os.Stdin, os.Stdout, question, false) {
				fmt.Println("Canceled.")
				return
			}
		}

		// Write the default config file
		fatalOnErr(saveConfig(defaultConfigFilename, defaultConfig, 0o644))

	case "cancel":
		cfg := fatalOnErr2(loadConfig(configFilename))
		versionTag := fatalOnErr2(getCurrentVersionTag())

		// Files changed by the release that have to be restored
		restoreFiles := append([]string{cfg.ChangeLogFilename}, cfg.VersionFilenames...)
		if cfg.PackageJSONFilename != "" {
			restoreFiles = append(restoreFiles, cfg.PackageJSONFilename)
		}

		commands := [][]string{
			// Undo the last commit
			{gitCommand, "reset", "--mixed", "HEAD~1"},

			// Remove the newest version tag
			{gitCommand, "tag", "-d", versionTag},
		}

		// Remove the recent changes made to the Changelog and the version files
		for _, filename := range restoreFiles {
			commands = append(commands, []string{gitCommand, "restore", filename})
		}

		if !forceMode {
			fmt.Println(paint(colorEnabled(os.Stdout), ansiYellow, "WARNING: the following commands will be run:"))
			fmt.Println()

			for _, cmdArgs := range commands {
				fmt.Printf("\t%s\n", strings.Join(cmdArgs, " "))
			}

			fmt.Println()

			question := "Are you sure you want to run them?"
			if !askYesNo(os.Stdin, os.Stdout, question, false) {
				fmt.Println("Canceled.")
				return
			}
		}

		for _, cmdArgs := range commands {
			if _, errOut, err := getCommandOutput(cmdArgs[0], cmdArgs[1:]...); err != nil {
				fatalf("%s", errOut)
			}
		}

	case "version":
		fmt.Println(programVersionStr)

	case "help":
		printHelpAndExit()

	default:
		fmt.Fprintf(os.Stderr, "%s: unknown command %s\n\n", programName, os.Args[1])
		printHelpAndExit()
	}

	// NOTE: Commands may return so there should not be any code
}

// printHelpAndExit prints the usage text and exits with ExitUsage.
func printHelpAndExit() {
	fmt.Fprintf(os.Stderr, `Usage: %s [command] [command options...]

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
`, programName)

	os.Exit(ExitUsage)
}

// getRepoAbsPath returns the absolute path of the Git repository root that
// contains the current working directory.
func getRepoAbsPath() (string, error) {
	path, errOut, err := getCommandOutput(gitCommand, "rev-parse", "--show-toplevel")
	if err != nil {
		return "", fmt.Errorf("could not get the absolute repo path from Git: %v", errOut)
	}

	path = strings.TrimSuffix(path, "\n")

	return path, nil
}
