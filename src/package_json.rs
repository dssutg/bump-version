//! The optional package.json bump: replaces only the top-level "version"
//! string so the rest of the document, including its exact formatting,
//! stays untouched.

use std::path::Path;

use crate::config::write_file_with_mode;
use crate::error::{Error, Result};

/// The top-level package.json field that holds the project version.
const PACKAGE_JSON_VERSION_FIELD: &str = "version";

/// update_package_json_version sets the top-level "version" field of the JSON
/// file at filename to new_version. Everything else in the file, including
/// its formatting, is preserved.
pub fn update_package_json_version(filename: &Path, new_version: &str) -> Result<()> {
    let data = std::fs::read_to_string(filename)
        .map_err(|err| Error::msg(format!("could not read {}: {err}", filename.display())))?;

    let updated = replace_json_version(&data, new_version).map_err(|err| {
        Error::msg(format!(
            "could not update the version in {}: {err}",
            filename.display()
        ))
    })?;

    write_file_with_mode(filename, updated.as_bytes(), 0o600)
        .map_err(|err| Error::msg(format!("could not write {}: {err}", filename.display())))?;

    Ok(())
}

/// replace_json_version returns data with the value of its top-level "version"
/// string field replaced with new_version. It fails when the document has no
/// top-level "version" string field.
///
/// serde_json validates the document and its semantics (object root, string
/// "version" field). Since serde_json has no byte offsets and re-serializing
/// would reformat the file, the replacement value is then located in the raw
/// text with a small scanner and spliced in, preserving every other byte of
/// the input.
pub fn replace_json_version(data: &str, new_version: &str) -> Result<String> {
    // serde_json checks the whole document upfront...
    let root: serde_json::Value =
        serde_json::from_str(data).map_err(|err| Error::msg(err.to_string()))?;

    // ...answers the semantic questions...
    let object = match root.as_object() {
        Some(object) => object,
        None => return Err(Error::msg(r#"no top-level "version" field found"#)),
    };
    match object.get(PACKAGE_JSON_VERSION_FIELD) {
        Some(serde_json::Value::String(_)) => {}
        Some(_) => {
            return Err(Error::msg(
                r#"the top-level "version" field is not a string"#,
            ));
        }
        None => return Err(Error::msg(r#"no top-level "version" field found"#)),
    }

    // ...and a small scanner locates the value's byte span so the splice can
    // keep the original formatting of everything around it.
    let span = find_top_level_version_value_span(data)?;
    let value_start = span.0;
    let value_end = span.1;

    let mut updated =
        String::with_capacity(data.len() - (value_end - value_start) + new_version.len() + 2);
    updated.push_str(&data[..value_start]);
    updated.push('"');
    updated.push_str(new_version);
    updated.push('"');
    updated.push_str(&data[value_end..]);

    Ok(updated)
}

/// The byte span [start, end) of the raw string value of the top-level
/// "version" field, including its quotes.
type Span = (usize, usize);

/// find_top_level_version_value_span walks the raw (already validated) JSON
/// with a minimal token walker: it tracks object/array depth and the
/// key/value expectation inside the root object, and remembers the span of
/// the value that follows the "version" key.
fn find_top_level_version_value_span(data: &str) -> Result<Span> {
    let mut cursor = JsonCursor {
        data: data.as_bytes(),
        offset: 0,
    };

    let mut depth: u32 = 0;
    let mut expect_key = false;
    // the next token is the value of the top-level "version" key
    let mut pending = false;
    // a top-level "version" field has been seen
    let mut found = false;
    let mut span: Span = (0, 0);

    loop {
        cursor.skip_whitespace();
        if cursor.at_end() {
            break;
        }
        let token_start = cursor.offset;

        if pending {
            cursor.skip_whitespace();
            // The validated document always separates the key from its value
            // with a colon.
            if cursor.peek() == Some(b':') {
                cursor.offset += 1;
                cursor.skip_whitespace();
            }
            let value_start = cursor.offset; // at the opening quote
            let Some(text) = cursor.parse_string() else {
                return Err(Error::msg(
                    r#"the top-level "version" field is not a string"#,
                ));
            };
            let _ = text; // only the raw byte span matters for the splice
            span = (value_start, cursor.offset);
            pending = false;
            found = true;
            expect_key = true;
            continue;
        }

        match cursor.peek() {
            Some(b'{') | Some(b'[') => {
                cursor.offset += 1;
                depth += 1;
                if depth == 1 {
                    expect_key = cursor.data[token_start] == b'{';
                }
            }
            Some(b'}') | Some(b']') => {
                cursor.offset += 1;
                depth = depth.saturating_sub(1);
                if depth == 1 {
                    expect_key = true;
                }
            }
            // The ":" between a key and its value and the "," between members
            // do not produce tokens.
            Some(b':') | Some(b',') => {
                cursor.offset += 1;
            }
            Some(b'"') => {
                let text = cursor
                    .parse_string()
                    .ok_or_else(|| Error::msg("unexpected end of JSON"))?;
                if depth == 1 {
                    if expect_key {
                        expect_key = false;
                        pending = text == PACKAGE_JSON_VERSION_FIELD;
                    } else {
                        expect_key = true;
                    }
                }
            }
            Some(_) => {
                cursor.skip_primitive(); // number or literal value
                if depth == 1 {
                    expect_key = true;
                }
            }
            None => break,
        }
    }

    if depth != 0 {
        return Err(Error::msg("unexpected end of JSON"));
    }
    if !found {
        return Err(Error::msg(r#"no top-level "version" field found"#));
    }

    Ok(span)
}

/// A byte cursor over validated JSON text; every assumption it makes (escape
/// sequences, delimiter structure) was already checked by serde_json.
struct JsonCursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> JsonCursor<'a> {
    fn at_end(&self) -> bool {
        self.offset >= self.data.len()
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.offset).copied()
    }

    /// Advances past whitespace between tokens.
    fn skip_whitespace(&mut self) {
        while let Some(byte) = self.peek() {
            match byte {
                b' ' | b'\t' | b'\n' | b'\r' => self.offset += 1,
                _ => break,
            }
        }
    }

    /// Parses the string starting at the cursor (which must be positioned on
    /// the opening quote) and leaves the cursor past the closing quote.
    /// Escapes are interpreted for the plain JSON escapes; the raw byte span
    /// is what the splice needs.
    fn parse_string(&mut self) -> Option<String> {
        if self.peek() != Some(b'"') {
            return None;
        }
        self.offset += 1; // opening quote
        let mut text = String::new();
        while let Some(byte) = self.peek() {
            self.offset += 1;
            match byte {
                b'"' => return Some(text),
                b'\\' => {
                    let escape = self.peek()?;
                    self.offset += 1;
                    match escape {
                        b'"' => text.push('"'),
                        b'\\' => text.push('\\'),
                        b'/' => text.push('/'),
                        b'b' => text.push('\u{0008}'),
                        b'f' => text.push('\u{000C}'),
                        b'n' => text.push('\n'),
                        b'r' => text.push('\r'),
                        b't' => text.push('\t'),
                        // \uXXXX pairs (validated upstream) decode to a
                        // surrogate by raw code unit, like most JSON tools
                        // leave them when rendering back.
                        b'u' => {
                            let mut hex = String::new();
                            for _ in 0..4 {
                                hex.push(self.peek()? as char);
                                self.offset += 1;
                            }
                            let code = u32::from_str_radix(&hex, 16).ok()?;
                            match char::from_u32(code) {
                                Some(ch) => text.push(ch),
                                // lone surrogate; the validated document
                                // should have paired them already
                                None => text.push('\u{FFFD}'),
                            }
                        }
                        _ => return None,
                    }
                }
                _ => {
                    // copy the (multi-byte UTF-8) character verbatim
                    let start = self.offset - 1;
                    let length = byte_utf8_length(byte);
                    self.offset = start + length;
                    let slice = self.data.get(start..start + length)?;
                    text.push_str(String::from_utf8_lossy(slice).as_ref());
                }
            }
        }
        None // unterminated string
    }

    /// Advances past a primitive value (number, true, false, null).
    fn skip_primitive(&mut self) {
        while let Some(byte) = self.peek() {
            if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'+' || byte == b'.' {
                self.offset += 1;
            } else {
                break;
            }
        }
    }
}

/// Byte length of the UTF-8 sequence starting with `first`.
fn byte_utf8_length(first: u8) -> usize {
    match first {
        byte if byte < 0x80 => 1,
        byte if byte >> 5 == 0b110 => 2,
        byte if byte >> 4 == 0b1110 => 3,
        byte if byte >> 3 == 0b11110 => 4,
        _ => 1,
    }
}

#[cfg(test)]
mod tests;
