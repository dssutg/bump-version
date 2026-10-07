package main

import (
	"errors"
	"strings"
	"testing"
)

func initGitTestRepo(t *testing.T) {
	t.Helper()

	t.Setenv("GIT_CONFIG_GLOBAL", "/dev/null")
	t.Setenv("GIT_CONFIG_SYSTEM", "/dev/null")
	t.Chdir(t.TempDir())

	runGitCommand(t, "init")
	runGitCommand(t, "config", "user.name", "Test User")
	runGitCommand(t, "config", "user.email", "test@example.com")
	runGitCommand(t, "config", "commit.gpgsign", "false")
}

func runGitCommand(t *testing.T, args ...string) string {
	t.Helper()

	out, errOut, err := getCommandOutput(gitCommand, args...)
	if err != nil {
		t.Fatalf("git %s failed: %v: %s", strings.Join(args, " "), err, errOut)
	}
	return out
}

func commitTestMessage(t *testing.T, message string) {
	t.Helper()

	runGitCommand(t, "commit", "--allow-empty", "-m", message)
}

func TestCommitStatsDetermineReleaseType(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		stats CommitStats
		want  ReleaseType
	}{
		{"no commits is none", CommitStats{}, ReleaseTypeNone},
		{
			"breaking change is major",
			CommitStats{HasBreakingChange: true, ByKind: map[string][]Commit{CommitKindFeat: {{}}}},
			ReleaseTypeMajor,
		},
		{
			"feat is minor",
			CommitStats{ByKind: map[string][]Commit{CommitKindFeat: {{}}}},
			ReleaseTypeMinor,
		},
		{
			"fix is patch",
			CommitStats{ByKind: map[string][]Commit{CommitKindFix: {{}}}},
			ReleaseTypePatch,
		},
		{
			"perf is patch",
			CommitStats{ByKind: map[string][]Commit{CommitKindPerf: {{}}}},
			ReleaseTypePatch,
		},
		{
			"chore is none",
			CommitStats{ByKind: map[string][]Commit{CommitKindChore: {{}}}},
			ReleaseTypeNone,
		},
		{
			"breaking wins over feat",
			CommitStats{HasBreakingChange: true, ByKind: map[string][]Commit{CommitKindFix: {{}}}},
			ReleaseTypeMajor,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := tt.stats.DetermineReleaseType(); got != tt.want {
				t.Errorf("DetermineReleaseType() = %q, want %q", got, tt.want)
			}
		})
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestProcessCommitMessagesDeterminesReleaseTypes(t *testing.T) {
	allowedKinds := getDefaultConfig().AllowedCommitKinds

	tests := []struct {
		name    string
		commits []string
		want    ReleaseType
	}{
		{"feat is minor", []string{"feat: add gadget"}, ReleaseTypeMinor},
		{"fix is patch", []string{"fix: repair widget"}, ReleaseTypePatch},
		{"perf is patch", []string{"perf: speed up search"}, ReleaseTypePatch},
		{"chore is none", []string{"chore: tidy up"}, ReleaseTypeNone},
		{
			"breaking footer is major",
			[]string{"feat: add api\n\nelaborated details\n\nBREAKING CHANGE: api changed"},
			ReleaseTypeMajor,
		},
		{"bang is major", []string{"feat!: drop old api"}, ReleaseTypeMajor},
	}

	for _, tt := range tests {
		initGitTestRepo(t)
		for _, message := range tt.commits {
			commitTestMessage(t, message)
		}

		stats, err := processCommitMessages("", false, allowedKinds)
		if err != nil {
			t.Fatalf("%s: processCommitMessages() returned error: %v", tt.name, err)
		}
		if got := stats.DetermineReleaseType(); got != tt.want {
			t.Errorf("%s: DetermineReleaseType() = %q, want %q", tt.name, got, tt.want)
		}
		if stats.InWindowCommits != len(tt.commits) {
			t.Errorf("%s: InWindowCommits = %d, want %d", tt.name, stats.InWindowCommits, len(tt.commits))
		}
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestProcessCommitMessagesSinceTagExcludesPreTagCommits(t *testing.T) {
	initGitTestRepo(t)

	commitTestMessage(t, "chore: initial setup")
	runGitCommand(t, "tag", "v1.0.0")
	commitTestMessage(t, "fix: repair widget")
	commitTestMessage(t, "feat: add gadget")

	stats, err := processCommitMessages(
		"v1.0.0",
		false,
		getDefaultConfig().AllowedCommitKinds,
	)
	if err != nil {
		t.Fatalf("processCommitMessages() returned error: %v", err)
	}

	if stats.InWindowCommits != 2 {
		t.Errorf("InWindowCommits = %d, want 2", stats.InWindowCommits)
	}
	if len(stats.ByKind) != 2 {
		t.Errorf("ByKind has %d kinds, want 2: %v", len(stats.ByKind), stats.ByKind)
	}
	if len(stats.ByKind[CommitKindChore]) != 0 {
		t.Error("pre-tag chore commit leaked into ByKind")
	}
	if got := stats.DetermineReleaseType(); got != ReleaseTypeMinor {
		t.Errorf("DetermineReleaseType() = %q, want %q", got, ReleaseTypeMinor)
	}
	if stats.HasBreakingChange {
		t.Error("HasBreakingChange = true, want false")
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestProcessCommitMessagesInvalidCommits(t *testing.T) {
	initGitTestRepo(t)

	commitTestMessage(t, "feat: valid work")
	commitTestMessage(t, "this message is invalid")

	allowedKinds := getDefaultConfig().AllowedCommitKinds

	if _, err := processCommitMessages("", false, allowedKinds); !errors.Is(err, ErrFoundInvalidCommits) {
		t.Errorf("strict mode error = %v, want ErrFoundInvalidCommits", err)
	}

	stats, err := processCommitMessages("", true, allowedKinds)
	if err != nil {
		t.Fatalf("lenient mode returned error: %v", err)
	}
	if stats.InWindowCommits != 1 {
		t.Errorf("InWindowCommits = %d, want 1", stats.InWindowCommits)
	}
	if got := stats.DetermineReleaseType(); got != ReleaseTypeMinor {
		t.Errorf("DetermineReleaseType() = %q, want %q", got, ReleaseTypeMinor)
	}
}

//nolint:paralleltest // t.Chdir changes the process working directory
func TestProcessCommitMessagesRevertedPreTagCommit(t *testing.T) {
	initGitTestRepo(t)

	commitTestMessage(t, "feat!: remove old api")
	fullHash := strings.TrimSpace(runGitCommand(t, "rev-parse", "HEAD"))
	runGitCommand(t, "tag", "v1.0.0")
	commitTestMessage(t, "revert: remove old api\n\nRefs: "+fullHash)

	stats, err := processCommitMessages(
		"v1.0.0",
		false,
		getDefaultConfig().AllowedCommitKinds,
	)
	if err != nil {
		t.Fatalf("processCommitMessages() returned error: %v", err)
	}

	if stats.InWindowCommits != 1 {
		t.Errorf("InWindowCommits = %d, want 1", stats.InWindowCommits)
	}
	if len(stats.ByKind) != 0 {
		t.Errorf("ByKind = %v, want no kind sections (revert commits are excluded)", stats.ByKind)
	}
	if len(stats.RevertedPreTag) != 1 {
		t.Fatalf("RevertedPreTag has %d commits, want 1", len(stats.RevertedPreTag))
	}
	if got := stats.RevertedPreTag[0].Hash; got != fullHash {
		t.Errorf("RevertedPreTag hash = %q, want %q", got, fullHash)
	}
	if !stats.HasBreakingChange {
		t.Error("HasBreakingChange = false, want true (reverted pre-tag breaking commit)")
	}
	if got := stats.DetermineReleaseType(); got != ReleaseTypeMajor {
		t.Errorf("DetermineReleaseType() = %q, want %q", got, ReleaseTypeMajor)
	}
}
