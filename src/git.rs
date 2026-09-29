//! The git facts a notification shows next to the agent: which branch the
//! pane's directory is on, whether the tree is dirty, and how many lines it
//! changed versus `HEAD`.
//!
//! Herdr's Agent sidebar gets its `branch` and `git_status` tokens from the
//! client, not from the socket API (`branch` is only exposed on worktrees), so
//! a notification has to ask git itself.

use std::fmt::Write;
use std::time::Duration;

use crate::util::command_stdout_with_timeout;

/// Ceiling on one git call, so a pathological repository delays a notification
/// instead of stalling the event hook that produces it.
const GIT_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) struct GitSummary {
    pub(crate) branch: Option<String>,
    /// Tracked files differ from `HEAD`. Untracked files are not counted, so
    /// the marker and the line counts describe the same set of changes.
    pub(crate) dirty: bool,
    /// Insertions and deletions versus `HEAD`; None when git cannot answer.
    pub(crate) changed_lines: Option<(usize, usize)>,
}

impl GitSummary {
    /// The short form shell prompts and diffstats share, joined with the same
    /// separator the sidebar rows use: `main* · +120/-45`.
    ///
    /// None when there is nothing worth showing, so a directory that is not a
    /// repository, or a detached `HEAD`, adds nothing to the notification.
    pub(crate) fn label(&self) -> Option<String> {
        let mut label = String::new();
        if let Some(branch) = self.branch.as_deref() {
            label.push_str(branch);
            if self.dirty {
                label.push('*');
            }
        }

        if let Some((inserted, deleted)) = self.changed_lines {
            if inserted > 0 || deleted > 0 {
                if !label.is_empty() {
                    label.push_str(" · ");
                }
                let _ = write!(label, "+{inserted}/-{deleted}");
            }
        }

        (!label.is_empty()).then_some(label)
    }
}

/// Best-effort: None when `cwd` is not in a repository, or when git cannot be
/// run at all, which is what keeps a notification working on a bare machine.
///
/// Both calls are bounded by `GIT_TIMEOUT`; a repository slow enough to hit it
/// contributes no git label rather than a late notification.
pub(crate) fn git_summary(cwd: &str) -> Option<GitSummary> {
    let status = command_stdout_with_timeout(
        "git",
        &[
            "-C",
            cwd,
            "status",
            "--porcelain",
            "--branch",
            "--untracked-files=no",
        ],
        GIT_TIMEOUT,
    )?;
    let (branch, dirty) = branch_and_dirty_from_status(&status);
    // Versus HEAD, so this is what the pane has changed and not committed.
    // A repository without commits yet has no HEAD; the counts are then
    // simply unknown.
    let changed_lines = command_stdout_with_timeout(
        "git",
        &["-C", cwd, "diff", "--numstat", "HEAD"],
        GIT_TIMEOUT,
    )
    .map(|numstat| changed_lines_from_numstat(&numstat));

    let summary = GitSummary {
        branch,
        dirty,
        changed_lines,
    };

    summary.label()?;
    Some(summary)
}

/// `## main...origin/main [ahead 1]` / `## main [ahead 1]` / `## HEAD (no branch)`.
fn branch_and_dirty_from_status(status: &str) -> (Option<String>, bool) {
    let mut lines = status.lines();
    let branch = lines
        .next()
        .and_then(|line| line.strip_prefix("## "))
        .and_then(branch_from_status_line);

    // Every remaining line is one changed path.
    (branch, lines.next().is_some())
}

fn branch_from_status_line(line: &str) -> Option<String> {
    // Drop the upstream (`...origin/main`) and any `[ahead 1, behind 2]` part.
    let name = line
        .split("...")
        .next()
        .unwrap_or(line)
        .split(' ')
        .next()
        .unwrap_or(line)
        .trim();

    // A detached HEAD has no branch name to show.
    if name.is_empty() || name == "HEAD" {
        None
    } else {
        Some(name.to_string())
    }
}

/// Sums `<inserted>\t<deleted>\t<path>` lines. Binary files report `-` and add
/// nothing.
fn changed_lines_from_numstat(numstat: &str) -> (usize, usize) {
    numstat.lines().fold((0, 0), |(inserted, deleted), line| {
        let mut fields = line.split('\t');
        match (
            fields.next().and_then(|value| value.parse::<usize>().ok()),
            fields.next().and_then(|value| value.parse::<usize>().ok()),
        ) {
            (Some(line_inserted), Some(line_deleted)) => {
                (inserted + line_inserted, deleted + line_deleted)
            }
            _ => (inserted, deleted),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(branch: Option<&str>, dirty: bool, lines: Option<(usize, usize)>) -> GitSummary {
        GitSummary {
            branch: branch.map(str::to_string),
            dirty,
            changed_lines: lines,
        }
    }

    #[test]
    fn labels_branch_dirty_marker_and_line_counts_like_a_prompt_does() {
        assert_eq!(
            summary(Some("main"), true, Some((120, 45)))
                .label()
                .as_deref(),
            Some("main* · +120/-45")
        );
        assert_eq!(
            summary(Some("feature/api"), false, Some((3, 0)))
                .label()
                .as_deref(),
            Some("feature/api · +3/-0")
        );
        // A clean tree carries neither marker nor counts.
        assert_eq!(
            summary(Some("main"), false, Some((0, 0)))
                .label()
                .as_deref(),
            Some("main")
        );
        assert_eq!(summary(None, false, None).label(), None);
    }

    #[test]
    fn reads_branch_and_dirty_from_status() {
        let (branch, dirty) = branch_and_dirty_from_status("## main...origin/main [ahead 1]\n");
        assert_eq!(branch.as_deref(), Some("main"));
        assert!(!dirty);

        let (branch, dirty) = branch_and_dirty_from_status("## main [ahead 1]\n M src/lib.rs\n");
        assert_eq!(branch.as_deref(), Some("main"));
        assert!(dirty);

        // Detached HEAD has no branch name, but changes still count.
        let (branch, dirty) = branch_and_dirty_from_status("## HEAD (no branch)\n M a.txt\n");
        assert_eq!(branch, None);
        assert!(dirty);

        assert_eq!(branch_and_dirty_from_status("").0, None);
    }

    #[test]
    fn sums_numstat_and_skips_binary_files() {
        assert_eq!(
            changed_lines_from_numstat("12\t3\tsrc/a.rs\n-\t-\tassets/logo.png\n8\t0\tsrc/b.rs\n"),
            (20, 3)
        );
        assert_eq!(changed_lines_from_numstat(""), (0, 0));
    }
}
