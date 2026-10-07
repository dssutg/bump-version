package main

import (
	"io"
	"os"
	"runtime"
)

// ANSI SGR escape sequences used for terminal output.
const (
	ansiReset  = "\x1b[0m"
	ansiRed    = "\x1b[31m"
	ansiGreen  = "\x1b[32m"
	ansiYellow = "\x1b[33m"
	ansiBold   = "\x1b[1m"
)

// supportsColor reports whether ANSI color escapes may be written to w.
// CLICOLOR_FORCE (non-empty, not "0") forces colors on anywhere; otherwise
// colors require a character device, no non-empty NO_COLOR, TERM different
// from "dumb" and a non-Windows system.
func supportsColor(w io.Writer, getenv func(string) string, goos string) bool {
	if force := getenv("CLICOLOR_FORCE"); force != "" && force != "0" {
		return true
	}

	if getenv("NO_COLOR") != "" {
		return false
	}
	if getenv("TERM") == "dumb" {
		return false
	}
	if goos == "windows" {
		return false
	}

	f, ok := w.(*os.File)
	if !ok {
		return false
	}
	info, err := f.Stat()
	if err != nil {
		return false
	}
	return info.Mode()&os.ModeCharDevice != 0
}

// colorEnabled reports whether ANSI colors are enabled for the given writer
// in the current process environment and platform.
func colorEnabled(w io.Writer) bool {
	return supportsColor(w, os.Getenv, runtime.GOOS)
}

// paint wraps s in the given SGR escape code when colors are enabled, and
// returns s unchanged otherwise.
func paint(enabled bool, code, s string) string {
	if !enabled {
		return s
	}
	return code + s + ansiReset
}
