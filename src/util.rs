use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How often a bounded command is checked for having finished.
const TIMEOUT_POLL: Duration = Duration::from_millis(20);

/// Runs a command and returns its stdout on success. None when the binary is
/// missing, the command fails, or the output is not valid UTF-8.
pub(crate) fn command_stdout(bin: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(bin).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// Like `command_stdout`, but kills the command once `timeout` has passed and
/// reports None, and with `env` added to the child's environment. `Command` has
/// no timeout of its own, so the child is polled until the deadline; stdout is
/// drained on another thread because a command that fills the pipe buffer
/// before it exits would otherwise deadlock.
pub(crate) fn command_stdout_with_timeout(
    bin: &str,
    args: &[&str],
    env: &[(&str, &str)],
    timeout: Duration,
) -> Option<String> {
    let mut child = Command::new(bin)
        .args(args)
        .envs(env.iter().copied())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let mut pipe = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = pipe.read_to_end(&mut buffer);
        buffer
    });

    let deadline = Instant::now() + timeout;
    let succeeded = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(TIMEOUT_POLL),
            // Past the deadline, or the wait itself failed: stop the child and
            // let the reader thread finish with whatever was captured.
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };

    let stdout = reader.join().ok()?;
    succeeded.then(|| String::from_utf8(stdout).ok())?
}

pub(crate) fn sanitize_group_id(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

pub(crate) fn notification_group_id(pane_id: &str) -> String {
    format!("herdr-{}", sanitize_group_id(pane_id))
}

/// The workspace part of a pane id, e.g. `w1:p3` -> `w1`.
///
/// Herdr pane ids are `workspace:pane`; the workspace is stable while panes
/// are created and destroyed inside it. Used to key per-workspace terminal
/// bindings.
pub(crate) fn workspace_id_from_pane_id(pane_id: &str) -> Option<&str> {
    pane_id.split(':').next().filter(|value| !value.is_empty())
}

pub(crate) fn shell_quote(value: &str) -> String {
    let mut quoted = String::from("'");
    for ch in value.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_command_returns_output_when_it_finishes_in_time() {
        let stdout = command_stdout_with_timeout(
            "sh",
            &["-c", "printf 'hello'"],
            &[],
            Duration::from_secs(5),
        );

        assert_eq!(stdout.as_deref(), Some("hello"));
    }

    #[test]
    fn bounded_command_passes_its_environment_through() {
        let stdout = command_stdout_with_timeout(
            "sh",
            &["-c", "printf '%s' \"$LC_ALL\""],
            &[("LC_ALL", "C")],
            Duration::from_secs(5),
        );

        assert_eq!(stdout.as_deref(), Some("C"));
    }

    #[test]
    fn bounded_command_gives_up_and_reports_none_when_it_overruns() {
        let started = Instant::now();
        let stdout =
            command_stdout_with_timeout("sh", &["-c", "sleep 30"], &[], Duration::from_millis(200));

        assert_eq!(stdout, None);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn extracts_workspace_from_pane_id() {
        assert_eq!(workspace_id_from_pane_id("w1:p3"), Some("w1"));
        assert_eq!(workspace_id_from_pane_id("w2:agent-42"), Some("w2"));
        assert_eq!(workspace_id_from_pane_id("no-colon"), Some("no-colon"));
        assert_eq!(workspace_id_from_pane_id(""), None);
        assert_eq!(workspace_id_from_pane_id(":p1"), None);
    }

    #[test]
    fn shell_quote_handles_apostrophes() {
        assert_eq!(shell_quote("/tmp/it's ok"), "'/tmp/it'\\''s ok'");
    }
}
