package main

import (
	"bufio"
	"bytes"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"
	"unicode"
	"unicode/utf8"
)

// askYesNo prompts the user with the given question and returns true for "yes" and false for "no".
// It accepts "y", "yes", "n", "no" (any case). If the reader hits EOF, it returns the provided defaultVal.
func askYesNo(r io.Reader, w io.Writer, question string, defaultVal bool) bool {
	reader := bufio.NewReader(r)

	for {
		defaultPrompt := "y/N"
		if defaultVal {
			defaultPrompt = "Y/n"
		}

		fmt.Fprintf(w, "%s [%s]: ", question, defaultPrompt)
		line, err := reader.ReadString('\n')
		if err != nil {
			// on EOF or other read error, return default
			return defaultVal
		}

		response := strings.TrimSpace(strings.ToLower(line))
		if response == "" {
			return defaultVal
		}

		switch response {
		case "y", "yes":
			return true
		case "n", "no":
			return false
		default:
			fmt.Fprintln(w, "Please answer yes or no (y/n).")
		}
	}
}

func getCommandOutput(name string, arg ...string) (out string, errOut string, err error) {
	var outBuf bytes.Buffer
	var errBuf bytes.Buffer

	//nolint:gosec // the command and its arguments come from this program's own call sites
	cmd := exec.Command(name, arg...)

	cmd.Stdin = os.Stdin
	cmd.Stdout = &outBuf
	cmd.Stderr = &errBuf

	err = cmd.Run()

	out = outBuf.String()
	errOut = errBuf.String()

	return
}

func fatalOnErr2[T any](value T, err error) T {
	fatalOnErr(err)
	return value
}

func fatalOnErr(err error) {
	if err != nil {
		fatalf("%v", err)
	}
}

func fatalf(format string, a ...any) {
	fmt.Fprintf(os.Stderr, programName+": "+format+"\n", a...)
	os.Exit(ExitFailure)
}

func fileExists(path string) bool {
	//nolint:gosec // the path comes from this program's own config and constants
	_, err := os.Stat(path)
	if err == nil {
		return true
	}
	if os.IsNotExist(err) {
		return false
	}
	return true
}

func IsHexDigit(r rune) bool {
	return (r >= '0' && r <= '9') || (r >= 'a' && r <= 'f') || (r >= 'A' && r <= 'F')
}

func IsUpperHexDigit(r rune) bool {
	return (r >= '0' && r <= '9') || (r >= 'A' && r <= 'F')
}

func IsLowerHexDigit(r rune) bool {
	return (r >= '0' && r <= '9') || (r >= 'a' && r <= 'f')
}

// HasLeadingWhitespace reports whether s begins with a Unicode whitespace rune.
func HasLeadingWhitespace(s string) bool {
	if s == "" {
		return false
	}
	r, _ := utf8.DecodeRuneInString(s)
	return unicode.IsSpace(r)
}

// HasTrailingWhitespace reports whether s ends with a Unicode whitespace rune.
func HasTrailingWhitespace(s string) bool {
	if s == "" {
		return false
	}
	r, _ := utf8.DecodeLastRuneInString(s)
	return unicode.IsSpace(r)
}
