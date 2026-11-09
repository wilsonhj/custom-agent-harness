//! Host Git, hardened. Every Git command the harness runs goes through [`git`].
//!
//! The repository's own `.git/config` is still read (it belongs to the owner), but the
//! settings that would run code chosen by someone else are overridden on the command line,
//! which takes precedence over every configuration file. User and system configuration are
//! never read, and the environment is replaced, so `GIT_DIR`, `GIT_INDEX_FILE` and friends
//! inherited from a caller cannot redirect a command.

use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

/// Absolute path, so a `PATH` entry cannot substitute another `git`.
const GIT: &str = "/usr/bin/git";

/// Overrides placed before every subcommand (plan Task 7 rules).
const HARDENING: &[&str] = &[
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "protocol.file.allow=never",
    "-c",
    "submodule.recurse=false",
    "--no-optional-locks",
];

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("could not run git: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("git {args} failed ({code:?}): {stderr}")]
    Failed {
        args: String,
        code: Option<i32>,
        stderr: String,
    },
}

/// Runs `git -C <repo> <hardening> <args>` and returns its stdout.
pub async fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
    git_with_env(repo, args, &[]).await
}

/// [`git`] with extra environment variables (identity and dates for `commit-tree`).
pub(crate) async fn git_with_env(
    repo: &Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> Result<Vec<u8>, GitError> {
    let mut cmd = Command::new(GIT);
    cmd.arg("-C")
        .arg(repo)
        .args(HARDENING)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent")
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .envs(extra_env.iter().copied())
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let out = cmd.output().await?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(GitError::Failed {
            args: args.join(" "),
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).trim_end().to_owned(),
        })
    }
}
