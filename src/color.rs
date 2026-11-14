//! POSIX terminal color detection and painting. Windows terminals stay
//! plain unless colors are forced on.

use std::env;
use std::io;

/// ANSI SGR escape sequences used for terminal output.
pub const ANSI_RESET: &str = "\x1b[0m";
pub const ANSI_RED: &str = "\x1b[31m";
pub const ANSI_GREEN: &str = "\x1b[32m";
pub const ANSI_YELLOW: &str = "\x1b[33m";
pub const ANSI_BOLD: &str = "\x1b[1m";

/// Anything the program paints to: writing capability plus terminal
/// awareness. A `Vec<u8>` capture in a test is never a terminal; the
/// standard streams query the underlying file descriptor.
pub trait Stream: io::Write {
    /// Reports whether the stream refers to a terminal character device.
    fn is_terminal(&self) -> bool;
}

impl Stream for io::Stdout {
    fn is_terminal(&self) -> bool {
        io::IsTerminal::is_terminal(self)
    }
}

impl Stream for io::Stderr {
    fn is_terminal(&self) -> bool {
        io::IsTerminal::is_terminal(self)
    }
}

impl Stream for Vec<u8> {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl<T: Stream + ?Sized> Stream for &mut T {
    fn is_terminal(&self) -> bool {
        (**self).is_terminal()
    }
}

/// supports_color reports whether ANSI color escapes may be written to a
/// stream with the given terminal-ness. CLICOLOR_FORCE (non-empty, not "0")
/// forces colors on anywhere; otherwise colors require a character device,
/// no non-empty NO_COLOR, a TERM different from "dumb" and a non-Windows
/// system. `getenv` and `windows` are injectable for tests.
pub fn supports_color(
    is_terminal: bool,
    getenv: &dyn Fn(&str) -> Option<String>,
    windows: bool,
) -> bool {
    if getenv("CLICOLOR_FORCE").is_some_and(|force| !force.is_empty() && force != "0") {
        return true;
    }

    if getenv("NO_COLOR").is_some_and(|no_color| !no_color.is_empty()) {
        return false;
    }
    if getenv("TERM").as_deref() == Some("dumb") {
        return false;
    }
    if windows {
        return false;
    }

    is_terminal
}

/// color_enabled reports whether ANSI colors are enabled for the given
/// stream in the current process environment and platform.
pub fn color_enabled(stream: &dyn Stream) -> bool {
    supports_color(
        stream.is_terminal(),
        &|key| env::var(key).ok(),
        cfg!(windows),
    )
}

/// paint wraps s in the given SGR escape code when colors are enabled, and
/// returns s unchanged otherwise.
pub fn paint(enabled: bool, code: &str, s: &str) -> String {
    if !enabled {
        return s.to_owned();
    }
    format!("{code}{s}{ANSI_RESET}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn getenv_from(env: &[(String, String)]) -> impl Fn(&str) -> Option<String> + '_ {
        move |key| {
            env.iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        }
    }

    #[test]
    fn supports_color_mirrors_the_detection_matrix() {
        // A character device (terminal-like), a regular file and a buffer
        // model the three stream kinds the tool writes to.
        let char_device = true;
        let regular_file = false;

        for (name, is_terminal, env, windows, want) in [
            ("char device", char_device, Vec::new(), false, true),
            (
                "char device with NO_COLOR",
                char_device,
                vec![("NO_COLOR".to_owned(), "1".to_owned())],
                false,
                false,
            ),
            (
                "char device with empty NO_COLOR",
                char_device,
                vec![("NO_COLOR".to_owned(), String::new())],
                false,
                true,
            ),
            (
                "char device with dumb TERM",
                char_device,
                vec![("TERM".to_owned(), "dumb".to_owned())],
                false,
                false,
            ),
            ("regular file", regular_file, Vec::new(), false, false),
            ("windows", char_device, Vec::new(), true, false),
            (
                "CLICOLOR_FORCE forces a buffer",
                regular_file,
                vec![("CLICOLOR_FORCE".to_owned(), "1".to_owned())],
                false,
                true,
            ),
            (
                "CLICOLOR_FORCE overrides NO_COLOR",
                regular_file,
                vec![
                    ("CLICOLOR_FORCE".to_owned(), "1".to_owned()),
                    ("NO_COLOR".to_owned(), "1".to_owned()),
                ],
                false,
                true,
            ),
            (
                "CLICOLOR_FORCE on windows",
                regular_file,
                vec![("CLICOLOR_FORCE".to_owned(), "1".to_owned())],
                true,
                true,
            ),
            (
                "CLICOLOR_FORCE 0 does not force",
                regular_file,
                vec![("CLICOLOR_FORCE".to_owned(), "0".to_owned())],
                false,
                false,
            ),
            (
                "CLICOLOR_FORCE empty does not force",
                regular_file,
                vec![("CLICOLOR_FORCE".to_owned(), String::new())],
                false,
                false,
            ),
        ] {
            let getenv = getenv_from(&env);
            let got = supports_color(is_terminal, &getenv, windows);
            assert_eq!(got, want, "supports_color({name})");
        }
    }

    #[test]
    fn test_paint() {
        assert_eq!(paint(true, ANSI_RED, "msg"), "\x1b[31mmsg\x1b[0m");
        assert_eq!(paint(false, ANSI_RED, "msg"), "msg");
    }

    #[test]
    fn buffers_are_never_terminals() {
        assert!(!color_enabled(&Vec::<u8>::new()));
    }
}
