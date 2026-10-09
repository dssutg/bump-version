package main

import (
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

//nolint:paralleltest // t.Chdir changes the process working directory
func TestLoadConfigUsesDefaultsWhenFileAbsent(t *testing.T) {
	t.Chdir(t.TempDir())

	cfg, err := loadConfig("")
	if err != nil {
		t.Fatalf("loadConfig() returned error: %v", err)
	}

	if !reflect.DeepEqual(cfg, getDefaultConfig()) {
		t.Errorf("loadConfig() = %+v, want the defaults %+v", cfg, getDefaultConfig())
	}
}

func TestLoadConfigFullParse(t *testing.T) {
	t.Parallel()

	path := filepath.Join(t.TempDir(), defaultConfigFilename)
	content := `# project configuration
version=2.3.4

versionFilenames=a.txt, b.txt
packageJSONFilename=web/package.json
changeLogFilename=HISTORY.md
ignoreInvalidCommits=true
versionTagFormat=rel-{version}
allowedCommitKinds= feat, fix , style
bumpVersionCommit=release: {version}
shouldPushToOrigin=true
`
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("could not write config: %v", err)
	}

	cfg, err := loadConfig(path)
	if err != nil {
		t.Fatalf("loadConfig() returned error: %v", err)
	}

	if cfg.Version != "2.3.4" {
		t.Errorf("Version = %q, want %q", cfg.Version, "2.3.4")
	}
	if want := []string{"a.txt", "b.txt"}; !reflect.DeepEqual(cfg.VersionFilenames, want) {
		t.Errorf("VersionFilenames = %q, want %q", cfg.VersionFilenames, want)
	}
	if cfg.PackageJSONFilename != "web/package.json" {
		t.Errorf("PackageJSONFilename = %q, want %q", cfg.PackageJSONFilename, "web/package.json")
	}
	if cfg.ChangeLogFilename != "HISTORY.md" {
		t.Errorf("ChangeLogFilename = %q, want %q", cfg.ChangeLogFilename, "HISTORY.md")
	}
	if !cfg.IgnoreInvalidCommits {
		t.Error("IgnoreInvalidCommits = false, want true")
	}
	if cfg.VersionTagFormat != relTagFormat {
		t.Errorf("VersionTagFormat = %q, want %q", cfg.VersionTagFormat, "rel-{version}")
	}
	if want := []string{CommitKindFeat, CommitKindFix, "style"}; !reflect.DeepEqual(cfg.AllowedCommitKinds, want) {
		t.Errorf("AllowedCommitKinds = %q, want %q", cfg.AllowedCommitKinds, want)
	}
	if cfg.BumpVersionCommit != "release: {version}" {
		t.Errorf("BumpVersionCommit = %q, want %q", cfg.BumpVersionCommit, "release: {version}")
	}
	if !cfg.ShouldPushToOrigin {
		t.Error("ShouldPushToOrigin = false, want true")
	}
}

func TestLoadConfigRejectsInvalidLine(t *testing.T) {
	t.Parallel()

	path := filepath.Join(t.TempDir(), defaultConfigFilename)
	if err := os.WriteFile(path, []byte("this is not a key value pair\n"), 0o600); err != nil {
		t.Fatalf("could not write config: %v", err)
	}

	_, err := loadConfig(path)
	if err == nil || !strings.Contains(err.Error(), "invalid config line 1 format") {
		t.Errorf("loadConfig() error = %v, want an invalid line error", err)
	}
}

func TestLoadConfigValidatesVersionTagFormat(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		value string
	}{
		{"missing placeholder", "vX.Y.Z"},
		{"duplicate placeholder", "{version}.{version}"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			path := filepath.Join(t.TempDir(), defaultConfigFilename)
			content := "versionTagFormat=" + tt.value + "\n"
			if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
				t.Fatalf("could not write config: %v", err)
			}

			_, err := loadConfig(path)
			if err == nil || !strings.Contains(err.Error(), "expected to have one occurrence") {
				t.Errorf("loadConfig() error = %v, want a placeholder count error", err)
			}
		})
	}
}

func TestLoadConfigIgnoresUnknownKeys(t *testing.T) {
	t.Parallel()

	path := filepath.Join(t.TempDir(), defaultConfigFilename)
	content := "unknownKey=whatever\nversion=9.9.9\n"
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatalf("could not write config: %v", err)
	}

	cfg, err := loadConfig(path)
	if err != nil {
		t.Fatalf("loadConfig() returned error: %v", err)
	}
	if cfg.Version != "9.9.9" {
		t.Errorf("Version = %q, want %q", cfg.Version, "9.9.9")
	}
}

func TestSaveLoadConfigRoundtrip(t *testing.T) {
	t.Parallel()

	cfg := getDefaultConfig()
	cfg.Version = "3.2.1"
	cfg.VersionFilenames = []string{"a.txt", "b.txt"}
	cfg.PackageJSONFilename = "package.json"
	cfg.ChangeLogFilename = "HISTORY.md"
	cfg.IgnoreInvalidCommits = true
	cfg.VersionTagFormat = relTagFormat
	cfg.AllowedCommitKinds = []string{CommitKindFeat, CommitKindFix}
	cfg.BumpVersionCommit = "release: {version}"
	cfg.ShouldPushToOrigin = true

	path := filepath.Join(t.TempDir(), defaultConfigFilename)
	if err := saveConfig(path, cfg, 0o600); err != nil {
		t.Fatalf("saveConfig() returned error: %v", err)
	}

	loaded, err := loadConfig(path)
	if err != nil {
		t.Fatalf("loadConfig() returned error: %v", err)
	}
	if !reflect.DeepEqual(loaded, cfg) {
		t.Errorf("roundtrip = %+v, want %+v", loaded, cfg)
	}
}

func TestLoadConfigMissingCustomFileFails(t *testing.T) {
	t.Parallel()

	path := filepath.Join(t.TempDir(), "absent.cfg")

	_, err := loadConfig(path)
	if !errors.Is(err, fs.ErrNotExist) {
		t.Errorf("loadConfig() error = %v, want a not-exist error", err)
	}
}
