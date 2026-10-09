package main

import (
	"bytes"
	"fmt"
	"os"
	"strings"
)

const (
	defaultConfigFilename = "bump-version.cfg"
	versionTagFormatSubst = "{version}" // substitute for an actual version
)

type Config struct {
	Version              string
	VersionFilenames     []string
	PackageJSONFilename  string
	ChangeLogFilename    string
	IgnoreInvalidCommits bool
	VersionTagFormat     string
	AllowedCommitKinds   []string
	BumpVersionCommit    string
	ShouldPushToOrigin   bool
}

// getDefaultConfig returns the default config as read-only.
func getDefaultConfig() Config {
	return Config{
		Version:              "1.0.0",
		VersionFilenames:     []string{},
		PackageJSONFilename:  "",
		ChangeLogFilename:    "CHANGELOG.md",
		IgnoreInvalidCommits: false,
		VersionTagFormat:     "v{version}",
		AllowedCommitKinds: []string{
			"BREAKING CHANGE",
			"build",
			"chore",
			"ci",
			"docs",
			"feat",
			"fix",
			"perf",
			"refactor",
			"revert",
			"style",
			"test",
		},
		BumpVersionCommit:  "chore(release): {version}",
		ShouldPushToOrigin: false,
	}
}

// loadConfig parses name on top of the built-in defaults. An empty name
// reads the default config file when it exists and otherwise yields the
// defaults untouched.
func loadConfig(name string) (Config, error) {
	defaultConfig := getDefaultConfig()

	// If the config file name is empty, read the default config file,
	// but even if that file does not exist, use the default config
	// object.
	if name == "" {
		name = defaultConfigFilename
		if !fileExists(name) {
			return defaultConfig, nil
		}
	}

	// Read config from file
	//nolint:gosec // the config file name comes from this program's own command line
	cfgData, err := os.ReadFile(name)
	if err != nil {
		return Config{}, err
	}

	cfg := defaultConfig // base off the default config
	lineNo := 1
	for line := range strings.Lines(string(cfgData)) {
		line = strings.TrimSpace(line) // ignore space
		if line == "" || strings.HasPrefix(line, "#") {
			// Ignore empty line or comments
			lineNo++
			continue
		}
		parts := strings.SplitN(line, "=", 2)
		if len(parts) != 2 {
			// Only key value pairs expected
			err := fmt.Errorf(
				"invalid config line %d format, must be =-separated key-value pair",
				lineNo,
			)
			return Config{}, err
		}
		// No spaces around equals
		key := strings.TrimSpace(parts[0])
		value := strings.TrimSpace(parts[1])
		// Overwrite the default parameters
		switch key {
		case "version":
			cfg.Version = value
		case "versionFilenames":
			cfg.VersionFilenames = strings.Split(value, ",")
			for i := range cfg.VersionFilenames {
				cfg.VersionFilenames[i] = strings.TrimSpace(cfg.VersionFilenames[i])
			}
		case "packageJSONFilename":
			cfg.PackageJSONFilename = value
		case "changeLogFilename":
			cfg.ChangeLogFilename = value
		case "ignoreInvalidCommits":
			cfg.IgnoreInvalidCommits = value == "true"
		case "versionTagFormat":
			cfg.VersionTagFormat = value
			// Validate version tag format
			if strings.Count(cfg.VersionTagFormat, versionTagFormatSubst) != 1 {
				err := fmt.Errorf(
					"version tag format %q expected to have one occurrence of %s",
					cfg.VersionTagFormat,
					versionTagFormatSubst,
				)
				return Config{}, err
			}
		case "allowedCommitKinds":
			cfg.AllowedCommitKinds = strings.Split(value, ",")
			for i := range cfg.AllowedCommitKinds {
				cfg.AllowedCommitKinds[i] = strings.TrimSpace(cfg.AllowedCommitKinds[i])
			}
		case "bumpVersionCommit":
			cfg.BumpVersionCommit = value
		case "shouldPushToOrigin":
			cfg.ShouldPushToOrigin = value == "true"
		}
		lineNo++
	}

	return cfg, nil
}

// saveConfig serialises config as key=value lines and writes them to
// configFilename with the given permissions.
func saveConfig(configFilename string, config Config, perm os.FileMode) error {
	var b bytes.Buffer
	fmt.Fprintf(&b, "version=%s\n", config.Version)
	fmt.Fprintf(&b, "versionFilenames=%s\n", strings.Join(config.VersionFilenames, ","))
	fmt.Fprintf(&b, "packageJSONFilename=%s\n", config.PackageJSONFilename)
	fmt.Fprintf(&b, "changeLogFilename=%s\n", config.ChangeLogFilename)
	fmt.Fprintf(&b, "ignoreInvalidCommits=%t\n", config.IgnoreInvalidCommits)
	fmt.Fprintf(&b, "versionTagFormat=%s\n", config.VersionTagFormat)
	fmt.Fprintf(&b, "allowedCommitKinds=%s\n", strings.Join(config.AllowedCommitKinds, ","))
	fmt.Fprintf(&b, "bumpVersionCommit=%s\n", config.BumpVersionCommit)
	fmt.Fprintf(&b, "shouldPushToOrigin=%t\n", config.ShouldPushToOrigin)
	return os.WriteFile(configFilename, b.Bytes(), perm)
}
