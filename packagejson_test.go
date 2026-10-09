package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestReplaceJSONVersion(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		in   string
		want string
	}{
		{
			name: "replaces top-level version and keeps formatting",
			in:   "{\n  \"name\": \"app\",\n  \"version\": \"1.0.0\",\n  \"private\": true\n}\n",
			want: "{\n  \"name\": \"app\",\n  \"version\": \"1.1.0\",\n  \"private\": true\n}\n",
		},
		{
			name: "ignores nested version fields",
			in:   "{\n  \"version\": \"1.0.0\",\n  \"dependencies\": {\n    \"version\": \"2.0.0\"\n  }\n}",
			want: "{\n  \"version\": \"1.1.0\",\n  \"dependencies\": {\n    \"version\": \"2.0.0\"\n  }\n}",
		},
		{
			name: "handles version as the last field",
			in:   `{"name":"app","version":"1.0.0"}`,
			want: `{"name":"app","version":"1.1.0"}`,
		},
		{
			name: "ignores a non-key string equal to version",
			in:   `{"name": "version", "version": "1.0.0"}`,
			want: `{"name": "version", "version": "1.1.0"}`,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got, err := replaceJSONVersion([]byte(tt.in), "1.1.0")
			if err != nil {
				t.Fatalf("replaceJSONVersion() returned error: %v", err)
			}
			if string(got) != tt.want {
				t.Errorf("replaceJSONVersion() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestReplaceJSONVersionErrors(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		in      string
		wantErr string
	}{
		{"no version field", `{"name": "app"}`, `no top-level "version" field found`},
		{"non-string version", `{"version": 1}`, `is not a string`},
		{"top-level array", `["version"]`, `no top-level "version" field found`},
		{"truncated", `{"version":`, "unexpected end of JSON"},
		{"malformed", `{"version": "1.0.0",}`, "invalid character"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			_, err := replaceJSONVersion([]byte(tt.in), "1.1.0")
			if err == nil || !strings.Contains(err.Error(), tt.wantErr) {
				t.Errorf("replaceJSONVersion() error = %v, want containing %q", err, tt.wantErr)
			}
		})
	}
}

func TestUpdatePackageJSONVersionWritesFile(t *testing.T) {
	t.Parallel()

	path := filepath.Join(t.TempDir(), "package.json")
	if err := os.WriteFile(path, []byte("{\n  \"version\": \"1.0.0\"\n}\n"), 0o600); err != nil {
		t.Fatalf("could not write package.json: %v", err)
	}

	if err := updatePackageJSONVersion(path, "2.0.0"); err != nil {
		t.Fatalf("updatePackageJSONVersion() returned error: %v", err)
	}

	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("could not read package.json: %v", err)
	}
	if want := "{\n  \"version\": \"2.0.0\"\n}\n"; string(got) != want {
		t.Errorf("package.json = %q, want %q", got, want)
	}
}

func TestUpdatePackageJSONVersionMissingFile(t *testing.T) {
	t.Parallel()

	err := updatePackageJSONVersion(filepath.Join(t.TempDir(), "absent.json"), "1.0.0")
	if err == nil {
		t.Error("updatePackageJSONVersion() for a missing file = nil, want an error")
	}
}
