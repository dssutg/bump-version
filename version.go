package main

import (
	"cmp"
	"errors"
	"fmt"
	"strconv"
	"strings"
)

// Version represents a Semantic Versioning version: a numeric core with an
// optional prerelease suffix. An empty Prerelease means a final release.
type Version struct {
	Major, Minor, Patch int
	Prerelease          string
}

// ReleaseType describes how a version should be incremented.
type ReleaseType string

// Release types understood by the tool. ReleaseTypeNone means "no release".
const (
	ReleaseTypeMajor      ReleaseType = "major"
	ReleaseTypeMinor      ReleaseType = "minor"
	ReleaseTypePatch      ReleaseType = "patch"
	ReleaseTypePrerelease ReleaseType = "prerelease"
	ReleaseTypeNone       ReleaseType = "none"
)

// ToString renders the version in canonical X.Y.Z[-prerelease] form.
func (v *Version) ToString() string {
	s := fmt.Sprintf("%d.%d.%d", v.Major, v.Minor, v.Patch)
	if v.Prerelease != "" {
		s += "-" + v.Prerelease
	}
	return s
}

// Compare orders versions by Semantic Versioning precedence rules: it returns
// -1 if v sorts before other, 0 if they are equal, and 1 if v sorts after.
// A version without a prerelease always outranks one with a prerelease.
func (v *Version) Compare(other Version) int {
	if c := cmp.Compare(v.Major, other.Major); c != 0 {
		return c
	}
	if c := cmp.Compare(v.Minor, other.Minor); c != 0 {
		return c
	}
	if c := cmp.Compare(v.Patch, other.Patch); c != 0 {
		return c
	}

	switch {
	case v.Prerelease == "" && other.Prerelease == "":
		return 0
	case v.Prerelease == "":
		return 1
	case other.Prerelease == "":
		return -1
	default:
		return comparePrereleases(v.Prerelease, other.Prerelease)
	}
}

// IncrementTo advances the version according to the release type. Major,
// minor and patch increments reset the prerelease suffix; a prerelease
// increment starts a release at rc.0 and otherwise continues the existing
// suffix; ReleaseTypeNone leaves the version unchanged.
func (v *Version) IncrementTo(releaseType ReleaseType) {
	switch releaseType {
	case ReleaseTypeMajor:
		v.Major++
		v.Minor = 0
		v.Patch = 0
		v.Prerelease = ""
	case ReleaseTypeMinor:
		v.Minor++
		v.Patch = 0
		v.Prerelease = ""
	case ReleaseTypePatch:
		v.Patch++
		v.Prerelease = ""
	case ReleaseTypePrerelease:
		v.Prerelease = nextPrerelease(v.Prerelease)
	case ReleaseTypeNone:
		// Nothing to do: keep the version as-is.
	}
}

func (v *Version) Increment(hasBreakingChange, hasFeatures, hasFixes bool) {
	switch {
	case hasBreakingChange:
		v.Major++
		v.Minor = 0
		v.Patch = 0
	case hasFeatures:
		v.Minor++
		v.Patch = 0
	case hasFixes:
		v.Patch++
	}
}

// nextPrerelease computes the next prerelease suffix: an empty suffix starts
// at rc.0, a suffix without a dot gains .0, and a suffix ending in a numeric
// identifier has that identifier incremented (rc.1 becomes rc.2).
func nextPrerelease(current string) string {
	if current == "" {
		return "rc.0"
	}

	dot := strings.LastIndexByte(current, '.')
	if dot < 0 {
		return current + ".0"
	}

	counter := current[dot+1:]
	if !isNumericIdentifier(counter) {
		return current + ".0"
	}

	n, err := strconv.Atoi(counter)
	if err != nil {
		return current + ".0"
	}
	return current[:dot+1] + strconv.Itoa(n+1)
}

// parseReleaseType validates a forced release type as accepted on the command
// line. ReleaseTypeNone is deliberately not accepted here: "nothing to
// release" is decided from the commits instead.
func parseReleaseType(s string) (ReleaseType, error) {
	switch s {
	case string(ReleaseTypeMajor):
		return ReleaseTypeMajor, nil
	case string(ReleaseTypeMinor):
		return ReleaseTypeMinor, nil
	case string(ReleaseTypePatch):
		return ReleaseTypePatch, nil
	case string(ReleaseTypePrerelease):
		return ReleaseTypePrerelease, nil
	default:
		return "", fmt.Errorf("invalid release type %q: must be one of major, minor, patch, prerelease", s)
	}
}

// parseVersion parses a strict Semantic Versioning string: exactly three
// dot-separated numbers without leading zeros, an optional non-empty
// prerelease suffix, and no build metadata (the "+..." suffix is rejected).
func parseVersion(version string) (Version, error) {
	if strings.Contains(version, "+") {
		return Version{}, errors.New("build metadata is not supported")
	}

	core := version
	var prerelease string
	if i := strings.IndexByte(version, '-'); i >= 0 {
		core = version[:i]
		prerelease = version[i+1:]
		if err := validatePrerelease(prerelease); err != nil {
			return Version{}, err
		}
	}

	parts := strings.Split(core, ".")
	if len(parts) != 3 {
		return Version{}, errors.New("version must have three dot-separated parts")
	}

	numbers := make([]int, len(parts))
	for i, part := range parts {
		n, err := parseVersionNumber(part)
		if err != nil {
			return Version{}, err
		}
		numbers[i] = n
	}

	return Version{
		Major:      numbers[0],
		Minor:      numbers[1],
		Patch:      numbers[2],
		Prerelease: prerelease,
	}, nil
}

// parseVersionNumber parses one numeric core part: digits only, no leading
// zeros, and within the range of int.
func parseVersionNumber(s string) (int, error) {
	if s == "" {
		return 0, errors.New("version number must not be empty")
	}
	if len(s) > 1 && s[0] == '0' {
		return 0, fmt.Errorf("version number %q must not have leading zeros", s)
	}
	for i := range len(s) {
		if s[i] < '0' || s[i] > '9' {
			return 0, fmt.Errorf("version number %q must contain digits only", s)
		}
	}

	n, err := strconv.Atoi(s)
	if err != nil {
		return 0, fmt.Errorf("version number %q is out of range: %w", s, err)
	}
	return n, nil
}

// validatePrerelease checks that the prerelease part is non-empty and made of
// dot-separated identifiers of letters, digits and hyphens.
func validatePrerelease(prerelease string) error {
	if prerelease == "" {
		return errors.New("prerelease part must not be empty")
	}

	for _, identifier := range strings.Split(prerelease, ".") {
		if identifier == "" {
			return errors.New("prerelease identifiers must not be empty")
		}
		for i := range len(identifier) {
			c := identifier[i]
			switch {
			case c >= '0' && c <= '9', c >= 'a' && c <= 'z', c >= 'A' && c <= 'Z', c == '-':
			default:
				return fmt.Errorf("prerelease identifier %q contains invalid character %q", identifier, c)
			}
		}
	}
	return nil
}

// comparePrereleases compares two prerelease suffixes identifier by
// identifier; a longer suffix that shares a prefix ranks higher.
func comparePrereleases(a, b string) int {
	aParts := strings.Split(a, ".")
	bParts := strings.Split(b, ".")

	for i := range min(len(aParts), len(bParts)) {
		if c := comparePrereleaseIdentifiers(aParts[i], bParts[i]); c != 0 {
			return c
		}
	}
	return cmp.Compare(len(aParts), len(bParts))
}

// comparePrereleaseIdentifiers compares two prerelease identifiers: numeric
// identifiers compare numerically and rank below alphanumeric ones.
func comparePrereleaseIdentifiers(a, b string) int {
	aNumeric := isNumericIdentifier(a)
	bNumeric := isNumericIdentifier(b)

	switch {
	case aNumeric && bNumeric:
		return compareNumericIdentifiers(a, b)
	case aNumeric:
		return -1
	case bNumeric:
		return 1
	default:
		return strings.Compare(a, b)
	}
}

// compareNumericIdentifiers compares digit strings numerically, tolerating
// identifiers too large for int and identifiers with leading zeros.
func compareNumericIdentifiers(a, b string) int {
	a = strings.TrimLeft(a, "0")
	b = strings.TrimLeft(b, "0")
	if a == "" {
		a = "0"
	}
	if b == "" {
		b = "0"
	}

	if c := cmp.Compare(len(a), len(b)); c != 0 {
		return c
	}
	return strings.Compare(a, b)
}

func isNumericIdentifier(s string) bool {
	if s == "" {
		return false
	}
	for i := range len(s) {
		if s[i] < '0' || s[i] > '9' {
			return false
		}
	}
	return true
}

func versionToGitTag(version Version, tagFormat string) string {
	return strings.Replace(tagFormat, versionTagFormatSubst, version.ToString(), 1)
}

func getCurrentVersionTag() (string, error) {
	currentVersionTagGitOutput, errOut, err := getCommandOutput("git", "describe", "--tags", "--abbrev=0")
	if err != nil {
		if seemsNoTagsError(errOut) {
			return "", errors.New("no version tags found; mark the first version with `git tag v0.1.0`")
		}
		return "", fmt.Errorf("error getting current version from Git: %s", errOut)
	}

	currentVersionTag := strings.TrimSpace(currentVersionTagGitOutput)

	return currentVersionTag, nil
}

func seemsNoTagsError(errOut string) bool {
	errOutLower := strings.ToLower(errOut)
	subLower := "no names found, cannot describe anything"
	return strings.Contains(errOutLower, subLower)
}

func getCurrentVersionByGitTag(versionTagFormat string) (Version, error) {
	// Get current version tag from Git
	currentVersionTag, err := getCurrentVersionTag()
	if err != nil {
		return Version{}, err
	}

	// Get current version from current tag
	currentVersionString, err := extractVersionStringFromTag(currentVersionTag, versionTagFormat)
	if err != nil {
		return Version{}, fmt.Errorf("error extracting current version from Git tag: %w", err)
	}

	// Parse current version
	currentVersion, err := parseVersion(currentVersionString)
	if err != nil {
		return Version{}, fmt.Errorf("error parsing current version from Git tag: %w", err)
	}

	return currentVersion, nil
}

func extractVersionStringFromTag(currentVersionTag, versionTagFormat string) (string, error) {
	// Get index of the version placeholder
	before, after, ok := strings.Cut(versionTagFormat, versionTagFormatSubst)
	if !ok {
		panic("ExtractVersionStringFromTag assumes valid versionTagFormat")
	}

	// Get substrings before and after the placeholder
	prefix := before
	suffix := after

	if !strings.HasPrefix(currentVersionTag, prefix) ||
		!strings.HasSuffix(currentVersionTag, suffix) {
		return "", fmt.Errorf("version tag %q must have format %s", currentVersionTag, versionTagFormat)
	}

	// Get rid of both the prefix and suffix substrings to
	// extract the actual version number
	return currentVersionTag[len(prefix) : len(currentVersionTag)-len(suffix)], nil
}
