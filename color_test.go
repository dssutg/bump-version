package main

import (
	"bytes"
	"io"
	"os"
	"testing"
)

const (
	goosLinux        = "linux"
	goosWindows      = "windows"
	envNoColor       = "NO_COLOR"
	envClicolorForce = "CLICOLOR_FORCE"
	colorTestMsg     = "msg"
)

func TestSupportsColor(t *testing.T) {
	t.Parallel()

	charDevice, err := os.OpenFile(os.DevNull, os.O_WRONLY, 0)
	if err != nil {
		t.Fatalf("open %s: %v", os.DevNull, err)
	}
	t.Cleanup(func() { _ = charDevice.Close() })

	regularFile, err := os.CreateTemp(t.TempDir(), "out")
	if err != nil {
		t.Fatalf("CreateTemp: %v", err)
	}
	t.Cleanup(func() { _ = regularFile.Close() })

	tests := []struct {
		name string
		w    io.Writer
		env  map[string]string
		goos string
		want bool
	}{
		{name: "char device", w: charDevice, env: nil, goos: goosLinux, want: true},
		{name: "char device with NO_COLOR", w: charDevice, env: map[string]string{envNoColor: "1"}, goos: goosLinux, want: false},
		{name: "char device with empty NO_COLOR", w: charDevice, env: map[string]string{envNoColor: ""}, goos: goosLinux, want: true},
		{name: "char device with dumb TERM", w: charDevice, env: map[string]string{"TERM": "dumb"}, goos: goosLinux, want: false},
		{name: "regular file", w: regularFile, env: nil, goos: goosLinux, want: false},
		{name: "buffer", w: &bytes.Buffer{}, env: nil, goos: goosLinux, want: false},
		{name: "windows", w: charDevice, env: nil, goos: goosWindows, want: false},
		{name: "CLICOLOR_FORCE forces a buffer", w: &bytes.Buffer{}, env: map[string]string{envClicolorForce: "1"}, goos: goosLinux, want: true},
		{name: "CLICOLOR_FORCE overrides NO_COLOR", w: &bytes.Buffer{}, env: map[string]string{envClicolorForce: "1", envNoColor: "1"}, goos: goosLinux, want: true},
		{name: "CLICOLOR_FORCE on Windows", w: &bytes.Buffer{}, env: map[string]string{envClicolorForce: "1"}, goos: goosWindows, want: true},
		{name: "CLICOLOR_FORCE 0 does not force", w: &bytes.Buffer{}, env: map[string]string{envClicolorForce: "0"}, goos: goosLinux, want: false},
		{name: "CLICOLOR_FORCE empty does not force", w: &bytes.Buffer{}, env: map[string]string{envClicolorForce: ""}, goos: goosLinux, want: false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			getenv := func(key string) string { return tt.env[key] }
			if got := supportsColor(tt.w, getenv, tt.goos); got != tt.want {
				t.Errorf("supportsColor() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestPaint(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name    string
		enabled bool
		code    string
		s       string
		want    string
	}{
		{name: "enabled", enabled: true, code: ansiRed, s: colorTestMsg, want: "\x1b[31mmsg\x1b[0m"},
		{name: "disabled", enabled: false, code: ansiRed, s: colorTestMsg, want: colorTestMsg},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if got := paint(tt.enabled, tt.code, tt.s); got != tt.want {
				t.Errorf("paint() = %q, want %q", got, tt.want)
			}
		})
	}
}
