package main

import (
	"fmt"
	"os"
	"strings"
)

func bumpVersion(cfg *Config) error {
	currentVersion, err := getCurrentVersionByGitTag(cfg.VersionTagFormat)
	if err != nil {
		return err
	}

	curTag := versionToGitTag(currentVersion, cfg.VersionTagFormat)

	commitStats, err := processCommitMessages(
		curTag,
		cfg.IgnoreInvalidCommits,
		cfg.AllowedCommitKinds,
	)
	if err != nil {
		return err
	}

	releaseType := commitStats.DetermineReleaseType()
	if releaseType == ReleaseTypeNone {
		warnNoCommitsThatCanIncrementCurrentVersion()
		return nil
	}

	urlBase := resolveURLBase("origin")

	// Determine new version based on the processed commits
	newVersion := currentVersion
	newVersion.IncrementTo(releaseType)
	newVersionStr := newVersion.ToString()

	fmt.Printf("Bumping next release version %s...\n", newVersionStr)

	// Generate Changelog
	if err := generateChangelog(currentVersion, newVersion, commitStats, cfg, urlBase); err != nil {
		return err
	}

	// Update version files
	for _, name := range cfg.VersionFilenames {
		if err := os.WriteFile(name, []byte(newVersionStr), 0o600); err != nil {
			return fmt.Errorf("could not write version file %s: %w", name, err)
		}
	}

	// Stage Changelog and version files
	gitAddArgs := []string{"add", cfg.ChangeLogFilename}
	gitAddArgs = append(gitAddArgs, cfg.VersionFilenames...)
	if _, errOut, err := getCommandOutput(gitCommand, gitAddArgs...); err != nil {
		return fmt.Errorf("error staging files to Git: %s", errOut)
	}

	// Commit release
	bumpVersionCommitMessage := strings.ReplaceAll(
		cfg.BumpVersionCommit,
		versionTagFormatSubst,
		newVersionStr,
	)
	if _, errOut, err := getCommandOutput(gitCommand, "commit", "-m", bumpVersionCommitMessage); err != nil {
		return fmt.Errorf("error committing changes to Git: %s", errOut)
	}

	// Tag release
	if _, errOut, err := getCommandOutput(gitCommand, "tag", "v"+newVersionStr); err != nil {
		return fmt.Errorf("error adding Git tag for this release: %s", errOut)
	}

	if cfg.ShouldPushToOrigin {
		if _, errOut, err := getCommandOutput(gitCommand, "push", "--tags", "origin"); err != nil {
			return fmt.Errorf("error pushing to remote Git repository: %s", errOut)
		}
	}

	return nil
}
