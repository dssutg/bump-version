package main

import (
	"slices"
	"strings"
	"testing"
)

const (
	releaseVer    = "1.2.3"
	prereleaseRC0 = "1.2.3-rc.0"
	prereleaseRC1 = "1.2.3-rc.1"
	vTagFormat    = "v{version}"
	relTagFormat  = "rel-{version}"
)

func mustParseVersion(t *testing.T, s string) Version {
	t.Helper()

	v, err := parseVersion(s)
	if err != nil {
		t.Fatalf("parseVersion(%q): %v", s, err)
	}
	return v
}

func TestParseVersionAcceptsValidVersions(t *testing.T) {
	t.Parallel()

	valid := []string{
		"0.0.0",
		"1.2.3",
		"10.20.30",
		"1.7.0-rc.1",
		"1.7.0-alpha",
		"1.0.0-0",
		"1.0.0-a-b",
		"1.0.0-rc.0.1",
		"1.0.0-alpha.beta.2",
	}

	for _, input := range valid {
		t.Run(input, func(t *testing.T) {
			t.Parallel()

			v, err := parseVersion(input)
			if err != nil {
				t.Fatalf("parseVersion(%q) returned error: %v", input, err)
			}
			if got := v.ToString(); got != input {
				t.Errorf("round trip = %q, want %q", got, input)
			}
		})
	}
}

func TestParseVersionRejectsInvalidVersions(t *testing.T) {
	t.Parallel()

	invalid := []struct {
		name  string
		input string
	}{
		{"empty", ""},
		{"one part", "1"},
		{"two parts", "1.2"},
		{"four parts", "1.2.3.4"},
		{"v prefix", "v0.4.1"},
		{"leading space", " 1.2.3"},
		{"trailing space", "1.2.3 "},
		{"leading zero major", "01.2.3"},
		{"leading zero minor", "1.02.3"},
		{"leading zero patch", "1.2.03"},
		{"negative major", "-1.2.3"},
		{"empty prerelease", "1.2.3-"},
		{"empty prerelease identifier", "1.2.3-rc..1"},
		{"build metadata on release", "1.2.3+build"},
		{"build metadata on prerelease", "1.2.3-rc.1+build"},
		{"non numeric part", "1.2.x"},
		{"space in part", "1.2.3 rc"},
		{"number out of range", "99999999999999999999.0.0"},
	}

	for _, tt := range invalid {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if _, err := parseVersion(tt.input); err == nil {
				t.Errorf("parseVersion(%q) returned no error, want an error", tt.input)
			}
		})
	}
}

func TestParseVersionRejectsBuildMetadataWithExplicitError(t *testing.T) {
	t.Parallel()

	if _, err := parseVersion("1.2.3+build"); err == nil ||
		!strings.Contains(err.Error(), "build metadata") {
		t.Errorf("parseVersion with build metadata: got error %v, want a build metadata error", err)
	}
}

func TestVersionToString(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		v    Version
		want string
	}{
		{"release", Version{Major: 2, Minor: 4, Patch: 6}, "2.4.6"},
		{"prerelease", Version{Major: 2, Minor: 4, Patch: 6, Prerelease: "rc.1"}, "2.4.6-rc.1"},
		{"zero", Version{}, "0.0.0"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := tt.v.ToString(); got != tt.want {
				t.Errorf("ToString() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestVersionCompare(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name        string
		left, right string
		want        int
	}{
		{"equal", "3.1.0", "3.1.0", 0},
		{"major less", "1.4.2", "5.0.0", -1},
		{"major greater", "5.0.1", "1.4.1", 1},
		{"minor less", "1.4.2", "1.5.0", -1},
		{"patch numeric order", "1.8.9", "1.8.10", -1},
		{"patch numeric order reversed", "1.8.10", "1.8.9", 1},
		{"prerelease below release", "1.0.0-rc.1", "1.0.0", -1},
		{"release above prerelease", "1.0.0", "1.0.0-rc.1", 1},
		{"numeric identifiers compare numerically", "1.0.0-rc.9", "1.0.0-rc.10", -1},
		{"longer prerelease list ranks higher", "2.3.4-rc", "2.3.4-rc.1", -1},
		{"numeric identifier ranks below alphanumeric", "1.0.0-1", "1.0.0-alpha", -1},
		{"alphanumeric identifiers compare lexically", "1.0.0-alpha", "1.0.0-beta", -1},
		{"mixed identifier kinds", "1.0.0-alpha.1", "1.0.0-alpha.beta", -1},
		{"equal prerelease", "4.0.0-rc.1", "4.0.0-rc.1", 0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			left := mustParseVersion(t, tt.left)
			right := mustParseVersion(t, tt.right)
			if got := left.Compare(right); got != tt.want {
				t.Errorf("%s.Compare(%s) = %d, want %d", tt.left, tt.right, got, tt.want)
			}
		})
	}
}

func TestVersionsSortByPrecedence(t *testing.T) {
	t.Parallel()

	input := []string{
		"2.1.0-rc.2",
		"2.1.0-rc.10",
		"2.1.0-rc",
		"2.1.0",
		"2.0.9",
		"2.1.0-rc.1",
	}
	want := []string{
		"2.0.9",
		"2.1.0-rc",
		"2.1.0-rc.1",
		"2.1.0-rc.2",
		"2.1.0-rc.10",
		"2.1.0",
	}

	versions := make([]Version, 0, len(input))
	for _, s := range input {
		versions = append(versions, mustParseVersion(t, s))
	}

	slices.SortFunc(versions, func(a, b Version) int { return a.Compare(b) })

	got := make([]string, len(versions))
	for i, v := range versions {
		got[i] = v.ToString()
	}

	if !slices.Equal(got, want) {
		t.Errorf("sorted versions = %v, want %v", got, want)
	}
}

func TestVersionIncrementTo(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name        string
		start       string
		releaseType ReleaseType
		want        string
	}{
		{"major", releaseVer, ReleaseTypeMajor, "2.0.0"},
		{"minor", releaseVer, ReleaseTypeMinor, "1.3.0"},
		{"patch", releaseVer, ReleaseTypePatch, "1.2.4"},
		{"prerelease starts at rc.0", releaseVer, ReleaseTypePrerelease, prereleaseRC0},
		{"prerelease continues rc counter", prereleaseRC0, ReleaseTypePrerelease, prereleaseRC1},
		{"prerelease after bare rc", "1.2.3-rc", ReleaseTypePrerelease, prereleaseRC0},
		{"prerelease after bare suffix", "1.2.3-alpha", ReleaseTypePrerelease, "1.2.3-alpha.0"},
		{"prerelease continues alpha counter", "1.2.3-alpha.0", ReleaseTypePrerelease, "1.2.3-alpha.1"},
		{"major clears prerelease", prereleaseRC1, ReleaseTypeMajor, "2.0.0"},
		{"minor clears prerelease", prereleaseRC1, ReleaseTypeMinor, "1.3.0"},
		{"patch clears prerelease", prereleaseRC1, ReleaseTypePatch, "1.2.4"},
		{"none keeps release", releaseVer, ReleaseTypeNone, releaseVer},
		{"none keeps prerelease", prereleaseRC1, ReleaseTypeNone, prereleaseRC1},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			v := mustParseVersion(t, tt.start)
			v.IncrementTo(tt.releaseType)
			if got := v.ToString(); got != tt.want {
				t.Errorf("IncrementTo(%s) on %s = %s, want %s", tt.releaseType, tt.start, got, tt.want)
			}
		})
	}
}

func TestParseReleaseType(t *testing.T) {
	t.Parallel()

	t.Run("accepts forcing values", func(t *testing.T) {
		t.Parallel()

		valid := map[string]ReleaseType{
			"major":      ReleaseTypeMajor,
			"minor":      ReleaseTypeMinor,
			"patch":      ReleaseTypePatch,
			"prerelease": ReleaseTypePrerelease,
		}

		for input, want := range valid {
			got, err := parseReleaseType(input)
			if err != nil {
				t.Errorf("parseReleaseType(%q) returned error: %v", input, err)
				continue
			}
			if got != want {
				t.Errorf("parseReleaseType(%q) = %q, want %q", input, got, want)
			}
		}
	})

	t.Run("rejects other values", func(t *testing.T) {
		t.Parallel()

		invalid := []string{"", "none", "Major", "hotfix", "bump"}

		for _, input := range invalid {
			if _, err := parseReleaseType(input); err == nil {
				t.Errorf("parseReleaseType(%q) returned no error, want an error", input)
			}
		}
	})
}

func TestVersionToGitTag(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		version Version
		format  string
		want    string
	}{
		{"v prefix", Version{Major: 1, Minor: 2, Patch: 3}, vTagFormat, "v1.2.3"},
		{"bare version", Version{Major: 1, Minor: 2, Patch: 3}, versionTagFormatSubst, releaseVer},
		{"custom prefix", Version{Major: 1, Minor: 2, Patch: 3}, relTagFormat, "rel-1.2.3"},
		{
			"prerelease version",
			Version{Major: 4, Minor: 0, Patch: 0, Prerelease: "alpha.1"},
			vTagFormat,
			"v4.0.0-alpha.1",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := versionToGitTag(tt.version, tt.format); got != tt.want {
				t.Errorf("versionToGitTag(%s, %q) = %q, want %q", tt.version.ToString(), tt.format, got, tt.want)
			}
		})
	}
}

func TestExtractVersionStringFromTag(t *testing.T) {
	t.Parallel()

	t.Run("v prefix", func(t *testing.T) {
		t.Parallel()

		got, err := extractVersionStringFromTag("v1.2.3", vTagFormat)
		if err != nil {
			t.Fatalf("extractVersionStringFromTag returned error: %v", err)
		}
		if got != releaseVer {
			t.Errorf("extractVersionStringFromTag = %q, want %q", got, releaseVer)
		}
	})

	t.Run("custom prefix", func(t *testing.T) {
		t.Parallel()

		got, err := extractVersionStringFromTag("rel-1.2.3", "rel-{version}")
		if err != nil {
			t.Fatalf("extractVersionStringFromTag returned error: %v", err)
		}
		if got != releaseVer {
			t.Errorf("extractVersionStringFromTag = %q, want %q", got, releaseVer)
		}
	})

	t.Run("missing prefix", func(t *testing.T) {
		t.Parallel()

		if _, err := extractVersionStringFromTag(releaseVer, vTagFormat); err == nil ||
			!strings.Contains(err.Error(), "must have format") {
			t.Errorf("extractVersionStringFromTag got error %v, want a format mismatch error", err)
		}
	})

	t.Run("wrong format", func(t *testing.T) {
		t.Parallel()

		if _, err := extractVersionStringFromTag("v9.9.9", "rel-{version}"); err == nil {
			t.Error("extractVersionStringFromTag with wrong format returned no error, want an error")
		}
	})
}
