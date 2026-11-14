//! Conventional Commits parsing and release statistics derived from the Git
//! history, including revert resolution.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use crate::error::{Error, Result};
use crate::utils::{self, has_leading_whitespace, has_trailing_whitespace, warn};
use crate::version::ReleaseType;

/// Conventional Commits kinds recognized by the tool and the changelog.
pub const KIND_FEAT: &str = "feat";
pub const KIND_FIX: &str = "fix";
pub const KIND_PERF: &str = "perf";
pub const KIND_REFACTOR: &str = "refactor";
pub const KIND_DOCS: &str = "docs";
pub const KIND_TEST: &str = "test";
pub const KIND_BUILD: &str = "build";
pub const KIND_CI: &str = "ci";
pub const KIND_STYLE: &str = "style";
pub const KIND_CHORE: &str = "chore";
pub const KIND_REVERT: &str = "revert";

/// Commit is one parsed Conventional Commits message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub kind: String,
    pub scope: String,
    pub description: String,
    pub body: String,
    pub footer: String,
    pub breaking: bool,
    /// commit hashes this commit refers to
    pub refers_to: Vec<String>,
}

impl Commit {
    /// Returns the first seven characters of the commit hash, or the whole
    /// hash when it is not longer than that.
    pub fn short_hash(&self) -> String {
        ensure_short_commit_hash(&self.hash)
    }
}

/// CommitStats aggregates the parsed commits of the release window.
#[derive(Debug, Default)]
pub struct CommitStats {
    /// Groups the live commits of the release window by their Conventional
    /// Commits kind. Revert commits are not included there: reverts only
    /// feed the "Reverted Changes" changelog section.
    pub by_kind: BTreeMap<String, Vec<Commit>>,
    pub has_breaking_change: bool,

    /// The number of live commits since the release tag. It lets a forced
    /// release type refuse an empty release window.
    pub in_window_commits: usize,

    /// Commits from before the tag that got reverted by reverts since the
    /// tag.
    pub reverted_pre_tag: Vec<Commit>,
}

impl CommitStats {
    /// Derives the release type from the collected commits: any breaking
    /// change makes a major release, any feat makes a minor one, any fix or
    /// perf makes a patch, and anything else means nothing to release.
    pub fn determine_release_type(&self) -> ReleaseType {
        if self.has_breaking_change {
            return ReleaseType::Major;
        }
        if self
            .by_kind
            .get(KIND_FEAT)
            .is_some_and(|commits| !commits.is_empty())
        {
            return ReleaseType::Minor;
        }
        if self
            .by_kind
            .get(KIND_FIX)
            .is_some_and(|commits| !commits.is_empty())
            || self
                .by_kind
                .get(KIND_PERF)
                .is_some_and(|commits| !commits.is_empty())
        {
            return ReleaseType::Patch;
        }
        ReleaseType::None
    }
}

/// process_commit_messages parses the full Git history, resolves reverts
/// against the since_tag boundary and collects the statistics that drive the
/// release type. An empty since_tag puts every commit into the release
/// window.
pub fn process_commit_messages(
    repo: &Path,
    since_tag: &str,
    ignore_invalid_commits: bool,
    allowed_kinds: &[String],
) -> Result<CommitStats> {
    let all_lines = get_git_commit_lines(repo, "")?; // full history newest->oldest

    let mut has_invalid_commits = false;

    // parse into chronological order (oldest first)
    let mut parsed: Vec<Commit> = Vec::new();
    for message in all_lines.iter().rev() {
        match parse_conventional_commit(message, true, allowed_kinds) {
            Ok(commit) => parsed.push(commit),
            Err(err) => {
                // Invalid commits outside the release window are silently
                // skipped when a tag bounds the window; only the unbounded
                // scan reports them.
                if since_tag.is_empty() {
                    has_invalid_commits = true;
                    log_invalid_commit(ignore_invalid_commits, message, &err);
                }
                continue;
            }
        }
    }

    if has_invalid_commits && !ignore_invalid_commits {
        return Err(Error::InvalidCommits);
    }

    // map commits by short hash and record whether they are pre-tag
    let mut by_ref: BTreeMap<String, Commit> = BTreeMap::new();

    // Set of the short hashes of commits that are before the since_tag
    // boundary. Only parsed commits are tracked here: references to unknown
    // hashes are never treated as pre-tag.
    let mut pre_tag: HashSet<String> = HashSet::new();

    // Build set of commits that are "sinceTag..HEAD"
    let mut since_set: HashSet<String> = HashSet::new();
    if !since_tag.is_empty() {
        let since_lines = get_git_commit_lines(repo, since_tag)?;

        // Invalid commits inside the release window are tracked separately:
        // like the full-history scan above, they can fail the release.
        let mut has_invalid_window_commits = false;

        for message in &since_lines {
            match parse_conventional_commit(message, true, allowed_kinds) {
                Ok(commit) => {
                    since_set.insert(commit.short_hash());
                }
                Err(err) => {
                    has_invalid_window_commits = true;
                    log_invalid_commit(ignore_invalid_commits, message, &err);
                }
            }
        }

        if has_invalid_window_commits && !ignore_invalid_commits {
            return Err(Error::InvalidCommits);
        }
    } else {
        // no tag -> everything treated as in-window
        for commit in &parsed {
            since_set.insert(commit.short_hash());
        }
    }

    for commit in &parsed {
        let reference = commit.short_hash();
        by_ref.insert(reference.clone(), commit.clone());
        // consider a commit pre-tag if it's NOT in since_set
        if !since_set.contains(&reference) {
            pre_tag.insert(reference);
        }
    }

    // Walk chronologically, toggling reverted state.
    let mut reverted: HashSet<String> = HashSet::new();

    // track commits from before the tag that get reverted by reverts seen
    // since the tag (sorted for a deterministic render order)
    let mut reverted_pre_tag_set: BTreeSet<String> = BTreeSet::new();

    for commit in &parsed {
        let short = commit.short_hash();

        if commit.kind == KIND_REVERT {
            // for each referenced commit, flip reverted state
            for unshortened in &commit.refers_to {
                let reference = ensure_short_commit_hash(unshortened);

                if reverted.remove(&reference) {
                    // if this is a revert-of-revert that restores a pre-tag
                    // commit, remove it from reverted_pre_tag_set
                    if pre_tag.contains(&reference) {
                        reverted_pre_tag_set.remove(&reference);
                    }
                } else {
                    reverted.insert(reference.clone());

                    // if this revert commit is in-window (since_tag) and it
                    // reverts a pre-tag commit, record it
                    if since_set.contains(&short) && pre_tag.contains(&reference) {
                        reverted_pre_tag_set.insert(reference);
                    }
                }
            }

            continue;
        }

        // Non-revert commits: nothing to do during the walk besides
        // tracking existence (already done)
    }

    // Build final CommitStats including detection of pre-tag commits
    // reverted by in-window reverts
    let mut commit_stats = CommitStats::default();

    for (short, commit) in &by_ref {
        // include only commits in the requested window
        if !since_set.contains(short) {
            continue;
        }

        // skip commits that are currently reverted
        if reverted.contains(short) {
            continue;
        }

        commit_stats.in_window_commits += 1;

        if commit.breaking {
            commit_stats.has_breaking_change = true;
        }

        // Revert commits are rendered by the dedicated
        // "Reverted Changes" section, not by kind sections
        if commit.kind == KIND_REVERT {
            continue;
        }

        commit_stats
            .by_kind
            .entry(commit.kind.clone())
            .or_default()
            .push(commit.clone());
    }

    // Populate reverted_pre_tag list from the set
    for reference in &reverted_pre_tag_set {
        if let Some(commit) = by_ref.get(reference) {
            commit_stats.reverted_pre_tag.push(commit.clone());
            if commit.breaking {
                commit_stats.has_breaking_change = true;
            }
        } else if !ignore_invalid_commits {
            warn(&format!(
                "WARNING: commit with hash {reference} was not added to the list of reverted \
                 commits! It is most likely an invalid commit or a non-existent hash."
            ));
        }
    }

    Ok(commit_stats)
}

/// log_invalid_commit reports an invalid commit message as a warning when it
/// is ignored, or as an error when it fails the release.
fn log_invalid_commit(ignore_invalid_commits: bool, commit_message: &str, err: &Error) {
    if ignore_invalid_commits {
        warn(&format!(
            "WARNING: ignored invalid commit message: {commit_message}. Reason: {err}"
        ));
    } else {
        warn(&format!(
            "ERROR: invalid commit message: {commit_message}. Reason: {err}"
        ));
    }
}

/// get_git_commit_lines returns one string per commit: the hash followed by
/// the full commit message, including body and footers, so attributes such
/// as BREAKING CHANGE and Refs participate in release type detection.
pub fn get_git_commit_lines(repo: &Path, since_tag: &str) -> Result<Vec<String>> {
    // \x1e (ASCII record separator) terminates every commit record.
    const RECORD_SEPARATOR: char = '\x1e';
    const PRETTY_FORMAT: &str = "--pretty=format:%H %B\x1e";

    let args: Vec<String> = if since_tag.is_empty() {
        vec![
            "log".to_owned(),
            PRETTY_FORMAT.to_owned(),
            "--abbrev=false".to_owned(),
        ]
    } else {
        vec![
            "log".to_owned(),
            format!("{since_tag}..HEAD"),
            PRETTY_FORMAT.to_owned(),
            "--abbrev=false".to_owned(),
        ]
    };

    // Get all commits since the provided tag
    let out = utils::git(repo, &args)?;
    if !out.success {
        return Err(Error::msg(format!(
            "error getting commits from Git: {}",
            out.stderr
        )));
    }

    // Trim each record and skip empty ones
    Ok(out
        .stdout
        .split(RECORD_SEPARATOR)
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .map(str::to_owned)
        .collect())
}

/// warn_no_commits_that_can_increment_current_version prints the warning
/// shown when no commit in the release window can increment the version.
pub fn warn_no_commits_that_can_increment_current_version() {
    warn("no commits that can increment current version");
}

/// Parses a Conventional Commits message and validates it. When has_hash is
/// set, message starts with the commit hash and a space as read from Git
/// history. allowed_kinds is a strict allow-list: a commit whose kind is
/// missing from it is rejected.
pub fn parse_conventional_commit(
    message: &str,
    has_hash: bool,
    allowed_kinds: &[String],
) -> Result<Commit> {
    if message.trim().is_empty() {
        return Err(Error::msg("empty commit message"));
    }

    // Split into lines preserving order
    let lines: Vec<&str> = message.split('\n').collect();
    for (line_index, line) in lines.iter().enumerate() {
        let line_number = line_index + 1;
        if has_leading_whitespace(line) || has_trailing_whitespace(line) {
            return Err(Error::msg(format!(
                "commit message line number {line_number} (`{line}`) should not have leading \
                 or trailing whitespace"
            )));
        }
    }

    // Check for empty lines at the end of the commit message
    if lines.len() >= 2 && lines.last().is_some_and(|last| last.trim().is_empty()) {
        return Err(Error::msg(
            "commit message should not have empty lines at the end of it",
        ));
    }

    let mut commit = Commit::default();

    let mut header = lines[0];
    if header.is_empty() {
        return Err(Error::msg("empty commit message header"));
    }

    // Parse optional leading hash
    if has_hash {
        let Some(first_field) = header.split_whitespace().next() else {
            return Err(Error::msg("empty commit message header"));
        };

        let hash = parse_commit_hash(first_field)?;
        commit.hash = hash;

        if header.len() <= commit.hash.len() || header.as_bytes()[commit.hash.len()] != b' ' {
            return Err(Error::msg("hash must be followed by a single space"));
        }

        // reconstruct the header without hash and the single space after it
        header = &header[commit.hash.len() + 1..];
    }

    // Check for leading spaces again
    if has_leading_whitespace(header) {
        return Err(Error::msg(
            "commit message header should not have leading whitespace",
        ));
    }

    // Find the first ":" that separates header from description
    let Some((before, after)) = header.split_once(':') else {
        return Err(Error::msg(
            "commit message header must have colon to separate commit kind and description",
        ));
    };

    // Parse and validate kind (stage 1)
    let mut header_before_description = before;
    if has_leading_whitespace(header_before_description) {
        return Err(Error::msg(
            "commit message kind should not have leading whitespace",
        ));
    }
    if has_trailing_whitespace(header_before_description) {
        return Err(Error::msg(
            "commit message kind should not have trailing whitespace before colon",
        ));
    }
    let mut breaking = false;
    if let Some(stripped) = header_before_description.strip_suffix('!') {
        breaking = true;
        // Get rid of !
        header_before_description = stripped;
        // Check for trailing spaces
        if has_trailing_whitespace(header_before_description) {
            return Err(Error::msg(
                "commit message kind should not have whitespace before `!'",
            ));
        }
    }

    // Extract kind and optional scope (stage 2)
    let kind;
    let scope;
    if let Some(start) = header_before_description.find('(') {
        // attempt to find matching ')'
        let end = match header_before_description.rfind(')') {
            Some(end) if end > start => end,
            _ => {
                return Err(Error::msg(
                    "commit message kind must have matching parentheses",
                ));
            }
        };

        kind = &header_before_description[..start];
        if has_trailing_whitespace(kind) {
            return Err(Error::msg(
                "commit message kind should not have spaces before `('",
            ));
        }

        scope = &header_before_description[start + 1..end];
        if has_leading_whitespace(scope) || has_trailing_whitespace(scope) {
            return Err(Error::msg(
                "commit message scope should not be enclosed with spaces",
            ));
        }

        if scope.is_empty() {
            return Err(Error::msg(
                "commit message scope cannot be empty within parentheses",
            ));
        }
    } else {
        kind = header_before_description;
        scope = "";
    }
    if kind.is_empty() {
        return Err(Error::msg(
            "commit message header should have non-empty kind",
        ));
    }
    commit.kind = kind.to_owned();
    commit.scope = scope.to_owned();
    commit.breaking = breaking;

    // Parse and validate description
    let description = after;
    if !description.starts_with(' ') {
        return Err(Error::msg(
            "commit message header must have space after colon",
        ));
    }
    let description = &description[1..]; // skip one space
    if has_leading_whitespace(description) {
        return Err(Error::msg(
            "commit message description should have only one whitespace after colon",
        ));
    }
    if has_trailing_whitespace(description) {
        return Err(Error::msg(
            "commit message description should not have trailing whitespace",
        ));
    }
    if description.is_empty() {
        return Err(Error::msg(
            "commit message header should have non-empty description",
        ));
    }
    if description.ends_with('.') {
        return Err(Error::msg(
            "commit message description should not end with period",
        ));
    }
    if description.chars().next().is_some_and(char::is_uppercase) {
        return Err(Error::msg(
            "commit message description should not start with a capital letter",
        ));
    }
    const MAX_DESCRIPTION_CHARACTERS: usize = 120;
    if description.chars().count() > MAX_DESCRIPTION_CHARACTERS {
        return Err(Error::msg(format!(
            "commit message description should not be more than {MAX_DESCRIPTION_CHARACTERS} characters"
        )));
    }
    commit.description = description.to_owned();

    // Validate kind against allowed_kinds
    if !allowed_kinds.iter().any(|allowed| allowed == &commit.kind) {
        return Err(Error::msg(format!(
            "commit message kind {} not allowed",
            commit.kind
        )));
    }

    // The remainder of the message (the lines following the header) is body
    // and footer separated by a blank line.
    let (body_lines, footer_lines) = split_body_and_footer(&lines);

    commit.body = body_lines.join("\n").trim().to_owned();
    commit.footer = footer_lines.join("\n").trim().to_owned();

    // If footer mentions BREAKING CHANGE, mark breaking
    if !commit.breaking && commit.footer.contains("BREAKING CHANGE") {
        commit.breaking = true;
    }

    // Parse references from body and footer
    for line in body_lines.iter().chain(footer_lines.iter()) {
        try_parse_reference(line, &mut commit)?;
    }

    // Make sure revert commits have at least one reference to other commits
    if commit.kind == KIND_REVERT && commit.refers_to.is_empty() {
        return Err(Error::msg(
            "revert commits must have hashes of the commits they revert; add `Refs: ` attribute \
             to commit footer",
        ));
    }

    // Check duplicate references
    if !commit.refers_to.is_empty() {
        let mut already_seen_refs: HashSet<&String> =
            HashSet::with_capacity(commit.refers_to.len());
        for reference in &commit.refers_to {
            if !already_seen_refs.insert(reference) {
                return Err(Error::msg(format!(
                    "commit message has duplicated ref hash {reference}"
                )));
            }
        }
    }

    Ok(commit)
}

/// Splits the message lines following the header into body lines and footer
/// lines, separated by the first blank line. The footer is everything after
/// the blank separator; with no separator the whole remainder is the body.
/// Leading blank lines right after the header are consumed here and get
/// trimmed away when the body is joined.
fn split_body_and_footer<'a>(lines: &[&'a str]) -> (Vec<&'a str>, Vec<&'a str>) {
    if lines.len() <= 1 {
        return (Vec::new(), Vec::new());
    }

    // consume leading blank lines between header and body
    let mut index = 1;
    while index < lines.len() && lines[index].trim().is_empty() {
        index += 1;
    }

    // Collect lines until a blank line that separates body and footer
    while index < lines.len() {
        if lines[index].trim().is_empty() {
            // everything after the blank line (skipping blank lines) is footer
            let mut footer_start = index + 1;
            while footer_start < lines.len() && lines[footer_start].trim().is_empty() {
                footer_start += 1;
            }

            let body: Vec<&str> = lines[1..index].to_vec();
            if footer_start < lines.len() {
                // footer exists
                let footer: Vec<&str> = lines[footer_start..].to_vec();
                return (body, footer);
            }

            // trailing blank lines only; body is lines[1..index]
            return (body, Vec::new());
        }
        index += 1;
    }

    // no blank separator found; everything after header is body
    (lines[1..].to_vec(), Vec::new())
}

/// parse_commit_hash validates a 7- or 40-character lowercase hexadecimal
/// commit hash.
fn parse_commit_hash(hash: &str) -> Result<String> {
    if hash.len() != 7 && hash.len() != 40 {
        return Err(Error::msg(
            "commit hash length must be either 7 or 40 characters long",
        ));
    }

    if !hash.chars().all(is_lower_hex_digit) {
        return Err(Error::msg(
            "commit message expected to have lowercase hexadecimal digits only",
        ));
    }

    Ok(hash.to_owned())
}

/// ensure_short_commit_hash truncates hash to its seven-character
/// abbreviation when it is longer.
fn ensure_short_commit_hash(hash: &str) -> String {
    if hash.chars().count() > 7 {
        return hash.chars().take(7).collect();
    }
    hash.to_owned()
}

/// try_parse_reference parses a footer line of the form "Attribute: value"
/// into commit; lines without a colon are left alone. Only the Refs
/// attribute is meaningful to this tool, so any other line is treated as
/// ordinary body prose: it may legitimately contain a colon without a
/// following space (for example a bare `publish:dry` or `*:focus`).
fn try_parse_reference(line: &str, commit: &mut Commit) -> Result<()> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(());
    }

    let Some((attribute_name, attribute_value)) = line.split_once(':') else {
        return Ok(());
    };

    const REFS_ATTRIBUTE: &str = "Refs";

    // Parse attribute name. Ignore every attribute that is not Refs:
    // body prose frequently contains colons that do not start a footer.
    if !attribute_name.trim().eq_ignore_ascii_case(REFS_ATTRIBUTE) {
        return Ok(());
    }

    if has_trailing_whitespace(attribute_name) {
        return Err(Error::msg(format!(
            "footer line attribute should not have spaces before colon: {line}"
        )));
    }

    // Parse attribute value
    if !attribute_value.starts_with(' ') {
        return Err(Error::msg(format!(
            "footer line attribute value should not have only one space after colon: {line}"
        )));
    }
    let attribute_value = &attribute_value[1..]; // skip leading space
    if has_leading_whitespace(attribute_value) {
        return Err(Error::msg(format!(
            "footer line attribute value should not have spaces after colon with spaces: {line}"
        )));
    }

    if attribute_name != REFS_ATTRIBUTE {
        return Err(Error::msg(format!(
            "attribute {attribute_name} should have consistent case like this: {REFS_ATTRIBUTE}"
        )));
    }

    for (hash_index, mut hash_str) in attribute_value.split(',').enumerate() {
        if hash_index != 0 {
            if !hash_str.starts_with(' ') {
                return Err(Error::msg(format!(
                    "hash list in attribute {attribute_name} should be delimited by comma and \
                     a single space: {hash_str}"
                )));
            }
            hash_str = &hash_str[1..]; // skip space
        }
        if has_leading_whitespace(hash_str) || has_trailing_whitespace(hash_str) {
            return Err(Error::msg(format!(
                "hash list in attribute {attribute_name} should be delimited by comma and \
                 a single space: {hash_str}"
            )));
        }

        let hash = parse_commit_hash(hash_str)
            .map_err(|err| Error::msg(format!("invalid hash {hash_str}: {err}")))?;

        commit.refers_to.push(hash);
    }

    Ok(())
}

/// Reports whether r is a decimal digit or a lowercase a-f letter.
fn is_lower_hex_digit(r: char) -> bool {
    r.is_ascii_digit() || matches!(r, 'a'..='f')
}

#[cfg(test)]
mod tests;
