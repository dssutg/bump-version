//! Tests for the shared helpers.

use super::*;
use std::io::Cursor;

const TEST_QUESTION: &str = "Continue?";

fn ask(question_input: &str, default_val: bool) -> (bool, Vec<u8>) {
    let mut out = Vec::new();
    let mut reader = Cursor::new(question_input.as_bytes());
    let got = ask_yes_no(&mut reader, &mut out, TEST_QUESTION, default_val);
    (got, out)
}

#[test]
fn ask_yes_no_answer_table() {
    let cases: [(&str, bool, bool); 9] = [
        ("y\n", false, true),
        ("yes\n", false, true),
        ("n\n", true, false),
        ("no\n", true, false),
        ("Y\n", false, true),
        ("Yes\n", false, true),
        ("  y  \n", false, true),
        ("\n", true, true),
        ("\n", false, false),
    ];

    for (input, default_val, want) in cases {
        let (got, out) = ask(input, default_val);
        assert_eq!(got, want, "ask_yes_no({input:?})");

        let want_prompt = if default_val {
            format!("{TEST_QUESTION} [Y/n]: ")
        } else {
            format!("{TEST_QUESTION} [y/N]: ")
        };
        let out = String::from_utf8(out).unwrap();
        assert!(
            out.contains(&want_prompt),
            "prompt = {out:?}, want {want_prompt:?}"
        );
        assert!(
            !out.contains('\x1b'),
            "prompt for a buffer contains escape sequences: {out:?}"
        );
    }
}

#[test]
fn ask_yes_no_eof_returns_default() {
    let (got, out) = ask("", false);
    assert!(!got, "ask_yes_no() on EOF = true, want the default false");
    assert!(
        String::from_utf8(out).unwrap().contains(TEST_QUESTION),
        "prompt must contain the question"
    );
}

#[test]
fn ask_yes_no_reprompts_on_invalid_input() {
    let (got, out) = ask("maybe\ny\n", false);
    assert!(
        got,
        "ask_yes_no() = false, want true after an invalid answer"
    );

    let out = String::from_utf8(out).unwrap();
    assert!(
        out.contains("Please answer yes or no (y/n)."),
        "output = {out:?}, want the re-prompt notice"
    );
    assert_eq!(out.matches(TEST_QUESTION).count(), 2, "prompt shown twice");
}

#[test]
fn file_exists_reports_existence() {
    let dir = testing::TempDir::new("file-exists");
    let path = dir.path().join("file.txt");
    assert!(
        !file_exists(&path),
        "file_exists() = true for a missing file"
    );

    std::fs::write(&path, "x").expect("could not write file");
    assert!(
        file_exists(&path),
        "file_exists() = false for an existing file"
    );
}

#[test]
fn has_leading_whitespace_table() {
    let cases: [(&str, bool); 6] = [
        ("", false),
        (" x", true),
        ("\tx", true),
        ("\nx", true),
        ("x", false),
        ("\u{a0}x", true), // non-breaking space
    ];

    for (input, want) in cases {
        assert_eq!(
            has_leading_whitespace(input),
            want,
            "has_leading_whitespace({input:?})"
        );
    }
}

#[test]
fn has_trailing_whitespace_table() {
    let cases: [(&str, bool); 4] = [("", false), ("x ", true), ("x\n", true), ("x", false)];

    for (input, want) in cases {
        assert_eq!(
            has_trailing_whitespace(input),
            want,
            "has_trailing_whitespace({input:?})"
        );
    }
}
