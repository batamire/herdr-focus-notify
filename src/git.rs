//! How much the pane's directory has changed versus `HEAD`.
//!
//! The branch next to these counts comes from Herdr's own `worktree list`,
//! not from here: it is the same source the Agent sidebar uses, and it still
//! answers for a repository that has no commits yet, where `git status` reports
//! `## No commits yet on main` instead of a branch name.

use std::time::Duration;

use crate::util::command_stdout_with_timeout;

/// Ceiling on the git call, so a pathological repository delays a notification
/// instead of stalling the event hook that produces it.
const GIT_TIMEOUT: Duration = Duration::from_secs(2);

/// Longest branch the subtitle shows before it is middle-truncated. macOS gives
/// the subtitle a single line and truncates its end, so an untruncated branch
/// would push the line counts out of view — the numbers are the part that
/// cannot be guessed from the branch name.
const MAX_BRANCH_CHARS: usize = 24;

/// Characters kept from the head of a truncated branch. The head identifies the
/// work (`feature/…`), the tail separates siblings (`…-layout`).
const BRANCH_HEAD_CHARS: usize = 12;

/// The git state a notification shows for a pane's directory, in the short form
/// shell prompts and diffstats share: `main* · +120/-45`.
///
/// None when there is no branch to attribute the changes to. A detached `HEAD`
/// or a directory outside a repository contributes nothing rather than a bare
/// `+120/-45` that explains neither where nor what.
pub(crate) fn label(branch: Option<&str>, changed_lines: Option<(usize, usize)>) -> Option<String> {
    let branch = display_branch(branch?);
    let (inserted, deleted) = changed_lines.unwrap_or((0, 0));

    if inserted == 0 && deleted == 0 {
        return Some(branch);
    }

    Some(format!("{branch}* · +{inserted}/-{deleted}"))
}

/// Insertions and deletions versus `HEAD`; None when git cannot answer, which
/// is the same best-effort contract the rest of the enrichment follows.
///
/// Counts cover tracked changes only, so the `*` marker and the numbers always
/// describe the same set of changes, and they are not a branch's total: work
/// the pane has already committed shows no counts.
pub(crate) fn changed_lines(cwd: &str) -> Option<(usize, usize)> {
    // `--no-optional-locks` keeps the probe from refreshing the index. Git
    // takes `.git/index.lock` to do that, and a notification fires exactly when
    // an agent is likely to be running git in the same repository.
    //
    // `LC_ALL=C` keeps the summary line parseable: git translates it, and a
    // translated `Dateien geändert` carries no `(+)`/`(-)` suffix to key on.
    let shortstat = command_stdout_with_timeout(
        "git",
        &[
            "--no-optional-locks",
            "-C",
            cwd,
            "diff",
            "--shortstat",
            "HEAD",
        ],
        &[("LC_ALL", "C")],
        GIT_TIMEOUT,
    )?;

    Some(changed_lines_from_shortstat(&shortstat))
}

/// ` 3 files changed, 12 insertions(+), 4 deletions(-)`, one clause per kind of
/// count, and either clause is left out when its count is zero. The counts are
/// keyed on the `(+)`/`(-)` suffixes rather than the words, which are
/// pluralized (`1 insertion(+)`) and translated.
fn changed_lines_from_shortstat(shortstat: &str) -> (usize, usize) {
    shortstat
        .split(',')
        .fold((0, 0), |(inserted, deleted), clause| {
            let clause = clause.trim();
            let count = clause
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<usize>().ok());

            match count {
                Some(count) if clause.ends_with("(+)") => (inserted + count, deleted),
                Some(count) if clause.ends_with("(-)") => (inserted, deleted + count),
                _ => (inserted, deleted),
            }
        })
}

/// Middle-truncates a branch too long for the subtitle's one line.
///
/// Counted in `chars`, not bytes: slicing a branch with a non-ASCII character
/// at a byte offset would panic.
fn display_branch(branch: &str) -> String {
    let chars: Vec<char> = branch.chars().collect();
    if chars.len() <= MAX_BRANCH_CHARS {
        return branch.to_string();
    }

    // One of the budgeted characters is the ellipsis.
    let tail_chars = MAX_BRANCH_CHARS - BRANCH_HEAD_CHARS - 1;
    let mut truncated: String = chars[..BRANCH_HEAD_CHARS].iter().collect();
    truncated.push('…');
    truncated.extend(&chars[chars.len() - tail_chars..]);
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_branch_dirty_marker_and_line_counts_like_a_prompt_does() {
        assert_eq!(
            label(Some("main"), Some((120, 45))).as_deref(),
            Some("main* · +120/-45")
        );
        assert_eq!(
            label(Some("feature/api"), Some((3, 0))).as_deref(),
            Some("feature/api* · +3/-0")
        );
        // A clean tree carries neither marker nor counts.
        assert_eq!(label(Some("main"), Some((0, 0))).as_deref(), Some("main"));
        assert_eq!(label(Some("main"), None).as_deref(), Some("main"));
        // Nothing to attribute the changes to, so nothing to show.
        assert_eq!(label(None, Some((120, 45))), None);
        assert_eq!(label(None, None), None);
    }

    #[test]
    fn middle_truncates_a_long_branch_to_keep_the_counts_on_the_line() {
        assert_eq!(display_branch("main"), "main");
        assert_eq!(display_branch("feature/JIRA-123-authentication-layout"), {
            let truncated = "feature/JIRA…tion-layout";
            assert_eq!(truncated.chars().count(), MAX_BRANCH_CHARS);
            truncated
        });
        // Counted in chars: a multi-byte branch must not panic on a slice.
        let long_unicode = "feature/ünicode-branch-name-that-is-long";
        assert_eq!(
            display_branch(long_unicode).chars().count(),
            MAX_BRANCH_CHARS
        );
    }

    #[test]
    fn sums_shortstat_by_its_count_suffixes() {
        assert_eq!(
            changed_lines_from_shortstat(" 3 files changed, 12 insertions(+), 4 deletions(-)"),
            (12, 4)
        );
        // Pluralization varies, and either clause is omitted at zero.
        assert_eq!(
            changed_lines_from_shortstat(" 1 file changed, 1 insertion(+)"),
            (1, 0)
        );
        assert_eq!(
            changed_lines_from_shortstat(" 2 files changed, 5 deletions(-)"),
            (0, 5)
        );
        // A clean tree prints nothing at all.
        assert_eq!(changed_lines_from_shortstat(""), (0, 0));
    }
}
