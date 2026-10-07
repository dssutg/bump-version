package main

import (
	"errors"
	"fmt"
	"io"
	"os"
	"strings"
)

// bumpOptions carries the command line switches that influence the release
// workflow.
type bumpOptions struct {
	Force       bool        // skip the confirmation prompt (-force)
	DryRun      bool        // write nothing, only print the plan (-dry-run)
	ReleaseType ReleaseType // empty: derived from commits; otherwise the -type override
	Stdin       io.Reader   // input for the confirmation prompt (nil: os.Stdin)
	Stdout      io.Writer   // destination for release output (nil: os.Stdout)
}

// releasePlan is everything needed to apply a release. It is computed by
// planRelease before anything is written.
type releasePlan struct {
	current, next Version
	releaseType   ReleaseType
	tagName       string
	stats         CommitStats
	urlBase       string
}

// planRelease computes the next release from the Git history and never
// writes anything. A plan with releaseType == ReleaseTypeNone means there is
// nothing to release; the reason has already been warned on stderr.
func planRelease(cfg *Config, opts bumpOptions) (releasePlan, error) {
	currentVersion, err := getCurrentVersionByGitTag(cfg.VersionTagFormat)
	if err != nil {
		return releasePlan{}, err
	}

	curTag := versionToGitTag(currentVersion, cfg.VersionTagFormat)

	stats, err := processCommitMessages(
		curTag,
		cfg.IgnoreInvalidCommits,
		cfg.AllowedCommitKinds,
	)
	if err != nil {
		return releasePlan{}, err
	}

	releaseType := opts.ReleaseType
	if releaseType == "" {
		releaseType = stats.DetermineReleaseType()
	}

	// An empty release window refuses to release even with a forced type.
	if stats.InWindowCommits == 0 {
		fmt.Fprintln(os.Stderr, programName+": nothing to release")
		return releasePlan{
			current:     currentVersion,
			stats:       stats,
			releaseType: ReleaseTypeNone,
		}, nil
	}
	if releaseType == ReleaseTypeNone {
		warnNoCommitsThatCanIncrementCurrentVersion()
		return releasePlan{
			current:     currentVersion,
			stats:       stats,
			releaseType: ReleaseTypeNone,
		}, nil
	}

	next := currentVersion
	next.IncrementTo(releaseType)

	return releasePlan{
		current:     currentVersion,
		next:        next,
		releaseType: releaseType,
		tagName:     versionToGitTag(next, cfg.VersionTagFormat),
		stats:       stats,
		urlBase:     resolveURLBase("origin"),
	}, nil
}

// printPlan writes the human-readable release plan to w.
func (p releasePlan) printPlan(w io.Writer) {
	fmt.Fprintf(w, "Release:     %s\n", p.releaseType)
	fmt.Fprintf(w, "Version:     %s -> %s\n", p.current.ToString(), p.next.ToString())
	fmt.Fprintf(w, "Tag:         %s\n", p.tagName)
}

// bumpVersion plans a release, shows it to the user and applies it. A dry run
// stops after printing the plan; without -force the release is confirmed
// interactively.
func bumpVersion(cfg *Config, opts bumpOptions) error {
	plan, err := planRelease(cfg, opts)
	if err != nil {
		return err
	}
	if plan.releaseType == ReleaseTypeNone {
		return nil
	}

	stdout := opts.Stdout
	if stdout == nil {
		stdout = os.Stdout
	}
	stdin := opts.Stdin
	if stdin == nil {
		stdin = os.Stdin
	}

	plan.printPlan(stdout)

	// Show what will be committed, then leave everything untouched on a dry run.
	fmt.Fprintln(stdout)
	head := generateNewChangelogHead(plan.current, plan.next, plan.stats, cfg, plan.urlBase)
	fmt.Fprint(stdout, head.String())

	if opts.DryRun {
		fmt.Fprintln(stdout, "Dry run: nothing was modified.")
		return nil
	}

	// Guards run before the prompt so that a doomed release fails fast.
	tagAlreadyExists, err := tagExists(plan.tagName)
	if err != nil {
		return err
	}
	if tagAlreadyExists {
		return fmt.Errorf("release tag %s already exists", plan.tagName)
	}

	if err := requireCleanWorktree(); err != nil {
		return err
	}

	if !opts.Force {
		question := "Release " + plan.next.ToString() + "?"
		if !askYesNo(stdin, stdout, question, false) {
			fmt.Fprintln(stdout, "Aborted.")
			return nil
		}
	}

	return applyReleasePlan(cfg, plan, stdout)
}

// applyReleasePlan writes the changelog and the version files, commits them
// and tags the release. There is deliberately no rollback: if something fails
// midway, the `cancel` command undoes the release.
func applyReleasePlan(cfg *Config, plan releasePlan, stdout io.Writer) error {
	newVersionStr := plan.next.ToString()

	fmt.Fprintf(stdout, "Bumping next release version %s...\n", newVersionStr)

	if err := generateChangelog(plan.current, plan.next, plan.stats, cfg, plan.urlBase); err != nil {
		return err
	}

	for _, name := range cfg.VersionFilenames {
		//nolint:gosec // the file name comes from this project's own config
		if err := os.WriteFile(name, []byte(newVersionStr), 0o600); err != nil {
			return fmt.Errorf("could not write version file %s: %w", name, err)
		}
	}

	gitAddArgs := make([]string, 0, 2+len(cfg.VersionFilenames))
	gitAddArgs = append(gitAddArgs, "add", "--", cfg.ChangeLogFilename)
	gitAddArgs = append(gitAddArgs, cfg.VersionFilenames...)
	if _, errOut, err := getCommandOutput(gitCommand, gitAddArgs...); err != nil {
		return fmt.Errorf("error staging files to Git: %s", errOut)
	}

	commitMessage := strings.ReplaceAll(
		cfg.BumpVersionCommit,
		versionTagFormatSubst,
		newVersionStr,
	)
	if _, errOut, err := getCommandOutput(gitCommand, "commit", "-m", commitMessage); err != nil {
		return fmt.Errorf("error committing changes to Git: %s", errOut)
	}

	tagMessage := "Release " + newVersionStr
	if _, errOut, err := getCommandOutput(gitCommand, "tag", "-a", plan.tagName, "-m", tagMessage); err != nil {
		return fmt.Errorf("error adding Git tag for this release: %s", errOut)
	}

	if cfg.ShouldPushToOrigin {
		if _, errOut, err := getCommandOutput(gitCommand, "push", "--follow-tags", "origin"); err != nil {
			return fmt.Errorf("error pushing to remote Git repository: %s", errOut)
		}
		return nil
	}

	branch := "HEAD"
	if currentBranch, branchErr := currentGitBranch(); branchErr == nil {
		branch = currentBranch
	}
	fmt.Fprintln(stdout)
	fmt.Fprintln(stdout, "Next steps:")
	fmt.Fprintf(stdout, "  git push origin %s --follow-tags\n", branch)

	return nil
}

// tagExists reports whether the Git tag already exists. Only an unknown
// revision counts as "does not exist"; real Git failures are returned.
func tagExists(tagName string) (bool, error) {
	_, errOut, err := getCommandOutput(
		gitCommand,
		"rev-parse",
		"--verify",
		"--quiet",
		"refs/tags/"+tagName,
	)
	if err != nil {
		if strings.TrimSpace(errOut) == "" {
			return false, nil
		}
		return false, fmt.Errorf("error checking Git tag %s: %s", tagName, strings.TrimSpace(errOut))
	}
	return true, nil
}

// requireCleanWorktree refuses to release over uncommitted work: staged
// changes would be swept into the release commit and a dirty tree would leave
// the release half-applied.
func requireCleanWorktree() error {
	out, errOut, err := getCommandOutput(gitCommand, "status", "--porcelain")
	if err != nil {
		return fmt.Errorf("error checking the worktree status: %s", errOut)
	}

	for _, line := range strings.Split(out, "\n") {
		if len(line) < 2 {
			continue
		}
		// In porcelain format the first column is the index status; "??" and
		// "!!" mark untracked and ignored files, which are not staged.
		if indexStatus := line[0]; indexStatus != ' ' && indexStatus != '?' && indexStatus != '!' {
			return errors.New("refusing to release: changes are already staged - commit or reset them first")
		}
	}

	if strings.TrimSpace(out) != "" {
		return errors.New("working tree is not clean - commit your changes before releasing")
	}

	return nil
}

// currentGitBranch returns the name of the currently checked-out branch.
func currentGitBranch() (string, error) {
	out, errOut, err := getCommandOutput(gitCommand, "rev-parse", "--abbrev-ref", "HEAD")
	if err != nil {
		return "", fmt.Errorf("error getting the current branch from Git: %s", errOut)
	}
	return strings.TrimSpace(out), nil
}
