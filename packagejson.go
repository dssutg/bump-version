package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
)

// packageJSONVersionField is the top-level package.json field that holds the
// project version.
const packageJSONVersionField = "version"

// updatePackageJSONVersion sets the top-level "version" field of the JSON file
// at filename to newVersion. Everything else in the file, including its
// formatting, is preserved.
func updatePackageJSONVersion(filename, newVersion string) error {
	//nolint:gosec // the file name comes from this project's own config
	data, err := os.ReadFile(filename)
	if err != nil {
		return fmt.Errorf("could not read %s: %w", filename, err)
	}

	updated, err := replaceJSONVersion(data, newVersion)
	if err != nil {
		return fmt.Errorf("could not update the version in %s: %w", filename, err)
	}

	//nolint:gosec // the file name comes from this project's own config
	if err := os.WriteFile(filename, updated, 0o600); err != nil {
		return fmt.Errorf("could not write %s: %w", filename, err)
	}

	return nil
}

// replaceJSONVersion returns data with the value of its top-level "version"
// string field replaced with newVersion. It fails when the document has no
// top-level "version" string field.
func replaceJSONVersion(data []byte, newVersion string) ([]byte, error) {
	dec := json.NewDecoder(bytes.NewReader(data))

	var (
		depth      int
		expectKey  bool
		pending    bool  // the next token is the value of the top-level "version" key
		found      bool  // a top-level "version" field has been seen
		valueStart int64 // byte index of the opening quote of the version value
		valueEnd   int64 // byte index just past the version value
	)

	for {
		tokenStart := dec.InputOffset()
		tok, err := dec.Token()
		if errors.Is(err, io.EOF) {
			break
		}
		if err != nil {
			return nil, err
		}

		if pending {
			if _, ok := tok.(string); !ok {
				return nil, errors.New(`the top-level "version" field is not a string`)
			}
			quote := bytes.IndexByte(data[tokenStart:], '"')
			if quote < 0 {
				return nil, errors.New(`could not locate the top-level "version" value`)
			}
			valueStart = tokenStart + int64(quote)
			valueEnd = dec.InputOffset()
			pending = false
			found = true
			expectKey = true
			continue
		}

		switch t := tok.(type) {
		case json.Delim:
			switch t {
			case '{', '[':
				depth++
				if depth == 1 {
					expectKey = t == '{'
				}
			case '}', ']':
				depth--
				if depth == 1 {
					expectKey = true
				}
			}
		case string:
			if depth == 1 {
				if expectKey {
					expectKey = false
					pending = t == packageJSONVersionField
				} else {
					expectKey = true
				}
			}
		default:
			if depth == 1 {
				expectKey = true
			}
		}
	}

	if depth != 0 {
		return nil, errors.New("unexpected end of JSON")
	}
	if !found {
		return nil, errors.New(`no top-level "version" field found`)
	}

	updated := make([]byte, 0, len(data)-int(valueEnd-valueStart)+len(newVersion)+2)
	updated = append(updated, data[:valueStart]...)
	updated = append(updated, '"')
	updated = append(updated, newVersion...)
	updated = append(updated, '"')
	updated = append(updated, data[valueEnd:]...)
	return updated, nil
}
