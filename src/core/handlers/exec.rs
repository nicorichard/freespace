// The single subprocess entry point for handlers.
//
// Every command freespace runs goes through here. The rules are deliberately
// narrow, because this is the only place in the app that executes anything:
//
//   * argv arrays only — never `sh -c`, so no shell metacharacter interpretation
//     is possible and argument boundaries are exact.
//   * stdin is closed, so a command can never block waiting for input and hang
//     the cleanup thread.
//   * stdout/stderr are captured, never inherited, so a stray write cannot
//     corrupt the TUI's terminal state.
//   * a wall-clock timeout, so a wedged tool cannot hang cleanup forever.
//
// Handlers are compiled into the binary; manifests select them by name and can
// never introduce a new command. See `src/core/handlers/mod.rs`.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

/// Default wall-clock limit for a handler subprocess.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Render an argv array the way it would be typed at a shell, for display in
/// the confirmation UI and the audit log.
pub fn display_argv(argv: &[&str]) -> String {
    argv.iter()
        .map(|arg| {
            if arg.is_empty() || arg.contains(|c: char| c.is_whitespace() || c == '"') {
                format!("{:?}", arg)
            } else {
                (*arg).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a program can be resolved on `PATH`. Used by handler probes so an
/// unavailable tool yields no items instead of an error.
pub fn program_exists(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(program);
        candidate.is_file() && is_executable(&candidate)
    })
}

#[cfg(unix)]
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &std::path::Path) -> bool {
    path.is_file()
}

/// Captured result of a subprocess run.
#[derive(Debug)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub status: Option<i32>,
}

impl Output {
    /// The most useful single line of failure text for a user-facing message.
    pub fn error_summary(&self) -> String {
        let detail = self
            .stderr
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("no error output");
        match self.status {
            Some(code) => format!("exited {}: {}", code, detail),
            None => format!("terminated by signal: {}", detail),
        }
    }
}

/// Run `argv` with the default timeout, returning captured output regardless of
/// exit status. Errors are reserved for failure to run at all.
pub fn run(argv: &[&str]) -> Result<Output> {
    run_with(argv, &[], DEFAULT_TIMEOUT)
}

/// Run `argv` with extra environment variables and an explicit timeout.
///
/// `argv[0]` is the program; the rest are passed as exact, separate arguments.
pub fn run_with(argv: &[&str], env: &[(&str, &str)], timeout: Duration) -> Result<Output> {
    let Some((program, args)) = argv.split_first() else {
        bail!("empty command");
    };

    let mut command = Command::new(program);
    command
        .args(args)
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("could not run `{}`", display_argv(argv)))?;

    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();

    // Read both pipes on worker threads so a large stdout cannot deadlock
    // against a full stderr buffer (or vice versa) while we wait.
    let stdout_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = stdout_pipe.as_mut() {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = stderr_pipe.as_mut() {
            let _ = pipe.read_to_end(&mut buf);
        }
        buf
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => {
                return Err(e).with_context(|| format!("`{}` failed", display_argv(argv)));
            }
        }
    };

    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();

    if status.is_none() {
        bail!(
            "`{}` timed out after {}s",
            display_argv(argv),
            timeout.as_secs()
        );
    }

    Ok(Output {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        status: status.and_then(|s| s.code()),
    })
}

/// Run `argv` and require a zero exit status, returning stdout.
pub fn run_checked(argv: &[&str]) -> Result<String> {
    let output = run(argv)?;
    if output.status != Some(0) {
        bail!("`{}` {}", display_argv(argv), output.error_summary());
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_argv_quotes_only_when_needed() {
        assert_eq!(
            display_argv(&["xcrun", "simctl", "delete", "ABC-123"]),
            "xcrun simctl delete ABC-123"
        );
        assert_eq!(display_argv(&["echo", "two words"]), "echo \"two words\"");
    }

    #[test]
    fn program_exists_finds_sh_but_not_nonsense() {
        assert!(program_exists("sh"));
        assert!(!program_exists("definitely-not-a-real-program-xyzzy"));
    }

    #[test]
    fn run_captures_stdout_and_status() {
        let out = run(&["echo", "hello"]).expect("echo runs");
        assert_eq!(out.status, Some(0));
        assert_eq!(out.stdout.trim(), "hello");
    }

    #[test]
    fn run_reports_nonzero_status_without_erroring() {
        let out = run(&["false"]).expect("false runs");
        assert_ne!(out.status, Some(0));
    }

    #[test]
    fn run_checked_errors_on_failure() {
        assert!(run_checked(&["false"]).is_err());
    }

    #[test]
    fn missing_program_is_an_error_not_a_panic() {
        assert!(run(&["definitely-not-a-real-program-xyzzy"]).is_err());
    }

    /// Arguments are passed verbatim: no shell is involved, so metacharacters
    /// are inert data rather than syntax.
    #[test]
    fn arguments_are_not_shell_interpreted() {
        let out = run(&["echo", "$HOME; rm -rf /"]).expect("echo runs");
        assert_eq!(out.stdout.trim(), "$HOME; rm -rf /");
    }

    #[test]
    fn timeout_kills_a_hanging_command() {
        let err = run_with(&["sleep", "30"], &[], Duration::from_millis(200))
            .expect_err("should time out");
        assert!(err.to_string().contains("timed out"), "got: {err}");
    }
}
