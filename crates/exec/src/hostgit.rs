//! Host Git, hardened. Every Git command the harness runs goes through [`git`].
//!
//! Threat model: the target repository (its `.git/config`, `.git/info/attributes`, committed
//! `.gitattributes`) is untrusted, and so is everything an agent wrote in the worktree. Host Git
//! runs outside the sandbox, so any repository-controlled way to make it start a program is
//! host code execution. [`HARDENING`] lists every such way relevant to the commands the harness
//! runs and what shuts it; command-line configuration takes precedence over every
//! configuration file. User and system configuration are never read, and the environment is
//! replaced, so `GIT_DIR`, `GIT_INDEX_FILE`, `GIT_SSH_COMMAND` and friends inherited from a
//! caller cannot redirect or arm a command.
//!
//! A task workspace is a linked worktree whose configuration comes from the source repository,
//! and that configuration (common `config`, or `config.worktree` under
//! `extensions.worktreeConfig`) can set `core.worktree` to any directory. Every Git command on a
//! workspace therefore goes through [`git_in`], which passes the worktree's own Git directory as
//! `--git-dir` and the workspace path as `--work-tree`. The command-line `--work-tree` takes
//! precedence over `core.worktree` from any configuration file, and an explicit `--git-dir`
//! skips discovery through the worktree's `.git` link.

use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

/// Absolute path, so a `PATH` entry cannot substitute another `git`.
const GIT: &str = "/usr/bin/git";

/// The SHA-1 empty tree. Git knows this object without it being stored, so it works as the
/// attribute source in any SHA-1 repository. A SHA-256 repository rejects it ("bad
/// --attr-source") and every command fails closed.
pub const EMPTY_TREE: &str = empty_tree!();

macro_rules! empty_tree {
    () => {
        "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
    };
}
use empty_tree;

/// One hardening measure applied to every host Git invocation.
enum Guard {
    /// A top-level option placed before the subcommand.
    Arg(&'static str),
    /// `-c key=value`: command-line configuration, read after every configuration file.
    Config(&'static str, &'static str),
    /// An environment variable (inherited by Git's own child processes, e.g. the
    /// `git reset --hard` that `worktree add` runs).
    Env(&'static str, &'static str),
}

/// Every repository- or worktree-controlled way to make host Git run a program, and the
/// measure that shuts it. Measures that are not a fixed entry here:
///
/// - Attribute drivers selected through `$GIT_DIR/info/attributes`, which `--attr-source`
///   does not cover: every `filter.*`, `diff.*` and `merge.*` driver defined in the effective
///   configuration is blanked per call (`driver_overrides`).
/// - `diff.external`, `diff.<driver>.command` and `diff.<driver>.textconv`: every `git diff`
///   the harness runs passes `--no-ext-diff --no-textconv` (an empty `diff.external` would make
///   Git try to run `""`, so it is not overridden here).
/// - `uploadpack.packObjectsHook`: honoured only from protected (system, global, command-line)
///   configuration; none is read and the environment is cleared, and no upload-pack runs.
/// - `core.gitProxy`: first match wins, so command-line configuration cannot override it;
///   `GIT_PROXY_COMMAND` set empty takes precedence and disables it, and `git://` is denied.
/// - `pager.<command>`: `--no-pager` (and stdout is never a terminal).
/// - Hooks run through `alias.*`: aliases cannot replace built-in commands, the only kind run.
const HARDENING: &[Guard] = &[
    // Attributes: nothing from any tree, index or worktree `.gitattributes`, no system or
    // global attributes file. Disables `filter=`, `diff=`, `merge=` selected in-tree.
    Guard::Arg(concat!("--attr-source=", empty_tree!())),
    Guard::Env("GIT_ATTR_SOURCE", EMPTY_TREE),
    Guard::Env("GIT_ATTR_NOSYSTEM", "1"),
    Guard::Config("core.attributesFile", "/dev/null"),
    // Hooks, file-system monitor, submodules (plan Task 7 rules).
    Guard::Config("core.hooksPath", "/dev/null"),
    Guard::Config("core.fsmonitor", "false"),
    Guard::Config("submodule.recurse", "false"),
    // Pagers ("cat" means no pager) and editors (":" means do not launch one).
    Guard::Arg("--no-pager"),
    Guard::Env("GIT_PAGER", "cat"),
    Guard::Config("core.pager", "cat"),
    Guard::Env("GIT_EDITOR", ":"),
    Guard::Config("core.editor", ":"),
    Guard::Config("sequence.editor", ":"),
    // Signing and signature display (`commit-tree` also gets `--no-gpg-sign`).
    Guard::Config("commit.gpgSign", "false"),
    Guard::Config("tag.gpgSign", "false"),
    Guard::Config("log.showSignature", "false"),
    Guard::Config("gpg.program", "/usr/bin/false"),
    Guard::Config("gpg.openpgp.program", "/usr/bin/false"),
    Guard::Config("gpg.x509.program", "/usr/bin/false"),
    Guard::Config("gpg.ssh.program", "/usr/bin/false"),
    Guard::Config("gpg.ssh.defaultKeyCommand", "/usr/bin/false"),
    // Transports: none at all. `protocol.<name>.allow` beats `protocol.allow`, so each
    // protocol a repository could re-enable is denied by name (`ext::` runs any command).
    // Lazy fetches of a partial clone would otherwise open a transport from any read.
    Guard::Config("protocol.allow", "never"),
    Guard::Config("protocol.file.allow", "never"),
    Guard::Config("protocol.git.allow", "never"),
    Guard::Config("protocol.ssh.allow", "never"),
    Guard::Config("protocol.http.allow", "never"),
    Guard::Config("protocol.https.allow", "never"),
    Guard::Config("protocol.ext.allow", "never"),
    Guard::Env("GIT_PROTOCOL_FROM_USER", "0"),
    Guard::Env("GIT_NO_LAZY_FETCH", "1"),
    Guard::Config("core.sshCommand", "/usr/bin/false"),
    Guard::Env("GIT_PROXY_COMMAND", ""),
    // Credentials and prompts: an empty helper value resets the helper list.
    Guard::Config("credential.helper", ""),
    Guard::Config("core.askPass", ""),
    Guard::Env("GIT_TERMINAL_PROMPT", "0"),
    Guard::Arg("--no-optional-locks"),
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
    /// The repository's configuration defines a `filter`, `diff` or `merge` driver whose name
    /// is not valid UTF-8, so it cannot be blanked reliably; nothing was run.
    #[error("repository configuration has a driver name that is not UTF-8: {0:?}")]
    NonUtf8DriverName(String),
}

/// Where a Git command runs: `-C <dir>`, plus `--git-dir`/`--work-tree` when pinned.
#[derive(Clone, Copy)]
struct Target<'a> {
    dir: &'a Path,
    /// `(git_dir, work_tree)`.
    pin: Option<(&'a Path, &'a Path)>,
}

/// A linked worktree's own Git directory and its validated work tree.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pinned<'a> {
    pub(crate) git_dir: &'a Path,
    pub(crate) work_tree: &'a Path,
}

impl<'a> Pinned<'a> {
    fn target(self) -> Target<'a> {
        Target {
            dir: self.work_tree,
            pin: Some((self.git_dir, self.work_tree)),
        }
    }
}

/// Runs `git -C <repo> <hardening> <args>` and returns its stdout.
pub async fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
    git_target(
        Target {
            dir: repo,
            pin: None,
        },
        args,
        &[],
    )
    .await
}

/// [`git`] bound to a workspace: `-C <work_tree> --git-dir=<git_dir> --work-tree=<work_tree>`.
pub(crate) async fn git_in(wt: Pinned<'_>, args: &[&str]) -> Result<Vec<u8>, GitError> {
    git_target(wt.target(), args, &[]).await
}

/// [`git_in`] with extra environment variables (identity and dates for `commit-tree`).
pub(crate) async fn git_in_with_env(
    wt: Pinned<'_>,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> Result<Vec<u8>, GitError> {
    git_target(wt.target(), args, extra_env).await
}

async fn git_target(
    repo: Target<'_>,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> Result<Vec<u8>, GitError> {
    let drivers = driver_overrides(repo).await?;
    let mut env: Vec<(String, String)> =
        vec![("GIT_CONFIG_COUNT".into(), drivers.len().to_string())];
    for (i, (key, value)) in drivers.into_iter().enumerate() {
        env.push((format!("GIT_CONFIG_KEY_{i}"), key));
        env.push((format!("GIT_CONFIG_VALUE_{i}"), value));
    }
    env.extend(extra_env.iter().map(|&(k, v)| (k.to_owned(), v.to_owned())));
    run(repo, args, &env).await
}

/// `-c` cannot name a driver whose name contains `=`, so the overrides travel as
/// `GIT_CONFIG_KEY_<n>`/`GIT_CONFIG_VALUE_<n>` (same command-line scope).
///
/// Every `filter.<name>`, `diff.<name>` and `merge.<name>` driver defined in the effective
/// configuration (includes resolved) gets its program keys set empty, which Git treats as "no
/// program", and `filter.<name>.required=false`, so no attribute source at all, including
/// `$GIT_DIR/info/attributes`, can select a program. A driver name that is not valid UTF-8
/// cannot be passed back as a key reliably, so it fails the call with `NonUtf8DriverName`
/// before anything runs. The configuration is re-read on every
/// call; it can only change between this read and the command if something writes the
/// repository's configuration (or a file it includes) meanwhile.
async fn driver_overrides(repo: Target<'_>) -> Result<Vec<(String, String)>, GitError> {
    let args = [
        "config",
        "-z",
        "--name-only",
        "--get-regexp",
        r"^(filter|diff|merge)\.",
    ];
    let names = match run(repo, &args, &[]).await {
        Ok(out) => out,
        // Exit 1: no matching key.
        Err(GitError::Failed { code: Some(1), .. }) => Vec::new(),
        Err(e) => return Err(e),
    };
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for name in names.split(|&b| b == 0).filter(|n| !n.is_empty()) {
        // A lossy name would blank a key with U+FFFD in it, leaving the real driver armed.
        let Ok(name) = std::str::from_utf8(name) else {
            return Err(GitError::NonUtf8DriverName(
                String::from_utf8_lossy(name).into_owned(),
            ));
        };
        // `<section>.<subsection>.<key>`; keys without a subsection name no driver.
        let Some((section, rest)) = name.split_once('.') else {
            continue;
        };
        let Some((sub, _key)) = rest.rsplit_once('.') else {
            continue;
        };
        if !seen.insert((section.to_owned(), sub.to_owned())) {
            continue;
        }
        let blank: &[(&str, &str)] = match section {
            "filter" => &[
                ("clean", ""),
                ("smudge", ""),
                ("process", ""),
                ("required", "false"),
            ],
            "diff" => &[("textconv", ""), ("command", "")],
            "merge" => &[("driver", "")],
            _ => &[],
        };
        out.extend(
            blank
                .iter()
                .map(|(k, v)| (format!("{section}.{sub}.{k}"), (*v).to_owned())),
        );
    }
    Ok(out)
}

async fn run(
    repo: Target<'_>,
    args: &[&str],
    env: &[(String, String)],
) -> Result<Vec<u8>, GitError> {
    let mut cmd = Command::new(GIT);
    cmd.arg("-C").arg(repo.dir);
    if let Some((git_dir, work_tree)) = repo.pin {
        let mut d = std::ffi::OsString::from("--git-dir=");
        d.push(git_dir);
        let mut w = std::ffi::OsString::from("--work-tree=");
        w.push(work_tree);
        cmd.arg(d).arg(w);
    }
    for guard in HARDENING {
        match guard {
            Guard::Arg(a) => {
                cmd.arg(a);
            }
            Guard::Config(k, v) => {
                cmd.arg("-c").arg(format!("{k}={v}"));
            }
            Guard::Env(..) => {}
        }
    }
    cmd.args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent")
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    for guard in HARDENING {
        if let Guard::Env(k, v) = guard {
            cmd.env(k, v);
        }
    }
    cmd.envs(env.iter().map(|(k, v)| (k, v)))
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
