package main

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"
)

const (
	// changelogTestRepo is shared with other test files of this package.
	changelogTestRepo = "https://github.com/acme/app"

	hashFull    = "0123456789abcdef0123456789abcdef01234567"
	hashFix     = "bbbb222"
	hashPerf    = "cccc333"
	hashChore   = "dddd444"
	hashCustom  = "eeee555"
	hashRevert  = "ffff666"
	hashStyle   = "6543210"
	hashOld     = "abc1234"
	hashCreated = "def5678"
)

// wantChangelogHeader is an independent reimplementation of the expected
// release header, used to build expected changelog output in tests.
func wantChangelogHeader(version, compareURL, date string) string {
	if compareURL != "" {
		return fmt.Sprintf("# Changelog\n\n## [%s](%s) (%s)\n", version, compareURL, date)
	}
	return fmt.Sprintf("# Changelog\n\n## [%s] (%s)\n", version, date)
}

func TestGenerateNewChangelogHeadWithRemoteLinks(t *testing.T) {
	t.Parallel()

	date := time.Now().Format(time.DateOnly)
	cfg := &Config{VersionTagFormat: vTagFormat}
	stats := CommitStats{
		ByKind: map[string][]Commit{
			CommitKindFeat:  {{Hash: hashFull, Scope: "ui", Description: "add button", Breaking: true}},
			CommitKindFix:   {{Hash: hashFix, Description: "fix crash"}},
			CommitKindPerf:  {{Hash: hashPerf, Description: "speed up parsing"}},
			CommitKindChore: {{Hash: hashChore, Description: "update deps"}},
			"custom":        {{Hash: hashCustom, Description: "do thing"}},
		},
		RevertedPreTag: []Commit{{Hash: hashRevert, Description: "old feature"}},
	}

	current := Version{Major: 0, Minor: 1}
	next := Version{Major: 0, Minor: 2}

	head := generateNewChangelogHead(current, next, stats, cfg, changelogTestRepo)
	got := head.String()

	commitLink := func(h string) string {
		return "[" + h + "](" + changelogTestRepo + "/commit/" + h + ")"
	}
	want := wantChangelogHeader("0.2.0", changelogTestRepo+"/compare/v0.1.0...v0.2.0", date) +
		"\n\n### Features\n\n" +
		"* **ui:** [BREAKING CHANGE] add button [" + hashFull[:7] + "](" +
		changelogTestRepo + "/commit/" + hashFull + ")\n" +
		"\n\n### Bug Fixes\n\n" +
		"* fix crash " + commitLink(hashFix) + "\n" +
		"\n\n### Performance\n\n" +
		"* speed up parsing " + commitLink(hashPerf) + "\n" +
		"\n\n### Chores\n\n" +
		"* do thing " + commitLink(hashCustom) + "\n" +
		"* update deps " + commitLink(hashChore) + "\n" +
		"\n\n### Reverted Changes\n\n" +
		"* old feature " + commitLink(hashRevert) + "\n"

	if got != want {
		t.Errorf("generateNewChangelogHead() mismatch:\n--- got ---\n%s\n--- want ---\n%s", got, want)
	}
}

func TestGenerateNewChangelogHeadWithoutRemote(t *testing.T) {
	t.Parallel()

	date := time.Now().Format(time.DateOnly)
	cfg := &Config{VersionTagFormat: vTagFormat}
	stats := CommitStats{
		ByKind: map[string][]Commit{
			CommitKindStyle: {{Hash: hashStyle, Description: "align widgets"}},
		},
	}

	head := generateNewChangelogHead(Version{Minor: 2}, Version{Minor: 3}, stats, cfg, "")
	got := head.String()

	want := wantChangelogHeader("0.3.0", "", date) +
		"\n\n### Styling\n\n" +
		"* align widgets " + hashStyle + "\n"

	if got != want {
		t.Errorf("generateNewChangelogHead() mismatch:\n--- got ---\n%s\n--- want ---\n%s", got, want)
	}
}

func TestGenerateNewChangelogHeadWithNoCommits(t *testing.T) {
	t.Parallel()

	date := time.Now().Format(time.DateOnly)
	cfg := &Config{VersionTagFormat: vTagFormat}

	head := generateNewChangelogHead(Version{Major: 1}, Version{Major: 2}, CommitStats{}, cfg, changelogTestRepo)
	got := head.String()

	want := wantChangelogHeader("2.0.0", changelogTestRepo+"/compare/v1.0.0...v2.0.0", date)

	if got != want {
		t.Errorf("generateNewChangelogHead() mismatch:\n--- got ---\n%s\n--- want ---\n%s", got, want)
	}
}

func TestGenerateChangelogMergesExistingFile(t *testing.T) {
	t.Parallel()

	date := time.Now().Format(time.DateOnly)
	path := filepath.Join(t.TempDir(), "CHANGELOG.md")
	cfg := &Config{ChangeLogFilename: path, VersionTagFormat: vTagFormat}

	oldContent := "# Changelog\n\n" +
		"## [0.9.0](https://example.com/compare/v0.8.0...v0.9.0) (2026-01-01)\n" +
		"\n\n### Styling\n\n" +
		"* tidy old docs " + hashOld + "\n"
	if err := os.WriteFile(path, []byte(oldContent), 0o600); err != nil {
		t.Fatalf("could not write existing changelog: %v", err)
	}

	stats := CommitStats{
		ByKind: map[string][]Commit{
			CommitKindFix: {{Hash: hashCreated, Description: "fix new thing"}},
		},
	}

	err := generateChangelog(Version{Minor: 9}, Version{Minor: 10}, stats, cfg, "")
	if err != nil {
		t.Fatalf("generateChangelog() returned error: %v", err)
	}

	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("could not read changelog: %v", err)
	}

	want := wantChangelogHeader("0.10.0", "", date) +
		"\n\n### Bug Fixes\n\n" +
		"* fix new thing " + hashCreated + "\n" +
		"\n" +
		"## [0.9.0](https://example.com/compare/v0.8.0...v0.9.0) (2026-01-01)\n" +
		"\n\n### Styling\n\n" +
		"* tidy old docs " + hashOld + "\n" +
		"\n"

	if string(got) != want {
		t.Errorf("generateChangelog() mismatch:\n--- got ---\n%s\n--- want ---\n%s", got, want)
	}
}

func TestGenerateChangelogCreatesMissingFile(t *testing.T) {
	t.Parallel()

	date := time.Now().Format(time.DateOnly)
	path := filepath.Join(t.TempDir(), "CHANGELOG.md")
	cfg := &Config{ChangeLogFilename: path, VersionTagFormat: vTagFormat}

	stats := CommitStats{
		ByKind: map[string][]Commit{
			CommitKindStyle: {{Hash: hashStyle, Description: "align widgets"}},
		},
	}

	if err := generateChangelog(Version{Minor: 1}, Version{Minor: 2}, stats, cfg, ""); err != nil {
		t.Fatalf("generateChangelog() returned error: %v", err)
	}

	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("could not read changelog: %v", err)
	}

	want := wantChangelogHeader("0.2.0", "", date) +
		"\n\n### Styling\n\n" +
		"* align widgets " + hashStyle + "\n"

	if string(got) != want {
		t.Errorf("generateChangelog() mismatch:\n--- got ---\n%s\n--- want ---\n%s", got, want)
	}
}

func TestWithoutEmbeddedCredentials(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		in   string
		want string
	}{
		{
			"token userinfo",
			"https://x-access-token:ghp_secret123@github.com/acme/app",
			changelogTestRepo,
		},
		{
			"user and password",
			"https://user:pass@example.com/repo",
			"https://example.com/repo",
		},
		{
			"user without password",
			"https://user@example.com/repo",
			"https://example.com/repo",
		},
		{
			"credential free untouched",
			changelogTestRepo,
			changelogTestRepo,
		},
		{
			"at sign in path untouched",
			"https://example.com/repo@v2",
			"https://example.com/repo@v2",
		},
		{
			"no scheme untouched",
			"git@example.com:acme/app.git",
			"git@example.com:acme/app.git",
		},
		{
			"non http scheme untouched",
			"ftp://user@example.com/repo",
			"ftp://user@example.com/repo",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := withoutEmbeddedCredentials(tt.in); got != tt.want {
				t.Errorf("withoutEmbeddedCredentials(%q) = %q, want %q", tt.in, got, tt.want)
			}
		})
	}
}

func TestNormalizeRemoteURL(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		in   string
		want string
	}{
		{"https strips git suffix", changelogTestRepo + ".git", changelogTestRepo},
		{"http passes through", "http://gitlab.com/acme/app.git", "http://gitlab.com/acme/app"},
		{"scp-like with user", "git@github.com:acme/app.git", changelogTestRepo},
		{"scp-like without user", "github.com:acme/app", changelogTestRepo},
		{"ssh url", "ssh://git@github.com/acme/app.git", changelogTestRepo},
		{"scheme-less", "github.com/acme/app", changelogTestRepo},
		{"empty", "", ""},
		{"whitespace", "  ", ""},
		{"garbage with spaces", "not a url", ""},
		{"single word", "garbage", ""},
		{"unsupported scheme", "ftp://example.com/app", ""},
		{"missing path", "https://github.com", ""},
		{"missing path scp", "github.com:", ""},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := normalizeRemoteURL(tt.in); got != tt.want {
				t.Errorf("normalizeRemoteURL(%q) = %q, want %q", tt.in, got, tt.want)
			}
		})
	}
}

func TestSortChangeLogRecords(t *testing.T) {
	t.Parallel()

	t.Run("case insensitive with scopes", func(t *testing.T) {
		t.Parallel()

		records := []string{
			"* zulu report " + hashOld,
			"* alpha report " + hashCreated,
			"* **scope:** Beta report " + hashFix,
		}
		want := []string{
			"* alpha report " + hashCreated,
			"* **scope:** Beta report " + hashFix,
			"* zulu report " + hashOld,
		}

		sortChangeLogRecords(records)
		for i := range want {
			if records[i] != want[i] {
				t.Fatalf("records[%d] = %q, want %q", i, records[i], want[i])
			}
		}
	})

	t.Run("leading asterisks are skipped", func(t *testing.T) {
		t.Parallel()

		records := []string{"*b two", "a one"}
		want := []string{"a one", "*b two"}

		sortChangeLogRecords(records)
		for i := range want {
			if records[i] != want[i] {
				t.Fatalf("records[%d] = %q, want %q", i, records[i], want[i])
			}
		}
	})
}
