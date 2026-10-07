package main

import (
	"errors"
	"fmt"
	"os"
	"slices"
	"strings"
	"unicode"
	"unicode/utf8"
)

const (
	CommitKindFeat     = "feat"
	CommitKindFix      = "fix"
	CommitKindPerf     = "perf"
	CommitKindRefactor = "refactor"
	CommitKindDocs     = "docs"
	CommitKindTest     = "test"
	CommitKindBuild    = "build"
	CommitKindCI       = "ci"
	CommitKindStyle    = "style"
	CommitKindChore    = "chore"
	CommitKindRevert   = "revert"
)

var ErrFoundInvalidCommits = errors.New("found invalid commits")

type Commit struct {
	Hash        string
	Kind        string
	Scope       string
	Description string
	Body        string
	Footer      string
	Breaking    bool
	RefersTo    []string // commit hashes this commit refers to
}

func (c *Commit) ShortHash() string {
	return ensureShortCommitHash(c.Hash)
}

type CommitStats struct {
	// ByKind groups the live commits of the release window by their
	// Conventional Commits kind. Revert commits are not included there:
	// reverts only feed the "Reverted Changes" changelog section.
	ByKind            map[string][]Commit
	HasBreakingChange bool

	// InWindowCommits is the number of live commits since the release tag.
	// It lets a forced release type refuse an empty release window.
	InWindowCommits int

	// Commits from before the tag that got
	// reverted by reverts since the tag
	RevertedPreTag []Commit
}

// DetermineReleaseType derives the release type from the collected commits:
// any breaking change makes a major release, any feat makes a minor one, any
// fix or perf makes a patch, and anything else means nothing to release.
func (s CommitStats) DetermineReleaseType() ReleaseType {
	switch {
	case s.HasBreakingChange:
		return ReleaseTypeMajor
	case len(s.ByKind[CommitKindFeat]) > 0:
		return ReleaseTypeMinor
	case len(s.ByKind[CommitKindFix]) > 0, len(s.ByKind[CommitKindPerf]) > 0:
		return ReleaseTypePatch
	default:
		return ReleaseTypeNone
	}
}

func processCommitMessages(sinceTag string, ignoreInvalidCommits bool, allowedKinds []string) (CommitStats, error) {
	allLines, err := getGitCommitLines("") // full history newest->oldest
	if err != nil {
		return CommitStats{}, err
	}

	hasInvalidCommits := false

	// parse into chronological order (oldest first)
	var parsed []Commit
	for i := len(allLines) - 1; i >= 0; i-- {
		msg := allLines[i]
		c, err := ParseConventionalCommit(msg, true, allowedKinds)
		if err != nil {
			if sinceTag == "" {
				hasInvalidCommits = true
				logInvalidCommit(ignoreInvalidCommits, msg, err)
			}
			continue
		}
		parsed = append(parsed, c)
	}

	if hasInvalidCommits && !ignoreInvalidCommits {
		return CommitStats{}, ErrFoundInvalidCommits
	}

	// map commits by short hash and record their chronological index and
	// whether they are pre-tag
	byRef := make(map[string]Commit, len(parsed))
	indexOf := make(map[string]int, len(parsed))

	// true if commit is before the sinceTag boundary
	preTag := make(map[string]bool, len(parsed))

	// Build set of commits that are "sinceTag..HEAD"
	sinceSet := make(map[string]bool)
	if sinceTag != "" {
		sinceLines, err := getGitCommitLines(sinceTag)
		if err != nil {
			return CommitStats{}, err
		}

		hasInvalidCommits := false

		for _, msg := range sinceLines {
			c, err := ParseConventionalCommit(msg, true, allowedKinds)
			if err != nil {
				hasInvalidCommits = true
				logInvalidCommit(ignoreInvalidCommits, msg, err)
			} else {
				sinceSet[c.ShortHash()] = true
			}
		}

		if hasInvalidCommits && !ignoreInvalidCommits {
			return CommitStats{}, ErrFoundInvalidCommits
		}
	} else {
		// no tag -> everything treated as in-window
		for _, c := range parsed {
			sinceSet[c.ShortHash()] = true
		}
	}

	for i, c := range parsed {
		ref := c.ShortHash()
		byRef[ref] = c
		indexOf[ref] = i
		// consider a commit pre-tag if it's NOT in sinceSet
		preTag[ref] = !sinceSet[ref]
	}

	// Walk chronologically, toggling reverted state.
	reverted := make(map[string]bool)

	// track commits from before the tag that get reverted by
	// reverts seen since the tag
	revertedPreTagSet := make(map[string]bool)

	for _, c := range parsed {
		cRef := c.ShortHash()

		if c.Kind == CommitKindRevert {
			// for each referenced commit, flip reverted state
			for _, ref := range c.RefersTo {
				ref = ensureShortCommitHash(ref)

				if reverted[ref] {
					delete(reverted, ref)

					// if this is a revert-of-revert that restores a pre-tag commit,
					// remove it from revertedPreTagSet
					if preTag[ref] {
						delete(revertedPreTagSet, ref)
					}
				} else {
					reverted[ref] = true

					// if this revert commit is in-window (sinceTag) and
					// it reverts a pre-tag commit, record it
					if sinceSet[cRef] && preTag[ref] {
						revertedPreTagSet[ref] = true
					}
				}
			}

			continue
		}

		// Non-revert commits: nothing to do during the walk besides
		// tracking existence (already done)
	}

	// Build final CommitStats including detection of pre-tag commits
	// reverted by in-window reverts
	commitStats := CommitStats{
		ByKind:         make(map[string][]Commit),
		RevertedPreTag: make([]Commit, 0, 64),
	}

	for short, c := range byRef {
		// include only commits in the requested window
		if !sinceSet[short] {
			continue
		}

		// skip commits that are currently reverted
		if reverted[short] {
			continue
		}

		commitStats.InWindowCommits++

		if c.Breaking {
			commitStats.HasBreakingChange = true
		}

		// Revert commits are rendered by the dedicated
		// "Reverted Changes" section, not by kind sections
		if c.Kind == CommitKindRevert {
			continue
		}

		commitStats.ByKind[c.Kind] = append(commitStats.ByKind[c.Kind], c)
	}

	// Populate RevertedPreTag slice from the set
	for ref := range revertedPreTagSet {
		if c, ok := byRef[ref]; ok {
			commitStats.RevertedPreTag = append(commitStats.RevertedPreTag, c)
			if c.Breaking {
				commitStats.HasBreakingChange = true
			}
		} else if !ignoreInvalidCommits {
			fmt.Fprintf(os.Stderr,
				"%s: WARNING: commit with hash %s was not added to the list of reverted commits! It is most likely an invalid commit or a non-existent hash.\n",
				programName, ref,
			)
		}
	}

	return commitStats, nil
}

func logInvalidCommit(ignoreInvalidCommits bool, commitMessage string, err error) {
	if ignoreInvalidCommits {
		fmt.Fprintf(os.Stderr,
			"%s: WARNING: ignored invalid commit message: %s. Reason: %v\n",
			programName, commitMessage, err,
		)
	} else {
		fmt.Fprintf(os.Stderr,
			"%s: ERROR: invalid commit message: %s. Reason: %v\n",
			programName, commitMessage, err,
		)
	}
}

func getGitCommitLines(sinceTag string) ([]string, error) {
	var args []string
	if sinceTag != "" {
		args = []string{"log", sinceTag + "..HEAD", "--pretty=format:%H %B\x1e", "--abbrev=false"}
	} else {
		args = []string{"log", "--pretty=format:%H %B\x1e", "--abbrev=false"}
	}

	// Get all commits since the provided tag
	gitCommitsOut, errOut, err := getCommandOutput(gitCommand, args...)
	if err != nil {
		return nil, fmt.Errorf("error getting commits from Git: %s", errOut)
	}

	rawGitCommits := strings.Split(gitCommitsOut, "\x1e")

	commits := make([]string, 0, len(rawGitCommits))

	// Trim each record and skip empty ones
	for _, record := range rawGitCommits {
		message := strings.TrimSpace(record)
		if message != "" {
			commits = append(commits, message)
		}
	}

	return commits, nil
}

func ParseConventionalCommit(message string, hasHash bool, allowedKinds []string) (Commit, error) {
	if strings.TrimSpace(message) == "" {
		return Commit{}, errors.New("empty commit message")
	}

	// Split into lines preserving order
	lines := strings.Split(message, "\n")
	for lineIndex, line := range lines {
		lineNo := lineIndex + 1
		if HasLeadingWhitespace(line) || HasTrailingWhitespace(line) {
			return Commit{}, fmt.Errorf("commit message line number %d (`%s`) should not have have leading or trailing whitespace", lineNo, line)
		}
	}

	// Check for empty lines at the end of the commit message
	for len(lines) >= 2 && strings.TrimSpace(lines[len(lines)-1]) == "" {
		return Commit{}, errors.New("commit message should not have empty lines at the end of it")
	}

	commit := Commit{}

	header := lines[0]
	if HasLeadingWhitespace(header) {
		return Commit{}, errors.New("commit message header should not have leading whitespace")
	}
	if HasTrailingWhitespace(header) {
		return Commit{}, errors.New("commit message header should not have trailing whitespace")
	}
	if header == "" {
		return Commit{}, errors.New("empty commit message header")
	}

	// Parse optional leading hash
	if hasHash {
		parts := strings.Fields(header)

		hash, err := parseCommitHash(parts[0])
		if err != nil {
			return Commit{}, err
		}
		commit.Hash = hash

		if len(header) <= len(commit.Hash) || header[len(commit.Hash)] != ' ' {
			return Commit{}, errors.New("hash must be followed by a single space")
		}

		// reconstruct the header without hash and the single space after it
		header = header[len(commit.Hash)+1:]
	}

	// Check for leading spaces again
	if HasLeadingWhitespace(header) {
		return Commit{}, errors.New("commit message header should not have leading whitespace")
	}

	// Find the first ":" that separates header from description
	before, after, ok := strings.Cut(header, ":")
	if !ok {
		return Commit{}, errors.New("commit message header must have colon to separate commit kind and description")
	}

	// Parse and validate kind (stage 1)
	headerBeforeDescription := before
	if HasLeadingWhitespace(headerBeforeDescription) {
		return Commit{}, errors.New("commit message kind should not have leading whitespace")
	}
	if HasTrailingWhitespace(headerBeforeDescription) {
		return Commit{}, errors.New("commit message kind should not have trailing whitespace before colon")
	}
	breaking := false
	if strings.HasSuffix(headerBeforeDescription, "!") {
		breaking = true
		// Get rid of !
		headerBeforeDescription = strings.TrimSuffix(headerBeforeDescription, "!")
		// Check for trailing spaces
		if HasTrailingWhitespace(headerBeforeDescription) {
			return Commit{}, errors.New("commit message kind should not have whitespace before `!'")
		}
	}

	// Extract kind and optional scope (stage 2)
	var kind string
	scope := ""
	if start := strings.Index(headerBeforeDescription, "("); start != -1 {
		// attempt to find matching ')'
		end := strings.LastIndex(headerBeforeDescription, ")")
		if end == -1 || end <= start {
			return Commit{}, errors.New("commit message kind must have matching parentheses")
		}

		kind = headerBeforeDescription[:start]
		if HasTrailingWhitespace(kind) {
			return Commit{}, errors.New("commit message kind should not have spaces before `('")
		}

		scope = headerBeforeDescription[start+1 : end]
		if HasLeadingWhitespace(scope) || HasTrailingWhitespace(scope) {
			return Commit{}, errors.New("commit message scope should not be enclosed with spaces")
		}

		if scope == "" {
			return Commit{}, errors.New("commit message scope cannot be empty within parentheses")
		}
	} else {
		kind = headerBeforeDescription
	}
	if kind == "" {
		return Commit{}, errors.New("commit message header should have non-empty kind")
	}
	commit.Kind = kind
	commit.Scope = scope
	commit.Breaking = breaking

	// Parse and validate description
	description := after
	if !strings.HasPrefix(description, " ") {
		return Commit{}, errors.New("commit message header must have space after colon")
	}
	description = description[1:] // skip one space
	if HasLeadingWhitespace(description) {
		return Commit{}, errors.New("commit message description should have only one whitespace after colon")
	}
	if HasTrailingWhitespace(headerBeforeDescription) {
		return Commit{}, errors.New("commit message description should not have trailing whitespace")
	}
	if description == "" {
		return Commit{}, errors.New("commit message header should have non-empty description")
	}
	if strings.HasSuffix(description, ".") {
		return Commit{}, errors.New("commit message description should not end with period")
	}
	if r, _ := utf8.DecodeRuneInString(description); unicode.IsUpper(r) {
		return Commit{}, errors.New("commit message description should not start with a capital letter")
	}
	const maxDescriptionCharacters = 120
	if utf8.RuneCountInString(description) > maxDescriptionCharacters {
		return Commit{}, fmt.Errorf("commit message description should not be more than %d characters", maxDescriptionCharacters)
	}
	commit.Description = description

	// Validate kind against allowedKinds
	if !slices.Contains(allowedKinds, commit.Kind) {
		return Commit{}, fmt.Errorf("commit message kind %s not allowed", commit.Kind)
	}

	// The remainder of the message (lines[1:]) is body and footer separated by a blank line.
	bodyLines := []string{}
	footerLines := []string{}
	if len(lines) > 1 {
		// Skip possible empty line immediately after header
		i := 1

		// consume leading blank lines between header and body
		for i < len(lines) && strings.TrimSpace(lines[i]) == "" {
			i++
		}

		// Collect lines until a blank line that separates body and footer
		for ; i < len(lines); i++ {
			// find first blank line that will separate body from footer
			if strings.TrimSpace(lines[i]) == "" {
				// everything after the blank line (skipping blank lines) is footer
				j := i + 1
				for j < len(lines) && strings.TrimSpace(lines[j]) == "" {
					j++
				}
				if j < len(lines) {
					// footer exists
					bodyLines = append(bodyLines, lines[1:i]...)
					footerLines = append(footerLines, lines[j:]...)
				} else {
					// trailing blank lines only; body is lines[1:i]
					bodyLines = append(bodyLines, lines[1:i]...)
				}
				break
			}
		}

		if len(bodyLines) == 0 && len(footerLines) == 0 {
			// no blank separator found; everything after header is body
			bodyLines = append(bodyLines, lines[1:]...)
		}
	}

	commit.Body = strings.TrimSpace(strings.Join(bodyLines, "\n"))
	commit.Footer = strings.TrimSpace(strings.Join(footerLines, "\n"))

	// If footer mentions BREAKING CHANGE, mark breaking
	if !commit.Breaking && strings.Contains(commit.Footer, "BREAKING CHANGE") {
		commit.Breaking = true
	}

	// Parse references from body
	for _, line := range bodyLines {
		if err := tryParseReference(line, &commit); err != nil {
			return Commit{}, err
		}
	}

	// Parse references from footer
	for _, line := range footerLines {
		if err := tryParseReference(line, &commit); err != nil {
			return Commit{}, err
		}
	}

	// Make sure revert commits have at least one reference to other commits
	if commit.Kind == CommitKindRevert && len(commit.RefersTo) == 0 {
		return Commit{}, errors.New("revert commits must have hashes of the commits they revert; add `Refs: ` attribute to commit footer")
	}

	// Check duplicate references
	if len(commit.RefersTo) != 0 {
		alreadySeenRefs := make(map[string]bool, len(commit.RefersTo))
		for _, ref := range commit.RefersTo {
			if alreadySeenRefs[ref] {
				return Commit{}, fmt.Errorf("commit message has duplicated ref hash %s", ref)
			}
			alreadySeenRefs[ref] = true
		}
	}

	return commit, nil
}

func warnNoCommitsThatCanIncrementCurrentVersion() {
	fmt.Fprintln(os.Stderr, programName+": no commits that can increment current version")
}

func parseCommitHash(hash string) (string, error) {
	if len(hash) != 7 && len(hash) != 40 {
		return "", errors.New("commit hash length must be either 7 or 40 characters long")
	}

	for _, digit := range hash {
		if !IsLowerHexDigit(digit) {
			return "", errors.New("commit message expected to have lowercase hexadecimal digits only")
		}
	}

	return hash, nil
}

func ensureShortCommitHash(hash string) string {
	if len(hash) > 7 {
		return hash[:7]
	}
	return hash
}

func tryParseReference(line string, commit *Commit) error {
	line = strings.TrimSpace(line)
	if line == "" {
		return nil
	}

	colonIndex := strings.Index(line, ":")
	if colonIndex == -1 {
		return nil
	}

	if HasLeadingWhitespace(line) || HasTrailingWhitespace(line) {
		return fmt.Errorf("footer line attribute should not be enclosed with spaces: %s", line)
	}

	// Parse attribute name
	attrName := line[:colonIndex]
	if HasTrailingWhitespace(attrName) {
		return fmt.Errorf("footer line attribute should not have spaces before colon: %s", line)
	}

	// Parse attribute value
	attrValue := line[colonIndex+1:]
	if !strings.HasPrefix(attrValue, " ") {
		return fmt.Errorf("footer line attribute value should not have only one space after colon: %s", line)
	}
	attrValue = attrValue[1:] // skip leading space
	if HasLeadingWhitespace(attrValue) {
		return fmt.Errorf("footer line attribute value should not have spaces after colon with spaces: %s", line)
	}

	// Dispatch attribyte based on its name
	refsAttr := "Refs"
	if strings.EqualFold(attrName, refsAttr) {
		if attrName != refsAttr {
			return fmt.Errorf("attribute %s should have consistent case like this: %s", attrName, refsAttr)
		}

		hashListParts := strings.Split(attrValue, ",")
		hashes := make([]string, 0, len(hashListParts))
		for hashIndex, hashStr := range hashListParts {
			if hashIndex != 0 {
				if !strings.HasPrefix(hashStr, " ") {
					return fmt.Errorf("hash list in attribute %s should be delimited by comma and a single space: %s", attrName, hashStr)
				}
				hashStr = hashStr[1:] // skip space
			}
			if HasLeadingWhitespace(hashStr) || HasTrailingWhitespace(hashStr) {
				return fmt.Errorf("hash list in attribute %s should be delimited by comma and a single space: %s", attrName, hashStr)
			}

			hash, err := parseCommitHash(hashStr)
			if err != nil {
				return fmt.Errorf("invalid hash %s: %w", hashStr, err)
			}

			hashes = append(hashes, hash)
		}

		commit.RefersTo = append(commit.RefersTo, hashes...)
	}

	return nil
}
