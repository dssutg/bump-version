package main

import (
	"bytes"
	"io"
	"os"
	"strings"
	"testing"
)

const (
	tagV100    = "v1.0.0"
	tagV110    = "v1.1.0"
	tagRelV100 = "rel-1.0.0"
	tagRelV110 = "rel-1.1.0"

	initialCommitMsg = "chore: initial import"
	featWidgetMsg    = "feat: add widget"
	releaseCommitMsg = "chore(release): 1.1.0"
)

// prepareReleaseRepo initializes a repository whose first commit carries the
// given version tag, then appends the given unreleased commits.
func prepareReleaseRepo(t *testing.T, tag string, postTagCommits ...string) {
	t.Helper()

	initGitTestRepo(t)
	commitTestMessage(t, initialCommitMsg)
	runGitCommand(t, "tag", tag)
	for _, message := range postTagCommits {
		commitTestMessage(t, message)
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestBumpVersionSuccess(t *testing.T) {
	initGitTestRepo(t)

	if err := os.WriteFile("VERSION", []byte("1.0.0\n"), 0o600); err != nil {
		t.Fatalf("could not write VERSION: %v", err)
	}
	runGitCommand(t, "add", "VERSION")
	commitTestMessage(t, initialCommitMsg)
	runGitCommand(t, "tag", tagV100)
	commitTestMessage(t, featWidgetMsg)

	cfg := getDefaultConfig()
	cfg.VersionFilenames = []string{"VERSION"}

	var stdout bytes.Buffer
	opts := bumpOptions{Force: true, Stdin: strings.NewReader(""), Stdout: &stdout}

	if err := bumpVersion(&cfg, opts); err != nil {
		t.Fatalf("bumpVersion() returned error: %v", err)
	}

	versionFile, err := os.ReadFile("VERSION")
	if err != nil {
		t.Fatalf("could not read VERSION: %v", err)
	}
	if got := string(versionFile); got != "1.1.0" {
		t.Errorf("VERSION = %q, want %q", got, "1.1.0")
	}

	changelog, err := os.ReadFile("CHANGELOG.md")
	if err != nil {
		t.Fatalf("could not read CHANGELOG.md: %v", err)
	}
	if !strings.Contains(string(changelog), "## [1.1.0]") {
		t.Error("CHANGELOG.md does not contain the 1.1.0 release section")
	}
	if !strings.Contains(string(changelog), "add widget") {
		t.Error("CHANGELOG.md does not contain the feat commit")
	}

	if got := strings.TrimSpace(runGitCommand(t, "log", "-1", "--format=%s")); got != releaseCommitMsg {
		t.Errorf("release commit subject = %q, want %q", got, releaseCommitMsg)
	}

	if got := strings.TrimSpace(runGitCommand(t, "cat-file", "-t", tagV110)); got != "tag" {
		t.Errorf("tag %s object type = %q, want annotated %q", tagV110, got, "tag")
	}

	tagMessage := strings.TrimSpace(runGitCommand(
		t,
		"for-each-ref", "--format=%(contents)", "refs/tags/"+tagV110,
	))
	if tagMessage != "Release 1.1.0" {
		t.Errorf("tag message = %q, want %q", tagMessage, "Release 1.1.0")
	}

	if got := strings.TrimSpace(runGitCommand(t, "status", "--porcelain")); got != "" {
		t.Errorf("worktree is not clean after the release: %q", got)
	}

	output := stdout.String()
	if !strings.Contains(output, "Release:     minor") {
		t.Errorf("release plan output missing type:\n%s", output)
	}
	if !strings.Contains(output, tagV110) {
		t.Errorf("release plan output missing tag %s:\n%s", tagV110, output)
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestBumpVersionDryRun(t *testing.T) {
	prepareReleaseRepo(t, tagV100, featWidgetMsg)

	cfg := getDefaultConfig()

	var stdout bytes.Buffer
	opts := bumpOptions{
		Force:  true,
		DryRun: true,
		Stdin:  strings.NewReader(""),
		Stdout: &stdout,
	}

	if err := bumpVersion(&cfg, opts); err != nil {
		t.Fatalf("bumpVersion() returned error: %v", err)
	}

	output := stdout.String()
	if !strings.Contains(output, "1.1.0") {
		t.Errorf("dry run output does not mention the next version:\n%s", output)
	}
	if !strings.Contains(output, tagV110) {
		t.Errorf("dry run output does not mention the release tag:\n%s", output)
	}
	if !strings.Contains(output, "Dry run: nothing was modified.") {
		t.Errorf("dry run output misses the dry run notice:\n%s", output)
	}

	if fileExists("CHANGELOG.md") {
		t.Error("dry run wrote CHANGELOG.md")
	}
	if got := strings.TrimSpace(runGitCommand(t, "status", "--porcelain")); got != "" {
		t.Errorf("dry run modified the worktree: %q", got)
	}
	if got := strings.TrimSpace(runGitCommand(t, "tag", "-l")); got != tagV100 {
		t.Errorf("dry run changed tags: %q, want only %q", got, tagV100)
	}
	if got := strings.TrimSpace(runGitCommand(t, "log", "-1", "--format=%s")); got != featWidgetMsg {
		t.Errorf("dry run created a commit: %q", got)
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestBumpVersionRefusesDirtyWorktree(t *testing.T) {
	prepareReleaseRepo(t, tagV100, featWidgetMsg)

	if err := os.WriteFile("scratch.txt", []byte("wip"), 0o600); err != nil {
		t.Fatalf("could not create untracked file: %v", err)
	}

	cfg := getDefaultConfig()
	opts := bumpOptions{Force: true, Stdin: strings.NewReader(""), Stdout: io.Discard}

	err := bumpVersion(&cfg, opts)
	if err == nil || !strings.Contains(err.Error(), "working tree is not clean") {
		t.Errorf("bumpVersion() error = %v, want a dirty worktree error", err)
	}

	if fileExists("CHANGELOG.md") {
		t.Error("refused release wrote CHANGELOG.md")
	}
	if got := strings.TrimSpace(runGitCommand(t, "tag", "-l")); got != tagV100 {
		t.Errorf("refused release changed tags: %q, want only %q", got, tagV100)
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestBumpVersionRefusesStagedChanges(t *testing.T) {
	prepareReleaseRepo(t, tagV100, featWidgetMsg)

	if err := os.WriteFile("scratch.txt", []byte("wip"), 0o600); err != nil {
		t.Fatalf("could not create file: %v", err)
	}
	runGitCommand(t, "add", "scratch.txt")

	cfg := getDefaultConfig()
	opts := bumpOptions{Force: true, Stdin: strings.NewReader(""), Stdout: io.Discard}

	err := bumpVersion(&cfg, opts)
	want := "refusing to release: changes are already staged - commit or reset them first"
	if err == nil || !strings.Contains(err.Error(), want) {
		t.Errorf("bumpVersion() error = %v, want %q", err, want)
	}

	if fileExists("CHANGELOG.md") {
		t.Error("refused release wrote CHANGELOG.md")
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestBumpVersionPromptDeclined(t *testing.T) {
	prepareReleaseRepo(t, tagV100, featWidgetMsg)

	cfg := getDefaultConfig()
	var stdout bytes.Buffer
	opts := bumpOptions{Stdin: strings.NewReader("n\n"), Stdout: &stdout}

	if err := bumpVersion(&cfg, opts); err != nil {
		t.Fatalf("bumpVersion() returned error: %v", err)
	}

	output := stdout.String()
	if !strings.Contains(output, "Release 1.1.0? [y/N]: ") {
		t.Errorf("confirmation prompt not shown:\n%q", output)
	}
	if !strings.Contains(output, "Aborted.") {
		t.Errorf("declined prompt did not abort:\n%q", output)
	}

	if fileExists("CHANGELOG.md") {
		t.Error("declined release wrote CHANGELOG.md")
	}
	if got := strings.TrimSpace(runGitCommand(t, "tag", "-l")); got != tagV100 {
		t.Errorf("declined release changed tags: %q, want only %q", got, tagV100)
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestBumpVersionRefusesExistingNextTag(t *testing.T) {
	prepareReleaseRepo(t, tagV100, featWidgetMsg)

	// Occupy the next tag name on a commit that is not reachable from HEAD so
	// that the current version stays 1.0.0 while the next tag already exists.
	commitTestMessage(t, "chore: throwaway")
	throwawayHash := strings.TrimSpace(runGitCommand(t, "rev-parse", "HEAD"))
	runGitCommand(t, "reset", "--hard", "HEAD~1")
	runGitCommand(t, "tag", tagV110, throwawayHash)

	cfg := getDefaultConfig()
	opts := bumpOptions{Force: true, Stdin: strings.NewReader(""), Stdout: io.Discard}

	err := bumpVersion(&cfg, opts)
	if err == nil || !strings.Contains(err.Error(), "release tag "+tagV110+" already exists") {
		t.Errorf("bumpVersion() error = %v, want an existing tag error", err)
	}

	if fileExists("CHANGELOG.md") {
		t.Error("refused release wrote CHANGELOG.md")
	}
	if got := strings.TrimSpace(runGitCommand(t, "log", "-1", "--format=%s")); got != featWidgetMsg {
		t.Errorf("HEAD changed to %q, want %q", got, featWidgetMsg)
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestBumpVersionNothingToRelease(t *testing.T) {
	prepareReleaseRepo(t, tagV100, "chore: tidy up")

	cfg := getDefaultConfig()
	opts := bumpOptions{Force: true, Stdin: strings.NewReader(""), Stdout: io.Discard}

	if err := bumpVersion(&cfg, opts); err != nil {
		t.Fatalf("bumpVersion() returned error: %v", err)
	}

	if fileExists("CHANGELOG.md") {
		t.Error("no-op release wrote CHANGELOG.md")
	}
	if got := strings.TrimSpace(runGitCommand(t, "tag", "-l")); got != tagV100 {
		t.Errorf("no-op release changed tags: %q, want only %q", got, tagV100)
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestBumpVersionForcedReleaseType(t *testing.T) {
	prepareReleaseRepo(t, tagV100, "chore: tidy up")

	cfg := getDefaultConfig()
	opts := bumpOptions{
		Force:       true,
		ReleaseType: ReleaseTypeMinor,
		Stdin:       strings.NewReader(""),
		Stdout:      io.Discard,
	}

	if err := bumpVersion(&cfg, opts); err != nil {
		t.Fatalf("bumpVersion() returned error: %v", err)
	}

	runGitCommand(t, "rev-parse", "--verify", "refs/tags/"+tagV110)
	if got := strings.TrimSpace(runGitCommand(t, "log", "-1", "--format=%s")); got != releaseCommitMsg {
		t.Errorf("release commit subject = %q, want %q", got, releaseCommitMsg)
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestBumpVersionForcedReleaseTypeRefusesEmptyWindow(t *testing.T) {
	initGitTestRepo(t)
	commitTestMessage(t, initialCommitMsg)
	runGitCommand(t, "tag", tagV100)

	cfg := getDefaultConfig()
	opts := bumpOptions{
		Force:       true,
		ReleaseType: ReleaseTypeMinor,
		Stdin:       strings.NewReader(""),
		Stdout:      io.Discard,
	}

	if err := bumpVersion(&cfg, opts); err != nil {
		t.Fatalf("bumpVersion() returned error: %v", err)
	}

	if fileExists("CHANGELOG.md") {
		t.Error("empty window release wrote CHANGELOG.md")
	}
	if got := strings.TrimSpace(runGitCommand(t, "tag", "-l")); got != tagV100 {
		t.Errorf("empty window release changed tags: %q, want only %q", got, tagV100)
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestBumpVersionCustomTagFormat(t *testing.T) {
	prepareReleaseRepo(t, tagRelV100, featWidgetMsg)

	cfg := getDefaultConfig()
	cfg.VersionTagFormat = relTagFormat

	opts := bumpOptions{Force: true, Stdin: strings.NewReader(""), Stdout: io.Discard}

	if err := bumpVersion(&cfg, opts); err != nil {
		t.Fatalf("bumpVersion() returned error: %v", err)
	}

	runGitCommand(t, "rev-parse", "--verify", "refs/tags/"+tagRelV110)

	if _, _, err := getCommandOutput(
		gitCommand,
		"rev-parse", "--verify", "--quiet", "refs/tags/"+tagV110,
	); err == nil {
		t.Errorf("release also created the unexpected tag %s", tagV110)
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestBumpVersionPushesToOrigin(t *testing.T) {
	prepareReleaseRepo(t, tagV100, featWidgetMsg)

	originDir := t.TempDir()
	runGitCommand(t, "-C", originDir, "init", "--bare")
	runGitCommand(t, "remote", "add", "origin", originDir)

	branch := strings.TrimSpace(runGitCommand(t, "rev-parse", "--abbrev-ref", "HEAD"))
	runGitCommand(t, "push", "--set-upstream", "origin", branch+":"+branch)

	cfg := getDefaultConfig()
	cfg.ShouldPushToOrigin = true

	opts := bumpOptions{Force: true, Stdin: strings.NewReader(""), Stdout: io.Discard}

	if err := bumpVersion(&cfg, opts); err != nil {
		t.Fatalf("bumpVersion() returned error: %v", err)
	}

	remoteTags := runGitCommand(t, "--git-dir="+originDir, "tag")
	if !strings.Contains(remoteTags, tagV110) {
		t.Errorf("remote tags %q do not contain %s", remoteTags, tagV110)
	}

	remoteHead := strings.TrimSpace(runGitCommand(
		t,
		"--git-dir="+originDir, "log", "-1", "--format=%s",
	))
	if remoteHead != releaseCommitMsg {
		t.Errorf("remote HEAD subject = %q, want %q", remoteHead, releaseCommitMsg)
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestBumpVersionPrintsNextStepsWithoutPush(t *testing.T) {
	prepareReleaseRepo(t, tagV100, featWidgetMsg)

	cfg := getDefaultConfig()

	var stdout bytes.Buffer
	opts := bumpOptions{Force: true, Stdin: strings.NewReader(""), Stdout: &stdout}

	if err := bumpVersion(&cfg, opts); err != nil {
		t.Fatalf("bumpVersion() returned error: %v", err)
	}

	branch := strings.TrimSpace(runGitCommand(t, "rev-parse", "--abbrev-ref", "HEAD"))
	output := stdout.String()
	if !strings.Contains(output, "Next steps:") {
		t.Errorf("release output misses next steps:\n%s", output)
	}
	if !strings.Contains(output, "git push origin "+branch+" --follow-tags") {
		t.Errorf("release output misses the push command:\n%s", output)
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestBumpVersionChangelogCompareLink(t *testing.T) {
	prepareReleaseRepo(t, tagV100, featWidgetMsg)
	runGitCommand(t, "remote", "add", "origin", "https://github.com/acme/app.git")

	cfg := getDefaultConfig()
	opts := bumpOptions{Force: true, Stdin: strings.NewReader(""), Stdout: io.Discard}

	if err := bumpVersion(&cfg, opts); err != nil {
		t.Fatalf("bumpVersion() returned error: %v", err)
	}

	changelog, err := os.ReadFile("CHANGELOG.md")
	if err != nil {
		t.Fatalf("could not read CHANGELOG.md: %v", err)
	}

	compareLink := "(https://github.com/acme/app/compare/" + tagV100 + "..." + tagV110 + ")"
	if !strings.Contains(string(changelog), compareLink) {
		t.Errorf("CHANGELOG.md misses the compare link %s:\n%s", compareLink, changelog)
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestPlanReleaseForcedTypeOverridesDerivedType(t *testing.T) {
	prepareReleaseRepo(t, tagV100, "chore: tidy up")

	cfg := getDefaultConfig()
	opts := bumpOptions{ReleaseType: ReleaseTypePatch}

	plan, err := planRelease(&cfg, opts)
	if err != nil {
		t.Fatalf("planRelease() returned error: %v", err)
	}

	if plan.releaseType != ReleaseTypePatch {
		t.Errorf("release type = %q, want %q", plan.releaseType, ReleaseTypePatch)
	}
	if got := plan.next.ToString(); got != "1.0.1" {
		t.Errorf("next version = %q, want %q", got, "1.0.1")
	}
	if plan.tagName != "v1.0.1" {
		t.Errorf("tag name = %q, want %q", plan.tagName, "v1.0.1")
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestPlanReleaseNothingToRelease(t *testing.T) {
	prepareReleaseRepo(t, tagV100, "chore: tidy up")

	cfg := getDefaultConfig()

	plan, err := planRelease(&cfg, bumpOptions{})
	if err != nil {
		t.Fatalf("planRelease() returned error: %v", err)
	}
	if plan.releaseType != ReleaseTypeNone {
		t.Errorf("release type = %q, want %q", plan.releaseType, ReleaseTypeNone)
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestRequireCleanWorktree(t *testing.T) {
	initGitTestRepo(t)

	if err := requireCleanWorktree(); err != nil {
		t.Errorf("requireCleanWorktree() on a clean repository: %v", err)
	}

	if err := os.WriteFile("scratch.txt", []byte("x"), 0o600); err != nil {
		t.Fatalf("could not write file: %v", err)
	}
	err := requireCleanWorktree()
	if err == nil || !strings.Contains(err.Error(), "working tree is not clean") {
		t.Errorf("requireCleanWorktree() with an untracked file: %v", err)
	}

	runGitCommand(t, "add", "scratch.txt")
	err = requireCleanWorktree()
	if err == nil || !strings.Contains(err.Error(), "already staged") {
		t.Errorf("requireCleanWorktree() with a staged file: %v", err)
	}
}
