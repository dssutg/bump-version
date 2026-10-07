package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

const testQuestion = "Continue?"

func TestAskYesNo(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name       string
		input      string
		defaultVal bool
		want       bool
	}{
		{"y", "y\n", false, true},
		{"yes", "yes\n", false, true},
		{"n", "n\n", true, false},
		{"no", "no\n", true, false},
		{"upper case", "Y\n", false, true},
		{"mixed case", "Yes\n", false, true},
		{"padded response", "  y  \n", false, true},
		{"empty response uses default true", "\n", true, true},
		{"empty response uses default false", "\n", false, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			var out bytes.Buffer
			got := askYesNo(strings.NewReader(tt.input), &out, testQuestion, tt.defaultVal)
			if got != tt.want {
				t.Errorf("askYesNo(%q) = %v, want %v", tt.input, got, tt.want)
			}

			wantPrompt := testQuestion + " [y/N]: "
			if tt.defaultVal {
				wantPrompt = testQuestion + " [Y/n]: "
			}
			if !strings.Contains(out.String(), wantPrompt) {
				t.Errorf("prompt = %q, want it to contain %q", out.String(), wantPrompt)
			}
			if strings.Contains(out.String(), "\x1b") {
				t.Errorf("prompt for a buffer contains escape sequences: %q", out.String())
			}
		})
	}
}

func TestAskYesNoEOFReturnsDefault(t *testing.T) {
	t.Parallel()

	var out bytes.Buffer
	if got := askYesNo(strings.NewReader(""), &out, testQuestion, false); got {
		t.Error("askYesNo() on EOF = true, want the default false")
	}
	if !strings.Contains(out.String(), testQuestion) {
		t.Errorf("prompt = %q, want it to contain %q", out.String(), testQuestion)
	}
}

func TestAskYesNoRepromptsOnInvalidInput(t *testing.T) {
	t.Parallel()

	var out bytes.Buffer
	got := askYesNo(strings.NewReader("maybe\ny\n"), &out, testQuestion, false)
	if !got {
		t.Error("askYesNo() = false, want true after an invalid answer")
	}

	if !strings.Contains(out.String(), "Please answer yes or no (y/n).") {
		t.Errorf("output = %q, want the re-prompt notice", out.String())
	}
	if promptCount := strings.Count(out.String(), testQuestion); promptCount != 2 {
		t.Errorf("prompt shown %d times, want 2", promptCount)
	}
}

func TestFileExists(t *testing.T) {
	t.Parallel()

	path := filepath.Join(t.TempDir(), "file.txt")
	if fileExists(path) {
		t.Error("fileExists() = true for a missing file")
	}

	if err := os.WriteFile(path, []byte("x"), 0o600); err != nil {
		t.Fatalf("could not write file: %v", err)
	}
	if !fileExists(path) {
		t.Error("fileExists() = false for an existing file")
	}
}

func TestIsHexDigit(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		r    rune
		want bool
	}{
		{"hex zero", '0', true},
		{"nine", '9', true},
		{"lower a", 'a', true},
		{"lower f", 'f', true},
		{"upper A", 'A', true},
		{"upper F", 'F', true},
		{"lower g", 'g', false},
		{"upper G", 'G', false},
		{"space", ' ', false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := IsHexDigit(tt.r); got != tt.want {
				t.Errorf("IsHexDigit(%q) = %v, want %v", tt.r, got, tt.want)
			}
		})
	}
}

func TestIsUpperHexDigit(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		r    rune
		want bool
	}{
		{"upper zero", '0', true},
		{"upper accepts A", 'A', true},
		{"upper accepts F", 'F', true},
		{"upper rejects a", 'a', false},
		{"upper rejects f", 'f', false},
		{"upper space", ' ', false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := IsUpperHexDigit(tt.r); got != tt.want {
				t.Errorf("IsUpperHexDigit(%q) = %v, want %v", tt.r, got, tt.want)
			}
		})
	}
}

func TestIsLowerHexDigit(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		r    rune
		want bool
	}{
		{"lower zero", '0', true},
		{"lower accepts a", 'a', true},
		{"lower accepts f", 'f', true},
		{"lower rejects A", 'A', false},
		{"lower rejects F", 'F', false},
		{"lower space", ' ', false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := IsLowerHexDigit(tt.r); got != tt.want {
				t.Errorf("IsLowerHexDigit(%q) = %v, want %v", tt.r, got, tt.want)
			}
		})
	}
}

func TestHasLeadingWhitespace(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		s    string
		want bool
	}{
		{"empty string", "", false},
		{"space prefix", " x", true},
		{"tab prefix", "\tx", true},
		{"newline prefix", "\nx", true},
		{"no prefix", "x", false},
		{"non-breaking space prefix", "\u00a0x", true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := HasLeadingWhitespace(tt.s); got != tt.want {
				t.Errorf("HasLeadingWhitespace(%q) = %v, want %v", tt.s, got, tt.want)
			}
		})
	}
}

func TestHasTrailingWhitespace(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		s    string
		want bool
	}{
		{"empty string", "", false},
		{"space suffix", "x ", true},
		{"newline suffix", "x\n", true},
		{"no suffix", "x", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := HasTrailingWhitespace(tt.s); got != tt.want {
				t.Errorf("HasTrailingWhitespace(%q) = %v, want %v", tt.s, got, tt.want)
			}
		})
	}
}
