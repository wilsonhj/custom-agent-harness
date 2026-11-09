//! macOS Seatbelt sandbox: profile generation, fail-closed probe, and launch-plan wrapping.
//!
//! Evidence: `docs/superpowers/specs/research-2026-11-09/1a-macos-sandbox.md` (probed profile,
//! §1) and design §6.1. The profile below is written for this project; nothing is copied from
//! other projects.
//!
//! Two steps, kept apart so each can be tested:
//!
//! 1. [`resolve`] turns a [`SandboxProfile`] into a [`ResolvedProfile`] whose paths are real
//!    paths (`/tmp/x` becomes `/private/tmp/x`, symlinks are followed). A rule written with an
//!    alias does not match an access through the real path, so an unresolved path would make a
//!    deny rule fail open. A path that cannot be resolved is an error, never a skipped rule.
//! 2. [`ResolvedProfile::render`] is pure text generation. Paths never appear in the text:
//!    each one is a `(param "NAME")` reference whose value travels as a `-D NAME=<path>`
//!    argument ([`ResolvedProfile::params`]), so quotes, spaces or parentheses in a path cannot
//!    change the profile.
//!
//! Path rules follow one principle: **the most specific path wins; on a tie, deny wins.**
//! Seatbelt applies the last matching rule, so path rules are emitted from the shortest path to
//! the longest. This lets a deny on a parent (a repositories root, the state directory) coexist
//! with an allow on a child (the worktree, a task home stored under the state directory), and a
//! deny on a grandchild (`<worktree>/.claude`) win again.

use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use harness_core::outcome::FailureCode;

use crate::plan::LaunchPlan;

/// The only `sandbox-exec` the harness runs. Absolute, so a binary earlier on `PATH` is never
/// used; `/usr/bin` is protected by System Integrity Protection.
pub const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// Mach services a sandboxed run may look up, beyond those Apple's `system.sb` (imported by
/// every profile) already allows.
///
/// This is the probed minimum (research 1a §1, "Mach service lookups") with which
/// `claude --version`, `codex --version`, `cursor-agent --version`, `curl` over TLS and Node
/// `fetch` over TLS all worked through the egress proxy. Directory lookups (user and group
/// names), certificate trust evaluation, the keychain server (subscription logins, design §6.3)
/// and per-user temporary directories need them. Keep the list tight: every allowed service is
/// an IPC channel out of the sandbox.
///
/// File reads are not listed here: the profile allows every read and then denies specific
/// trees (secrets, state, Git metadata, repository agent configuration), because the CLIs read
/// widely across the system to start.
pub const MACH_LOOKUP_ALLOW: &[&str] = &[
    "com.apple.system.opendirectoryd.libinfo",
    "com.apple.system.opendirectoryd.membership",
    "com.apple.trustd",
    "com.apple.trustd.agent",
    "com.apple.SecurityServer",
    "com.apple.ocspd",
    "com.apple.bsd.dirhelper",
];

/// Profile used by [`probe`]: everything denied except running `/usr/bin/true`.
const PROBE_PROFILE: &str =
    "(version 1)(deny default)(allow file-read*)(allow process-exec (literal \"/usr/bin/true\"))";

/// Network reachable from inside the sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    /// No network at all, loopback included.
    None,
    /// Outbound TCP to `localhost:<port>` only, where the harness egress proxy listens.
    ProxyOnly { port: u16 },
}

/// What a run may touch. Paths may be given in any spelling; they are resolved before use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxProfile {
    /// The run's working tree. Must exist. Readable; writable iff `worktree_writable`.
    pub worktree: PathBuf,
    pub worktree_writable: bool,
    /// Always readable and writable (the run's HOME / TMPDIR / CLI config). Must exist.
    pub task_home: PathBuf,
    /// Trees that may be neither read nor written. They need not exist yet: a missing path is
    /// resolved through its nearest existing ancestor so it is protected before it is created.
    pub read_deny: Vec<PathBuf>,
    /// Trees re-allowed for reading inside a denied tree. Must exist.
    pub read_allow_extra: Vec<PathBuf>,
    pub network: Network,
}

/// A [`SandboxProfile`] with every path resolved to a real, absolute, UTF-8 path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProfile {
    pub worktree: String,
    pub worktree_writable: bool,
    pub task_home: String,
    pub read_deny: Vec<String>,
    pub read_allow_extra: Vec<String>,
    pub network: Network,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxInfo {
    pub sandbox_exec: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileError {
    #[error("cannot resolve sandbox path {path:?}: {reason}")]
    Unresolvable { path: PathBuf, reason: String },
}

/// Fails closed: `Err(SandboxUnavailable)` unless `/usr/bin/sandbox-exec` exists and can apply
/// a deny-default profile around `/usr/bin/true`. It also fails when the harness itself already
/// runs inside a sandbox, because Seatbelt profiles do not nest (research 1a §4).
pub fn probe() -> Result<SandboxInfo, FailureCode> {
    probe_at(Path::new(SANDBOX_EXEC))
}

fn probe_at(sandbox_exec: &Path) -> Result<SandboxInfo, FailureCode> {
    let status = Command::new(sandbox_exec)
        .args(["-p", PROBE_PROFILE, "--", "/usr/bin/true"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => Ok(SandboxInfo {
            sandbox_exec: sandbox_exec.to_path_buf(),
        }),
        _ => Err(FailureCode::SandboxUnavailable),
    }
}

/// Resolves the profile's paths, then renders the profile text.
pub fn render_profile(p: &SandboxProfile) -> Result<String, ProfileError> {
    Ok(resolve(p)?.render())
}

/// Wraps `plan` so it runs as `/usr/bin/sandbox-exec -p <profile> -D ... -- <argv>`. With
/// [`Network::ProxyOnly`], points the standard proxy variables at the egress proxy.
pub fn wrap(p: &SandboxProfile, plan: LaunchPlan) -> Result<LaunchPlan, ProfileError> {
    let resolved = resolve(p)?;
    let mut argv = vec![
        SANDBOX_EXEC.to_string(),
        "-p".to_string(),
        resolved.render(),
    ];
    for (key, value) in resolved.params() {
        argv.push("-D".to_string());
        argv.push(format!("{key}={value}"));
    }
    argv.push("--".to_string());
    argv.extend(plan.argv);

    let mut env = plan.env;
    if let Network::ProxyOnly { port } = resolved.network {
        let url = format!("http://127.0.0.1:{port}");
        let added = [
            ("HTTPS_PROXY", url.as_str()),
            ("HTTP_PROXY", url.as_str()),
            ("NODE_USE_ENV_PROXY", "1"),
        ];
        env.retain(|(k, _)| added.iter().all(|(a, _)| a != k));
        env.extend(added.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    }

    Ok(LaunchPlan {
        argv,
        env,
        cwd: plan.cwd,
        stdin: plan.stdin,
    })
}

/// Reads the run must never see (design §6.1): the user's SSH and AWS secrets, the harness
/// state directory, all Git metadata, and repository agent configuration that a CLI would
/// execute (`.claude/`, `.mcp.json`, `.codex/`, `.agents/`, `.cursor/`). The same paths are
/// also never writable. Paths are returned as given; [`resolve`] canonicalises them.
pub fn default_read_deny(
    home: &Path,
    state_dir: &Path,
    worktree: &Path,
    git_common_dir: &Path,
) -> Vec<PathBuf> {
    let mut deny = vec![
        home.join(".ssh"),
        home.join(".aws"),
        state_dir.to_path_buf(),
        git_common_dir.to_path_buf(),
    ];
    deny.extend(
        [
            ".git",
            ".claude",
            ".mcp.json",
            ".codex",
            ".agents",
            ".cursor",
        ]
        .iter()
        .map(|name| worktree.join(name)),
    );
    deny
}

/// Resolves every path in `p` to its real path. See the module documentation.
pub fn resolve(p: &SandboxProfile) -> Result<ResolvedProfile, ProfileError> {
    Ok(ResolvedProfile {
        worktree: resolve_existing(&p.worktree)?,
        worktree_writable: p.worktree_writable,
        task_home: resolve_existing(&p.task_home)?,
        read_deny: p
            .read_deny
            .iter()
            .map(|d| resolve_maybe_missing(d))
            .collect::<Result<_, _>>()?,
        read_allow_extra: p
            .read_allow_extra
            .iter()
            .map(|a| resolve_existing(a))
            .collect::<Result<_, _>>()?,
        network: p.network,
    })
}

fn unresolvable(path: &Path, reason: impl Into<String>) -> ProfileError {
    ProfileError::Unresolvable {
        path: path.to_path_buf(),
        reason: reason.into(),
    }
}

fn into_utf8(original: &Path, real: PathBuf) -> Result<String, ProfileError> {
    real.into_os_string()
        .into_string()
        .map_err(|_| unresolvable(original, "real path is not UTF-8"))
}

fn resolve_existing(path: &Path) -> Result<String, ProfileError> {
    if !path.is_absolute() {
        return Err(unresolvable(path, "not absolute"));
    }
    let real = fs::canonicalize(path).map_err(|e| unresolvable(path, e.to_string()))?;
    into_utf8(path, real)
}

/// Resolves the nearest existing ancestor and appends the missing tail, which must consist of
/// plain names. A dangling symlink is refused: its target could appear later.
fn resolve_maybe_missing(path: &Path) -> Result<String, ProfileError> {
    if !path.is_absolute() {
        return Err(unresolvable(path, "not absolute"));
    }
    let components: Vec<Component<'_>> = path.components().collect();
    for split in (1..=components.len()).rev() {
        let prefix: PathBuf = components[..split].iter().collect();
        match fs::canonicalize(&prefix) {
            Ok(mut real) => {
                for c in &components[split..] {
                    match c {
                        Component::Normal(name) => real.push(name),
                        _ => {
                            return Err(unresolvable(path, "missing part contains `.` or `..`"));
                        }
                    }
                }
                return into_utf8(path, real);
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                if fs::symlink_metadata(&prefix).is_ok() {
                    return Err(unresolvable(path, "dangling symlink"));
                }
            }
            Err(e) => return Err(unresolvable(path, e.to_string())),
        }
    }
    Err(unresolvable(path, "no existing ancestor"))
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rule {
    Allow,
    Deny,
}

impl ResolvedProfile {
    /// `-D` parameters referenced by [`render`](Self::render), in a stable order.
    pub fn params(&self) -> Vec<(String, String)> {
        let mut params = vec![
            ("WT".to_string(), self.worktree.clone()),
            ("GITFILE".to_string(), format!("{}/.git", self.worktree)),
            ("TASKHOME".to_string(), self.task_home.clone()),
        ];
        for (i, d) in self.read_deny.iter().enumerate() {
            params.push((format!("DENY_{i}"), d.clone()));
        }
        for (i, a) in self.read_allow_extra.iter().enumerate() {
            params.push((format!("ALLOW_{i}"), a.clone()));
        }
        if let Network::ProxyOnly { port } = self.network {
            params.push(("PROXY_PORT".to_string(), port.to_string()));
        }
        params
    }

    /// The SBPL profile text. Pure: contains parameter names, never paths.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("(version 1)\n(deny default)\n");
        out.push_str("; Apple's baseline for system libraries, dyld and standard daemons.\n");
        out.push_str("(import \"system.sb\")\n\n");

        out.push_str("; Processes: own process tree only.\n");
        out.push_str("(allow process-fork process-exec*)\n");
        out.push_str("(allow process-info* (target same-sandbox))\n");
        out.push_str("(allow signal (target same-sandbox))\n");
        out.push_str("(allow sysctl-read)\n");
        out.push_str("(allow ipc-posix*)\n");
        out.push_str("(allow pseudo-tty)\n");
        out.push_str("(allow file-ioctl)\n");
        out.push_str("; MACH_LOOKUP_ALLOW\n(allow mach-lookup (global-name");
        for name in MACH_LOOKUP_ALLOW {
            out.push_str(&format!(" \"{name}\""));
        }
        out.push_str("))\n\n");

        let path_rules = |op: &str, rules: Vec<(&String, &str, Rule)>, out: &mut String| {
            let mut rules = rules;
            // Most specific path last (Seatbelt: last match wins); on a tie, deny last.
            rules.sort_by_key(|(path, _, rule)| {
                (Path::new(path.as_str()).components().count(), *rule)
            });
            for (_, param, rule) in rules {
                let verb = match rule {
                    Rule::Allow => "allow",
                    Rule::Deny => "deny",
                };
                out.push_str(&format!("({verb} {op} (subpath (param \"{param}\")))\n"));
            }
        };

        let deny_names: Vec<String> = (0..self.read_deny.len())
            .map(|i| format!("DENY_{i}"))
            .collect();
        let allow_names: Vec<String> = (0..self.read_allow_extra.len())
            .map(|i| format!("ALLOW_{i}"))
            .collect();
        let gitfile = format!("{}/.git", self.worktree);

        out.push_str("; Reads: everything, then the most specific path rule wins.\n");
        out.push_str("(allow file-read*)\n");
        let mut reads = vec![
            (&self.worktree, "WT", Rule::Allow),
            (&self.task_home, "TASKHOME", Rule::Allow),
        ];
        reads.extend(
            self.read_allow_extra
                .iter()
                .zip(&allow_names)
                .map(|(p, n)| (p, n.as_str(), Rule::Allow)),
        );
        reads.extend(
            self.read_deny
                .iter()
                .zip(&deny_names)
                .map(|(p, n)| (p, n.as_str(), Rule::Deny)),
        );
        path_rules("file-read*", reads, &mut out);
        out.push('\n');

        out.push_str(
            "; Writes: task home, the worktree if writable, never Git metadata or denied trees.\n",
        );
        out.push_str("(allow file-write* (literal \"/dev/null\") (literal \"/dev/tty\") (regex #\"^/dev/ttys[0-9]+$\"))\n");
        let mut writes = vec![(&self.task_home, "TASKHOME", Rule::Allow)];
        if self.worktree_writable {
            writes.push((&self.worktree, "WT", Rule::Allow));
        }
        writes.push((&gitfile, "GITFILE", Rule::Deny));
        writes.extend(
            self.read_deny
                .iter()
                .zip(&deny_names)
                .map(|(p, n)| (p, n.as_str(), Rule::Deny)),
        );
        path_rules("file-write*", writes, &mut out);
        out.push_str("; The roots themselves cannot be removed or replaced.\n");
        out.push_str(
            "(deny file-write-unlink (literal (param \"WT\")) (literal (param \"TASKHOME\")))\n\n",
        );

        match self.network {
            Network::None => out.push_str("; Network: none (no rule allows any network access).\n"),
            Network::ProxyOnly { .. } => {
                out.push_str("; Network: the harness egress proxy only.\n");
                out.push_str(
                    "(allow network-outbound (remote tcp (string-append \"localhost:\" (param \"PROXY_PORT\"))))\n",
                );
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_sandbox_exec_is_sandbox_unavailable() {
        assert_eq!(
            probe_at(Path::new("/nonexistent/sandbox-exec")),
            Err(FailureCode::SandboxUnavailable)
        );
    }

    #[test]
    fn failing_sandbox_exec_is_sandbox_unavailable() {
        assert_eq!(
            probe_at(Path::new("/usr/bin/false")),
            Err(FailureCode::SandboxUnavailable)
        );
    }
}
