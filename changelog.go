package main

import (
	"bytes"
	"cmp"
	"fmt"
	"net/url"
	"os"
	"regexp"
	"slices"
	"strings"
	"time"
	"unicode"
	"unicode/utf8"
)

// changelogSection associates a commit kind with the changelog section that
// renders it. Sections are rendered in the order they are listed.
type changelogSection struct {
	name string
	kind string
}

// changelogSections returns the changelog kind sections in their canonical
// render order. Kinds without a dedicated section fall back to Chores.
func changelogSections() []changelogSection {
	return []changelogSection{
		{"Features", CommitKindFeat},
		{"Bug Fixes", CommitKindFix},
		{"Performance", CommitKindPerf},
		{"Refactoring", CommitKindRefactor},
		{"Documentation", CommitKindDocs},
		{"Tests", CommitKindTest},
		{"Build", CommitKindBuild},
		{"Continuous Integration", CommitKindCI},
		{"Styling", CommitKindStyle},
		{"Chores", CommitKindChore},
	}
}

func generateChangelog(currentVersion, newVersion Version, commitStats CommitStats, cfg *Config, urlBase string) error {
	newChangeLog := generateNewChangelogHead(currentVersion, newVersion, commitStats, cfg, urlBase)

	// Merge with the current Changelog if it exists
	if fileExists(cfg.ChangeLogFilename) {
		newChangeLog.WriteByte('\n')

		// Read old changelog file
		//nolint:gosec // the changelog file name comes from this project's own config
		changeLogFileContentBytes, err := os.ReadFile(cfg.ChangeLogFilename)
		if err != nil {
			return err
		}

		changeLogFileContent := string(changeLogFileContentBytes)
		changeLogLines := strings.Split(changeLogFileContent, "\n")

		// Remove old header if exists
		headerRe := regexp.MustCompile(`^\s*#\s*Changelog\s*$`)
		if len(changeLogLines) > 0 && headerRe.MatchString(changeLogLines[0]) {
			changeLogLines = changeLogLines[1:]
		}

		// Remove all leading empty lines
		for len(changeLogLines) > 0 && strings.TrimSpace(changeLogLines[0]) == "" {
			changeLogLines = changeLogLines[1:]
		}

		// Append all the rest lines of the old Changelog to the updated changelog
		for _, line := range changeLogLines {
			newChangeLog.WriteString(line)
			newChangeLog.WriteByte('\n')
		}
	}

	// Write updated Changelog
	//nolint:gosec // the changelog file name comes from this project's own config
	if err := os.WriteFile(cfg.ChangeLogFilename, newChangeLog.Bytes(), 0o600); err != nil {
		return err
	}

	return nil
}

// generateNewChangelogHead renders the release section that is prepended to
// the changelog. When urlBase is known, the release header links to the
// remote compare view and commit hashes link to their commits.
func generateNewChangelogHead(currentVersion, newVersion Version, commitStats CommitStats, cfg *Config, urlBase string) bytes.Buffer {
	curDateStr := time.Now().Format(time.DateOnly)

	var newChangeLog bytes.Buffer

	// Write Changelog header
	if urlBase != "" {
		fmt.Fprintf(&newChangeLog,
			"# Changelog\n\n## [%s](%s/compare/%s...%s) (%s)\n",
			newVersion.ToString(),
			urlBase,
			versionToGitTag(currentVersion, cfg.VersionTagFormat),
			versionToGitTag(newVersion, cfg.VersionTagFormat),
			curDateStr,
		)
	} else {
		fmt.Fprintf(&newChangeLog,
			"# Changelog\n\n## [%s] (%s)\n",
			newVersion.ToString(),
			curDateStr,
		)
	}

	// Write kind sections in canonical order; collect kinds that have no
	// dedicated section so they can be reported under Chores
	sections := changelogSections()
	knownKinds := make(map[string]bool, len(sections))
	for _, section := range sections {
		knownKinds[section.kind] = true
	}

	for _, section := range sections {
		commits := commitStats.ByKind[section.kind]
		if section.kind == CommitKindChore {
			for kind, kindCommits := range commitStats.ByKind {
				if !knownKinds[kind] {
					commits = append(commits, kindCommits...)
				}
			}
		}
		writeChangelogSection(&newChangeLog, section.name, commits, urlBase)
	}

	writeChangelogSection(&newChangeLog, "Reverted Changes", commitStats.RevertedPreTag, urlBase)

	return newChangeLog
}

func writeChangelogSection(newChangeLog *bytes.Buffer, sectionName string, commits []Commit, urlBase string) {
	if len(commits) == 0 {
		return
	}

	// Write header
	newChangeLog.WriteString("\n\n### ")
	newChangeLog.WriteString(sectionName)
	newChangeLog.WriteString("\n\n")

	// Stringify and sort list
	sorted := make([]string, len(commits))
	for i, commit := range commits {
		sorted[i] = commitToChangelogRecord(commit, urlBase)
	}
	sortChangeLogRecords(sorted)

	// Write record list
	for _, record := range sorted {
		newChangeLog.WriteString(record)
		newChangeLog.WriteByte('\n')
	}
}

func commitToChangelogRecord(c Commit, urlBase string) string {
	hash := c.ShortHash()
	if urlBase != "" {
		hash = fmt.Sprintf("[%s](%s/commit/%s)", c.ShortHash(), urlBase, c.Hash)
	}

	breakingChange := ""
	if c.Breaking {
		breakingChange = " [BREAKING CHANGE]"
	}
	if c.Scope != "" {
		return fmt.Sprintf("* **%s:**%s %s %s", c.Scope, breakingChange, c.Description, hash)
	}
	return fmt.Sprintf("*%s %s %s", breakingChange, c.Description, hash)
}

// resolveURLBase returns the web base URL of the given Git remote for
// changelog links. When the remote is missing or cannot be interpreted it
// warns once on stderr and returns "", which omits all links.
func resolveURLBase(remote string) string {
	out, errOut, err := getCommandOutput(gitCommand, "remote", "get-url", remote)
	if err != nil {
		detail := strings.TrimSpace(errOut)
		if detail == "" {
			detail = err.Error()
		}
		warnf("WARNING: could not get URL of remote %q: %s; changelog links will be omitted", remote, detail)
		return ""
	}

	base := withoutEmbeddedCredentials(normalizeRemoteURL(out))
	if base == "" {
		warnf("WARNING: could not interpret URL of remote %q; changelog links will be omitted", remote)
		return ""
	}
	return base
}

// normalizeRemoteURL converts a Git remote URL into a repository base URL
// suitable for changelog links. HTTP(S) URLs pass through, SSH and SCP-like
// (git@host:path) URLs are rewritten to HTTPS, and scheme-less host/owner/repo
// forms gain an https:// prefix. It returns "" for anything that cannot be
// interpreted as a repository URL.
func normalizeRemoteURL(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" || strings.ContainsAny(raw, " \t") {
		return ""
	}

	if !strings.Contains(raw, "://") {
		// SCP-like syntax: [user@]host:path
		if colon := strings.IndexByte(raw, ':'); colon >= 0 && !strings.Contains(raw[:colon], "/") {
			host := raw[:colon]
			if _, hostWithoutUser, ok := strings.Cut(host, "@"); ok {
				host = hostWithoutUser
			}
			path := strings.TrimSuffix(strings.TrimLeft(raw[colon+1:], "/"), ".git")
			if host == "" || path == "" {
				return ""
			}
			return "https://" + host + "/" + path
		}

		// Scheme-less host/owner/repo form
		if !strings.Contains(raw, "/") {
			return ""
		}
		return "https://" + strings.TrimPrefix(raw, "/")
	}

	u, err := url.Parse(raw)
	if err != nil || u.Host == "" {
		return ""
	}
	if u.Scheme != "http" && u.Scheme != "https" && u.Scheme != "ssh" {
		return ""
	}

	path := strings.TrimSuffix(strings.Trim(u.Path, "/"), ".git")
	if path == "" {
		return ""
	}
	if u.Scheme == "ssh" {
		return "https://" + u.Host + "/" + path
	}
	return u.Scheme + "://" + u.Host + "/" + path
}

// withoutEmbeddedCredentials removes the userinfo part (user:token@) from an
// HTTP(S) URL so that credentials embedded in a remote URL never reach the
// changelog.
func withoutEmbeddedCredentials(raw string) string {
	schemeEnd := strings.Index(raw, "://")
	if schemeEnd < 0 {
		return raw
	}
	if scheme := raw[:schemeEnd]; scheme != "http" && scheme != "https" {
		return raw
	}

	authority := raw[schemeEnd+3:]
	at := strings.IndexByte(authority, '@')
	if at < 0 {
		return raw
	}
	if slash := strings.IndexByte(authority, '/'); slash >= 0 && at > slash {
		return raw
	}

	return raw[:schemeEnd+3] + authority[at+1:]
}

func sortChangeLogRecords(records []string) {
	slices.SortStableFunc(records, func(a, b string) int {
		// skip '*' characters and compare by case-folded runes
		ai, bi := 0, 0
		for ai < len(a) && bi < len(b) {
			// advance to next non-asterisk rune in a
			if a[ai] == '*' {
				ai++
				continue
			}
			// advance to next non-asterisk rune in b
			if b[bi] == '*' {
				bi++
				continue
			}

			ra, sizeA := utf8.DecodeRuneInString(a[ai:])
			rb, sizeB := utf8.DecodeRuneInString(b[bi:])

			fa := unicode.ToLower(ra)
			fb := unicode.ToLower(rb)
			if fa != fb {
				return cmp.Compare(fa, fb)
			}

			ai += sizeA
			bi += sizeB
		}

		// skip remaining '*' in either string
		for ai < len(a) && a[ai] == '*' {
			ai++
		}
		for bi < len(b) && b[bi] == '*' {
			bi++
		}

		// shorter (remaining) string is less
		return cmp.Compare(len(a)-ai, len(b)-bi)
	})
}
