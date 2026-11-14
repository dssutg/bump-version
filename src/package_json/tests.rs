//! Tests for the package.json version bump.

use super::*;
use crate::utils::testing::TempDir;
use std::path::PathBuf;

#[test]
fn replace_json_version_table() {
    let cases = [
        (
            "replaces top-level version and keeps formatting",
            "{\n  \"name\": \"app\",\n  \"version\": \"1.0.0\",\n  \"private\": true\n}\n",
            "{\n  \"name\": \"app\",\n  \"version\": \"1.1.0\",\n  \"private\": true\n}\n",
        ),
        (
            "ignores nested version fields",
            "{\n  \"version\": \"1.0.0\",\n  \"dependencies\": {\n    \"version\": \"2.0.0\"\n  }\n}",
            "{\n  \"version\": \"1.1.0\",\n  \"dependencies\": {\n    \"version\": \"2.0.0\"\n  }\n}",
        ),
        (
            "handles version as the last field",
            "{\"name\":\"app\",\"version\":\"1.0.0\"}",
            "{\"name\":\"app\",\"version\":\"1.1.0\"}",
        ),
        (
            "ignores a non-key string equal to version",
            "{\"name\": \"version\", \"version\": \"1.0.0\"}",
            "{\"name\": \"version\", \"version\": \"1.1.0\"}",
        ),
        (
            "keeps compact formatting intact",
            "{\"version\":\"1.0.0\",\"v\":2}",
            "{\"version\":\"1.1.0\",\"v\":2}",
        ),
    ];

    for (name, input, want) in cases {
        let got = replace_json_version(input, "1.1.0")
            .unwrap_or_else(|err| panic!("replace_json_version({name}): {err}"));
        assert_eq!(got, want, "replace_json_version({name})");
    }
}

#[test]
fn replace_json_version_errors() {
    let cases = [
        (
            "no version field",
            "{\"name\": \"app\"}",
            "no top-level \"version\" field found",
        ),
        ("non-string version", "{\"version\": 1}", "is not a string"),
        (
            "top-level array",
            "[\"version\"]",
            "no top-level \"version\" field found",
        ),
        ("truncated", "{\"version\":", "parsing"),
        (
            "malformed trailing comma",
            "{\"version\": \"1.0.0\",}",
            "trailing comma",
        ),
    ];

    for (name, input, want_err) in cases {
        let err = replace_json_version(input, "1.1.0")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains(want_err),
            "replace_json_version({name}) error = {err}, want containing {want_err:?}"
        );
    }
}

#[test]
fn update_package_json_version_writes_file() {
    let dir = TempDir::new("packagejson");
    let path: PathBuf = dir.path().join("package.json");
    std::fs::write(&path, "{\n  \"version\": \"1.0.0\"\n}\n")
        .expect("could not write package.json");

    update_package_json_version(&path, "2.0.0")
        .expect("update_package_json_version() must succeed");

    let got = std::fs::read_to_string(&path).expect("could not read package.json");
    assert_eq!(got, "{\n  \"version\": \"2.0.0\"\n}\n");
}

#[test]
fn update_package_json_version_missing_file() {
    let dir = TempDir::new("packagejson-missing");
    let path = dir.path().join("absent.json");
    assert!(
        update_package_json_version(&path, "1.0.0").is_err(),
        "update_package_json_version() for a missing file must fail"
    );
}
