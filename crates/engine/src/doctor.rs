//! `doctor`: does this Mac satisfy what a run needs? Nothing here calls a model.
//!
//! Each runtime's `--version` goes through the launch path a real agent run uses: the
//! resolved real path, a read-only sandbox profile on a throwaway empty directory, no
//! network, the gating shim and the supervisor. A passing check therefore also proves the
//! runtime can start inside the harness sandbox.

use std::fs;
use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use harness_adapters::adapter_for;
use harness_core::types::RuntimeKind;
use harness_exec::LaunchPlan;
use harness_exec::sandbox_macos::{self, Network, SandboxProfile};
use harness_exec::supervisor::{self, Limits, LineSink, Stream, descendant_tracking_available};
use tokio_util::sync::CancellationToken;

use crate::worker::{launch_for, path_env, runtime_name};
use crate::{EngineConfig, new_id};

const VERSION_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

/// One line of `doctor`. A failing check's `detail` starts with a stable machine-readable
/// code, `<code>: <explanation>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorCheck {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

fn pass(name: &'static str, detail: impl Into<String>) -> DoctorCheck {
    DoctorCheck {
        name,
        ok: true,
        detail: detail.into(),
    }
}

fn fail(name: &'static str, code: &str, detail: impl AsRef<str>) -> DoctorCheck {
    DoctorCheck {
        name,
        ok: false,
        detail: format!("{code}: {}", detail.as_ref()),
    }
}

/// Runs every check, in a fixed order: sandbox, descendant tracking, both runtime versions,
/// the shim, the state directory.
pub async fn doctor(cfg: &EngineConfig) -> Vec<DoctorCheck> {
    vec![
        sandbox(),
        descendants(),
        version(cfg, RuntimeKind::Claude).await,
        version(cfg, RuntimeKind::Codex).await,
        shim(&cfg.shim_bin),
        state_dir(&cfg.state_dir),
    ]
}

fn sandbox() -> DoctorCheck {
    match sandbox_macos::probe() {
        Ok(_) => pass("sandbox", "sandbox-exec applies a deny-default profile"),
        Err(_) => fail(
            "sandbox",
            "sandbox_unavailable",
            "/usr/bin/sandbox-exec cannot apply a profile here (missing, or already inside a sandbox)",
        ),
    }
}

fn descendants() -> DoctorCheck {
    if descendant_tracking_available() {
        pass(
            "descendant_tracking",
            "process-table scan and responsible-process link work",
        )
    } else {
        fail(
            "descendant_tracking",
            "descendant_tracking_unavailable",
            "cannot enumerate descendants; the harness would not be able to prove a process tree gone",
        )
    }
}

struct Collect {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl LineSink for Collect {
    fn line(&mut self, stream: Stream, bytes: &[u8]) {
        let buf = match stream {
            Stream::Stdout => &mut self.stdout,
            Stream::Stderr => &mut self.stderr,
        };
        buf.extend_from_slice(bytes);
        buf.push(b'\n');
    }
}

/// Removes the throwaway directory when dropped.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scratch() -> std::io::Result<Scratch> {
    let root = fs::canonicalize(std::env::temp_dir())?.join(format!("eng-doctor-{}", new_id()));
    fs::DirBuilder::new().mode(0o700).create(&root)?;
    let s = Scratch(root);
    for sub in ["worktree", "home"] {
        fs::DirBuilder::new().mode(0o700).create(s.0.join(sub))?;
    }
    Ok(s)
}

async fn version(cfg: &EngineConfig, kind: RuntimeKind) -> DoctorCheck {
    let name = match kind {
        RuntimeKind::Claude => "claude_version",
        RuntimeKind::Codex => "codex_version",
    };
    let bin = match kind {
        RuntimeKind::Claude => &cfg.claude_bin,
        RuntimeKind::Codex => &cfg.codex_bin,
    };
    if !bin.is_absolute() {
        return fail(
            name,
            "runtime_not_found",
            format!(
                "`{}` not found (set ENG_{}_BIN, or put it on PATH)",
                runtime_name(kind),
                runtime_name(kind).to_uppercase()
            ),
        );
    }
    let adapter = adapter_for(kind);
    let pinned = adapter.capabilities().pinned_version;
    let launch = match launch_for(cfg, kind) {
        Ok(l) => l,
        Err(_) => {
            return fail(
                name,
                "spawn_error",
                format!("{} cannot be resolved", bin.display()),
            );
        }
    };
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return fail(name, "spawn_error", "HOME is not set");
    };
    let scratch = match scratch() {
        Ok(s) => s,
        Err(e) => return fail(name, "spawn_error", format!("scratch directory: {e}")),
    };
    let (worktree, task_home) = (scratch.0.join("worktree"), scratch.0.join("home"));
    let mut argv = launch.prefix;
    argv.extend(adapter.version_argv().into_iter().skip(1));
    let plan = LaunchPlan {
        argv,
        env: vec![
            ("PATH".into(), path_env(cfg)),
            ("HOME".into(), task_home.to_string_lossy().into_owned()),
            ("TMPDIR".into(), task_home.to_string_lossy().into_owned()),
        ],
        cwd: task_home.clone(),
        stdin: Vec::new(),
    };
    let profile = SandboxProfile {
        read_deny: sandbox_macos::default_read_deny(
            &home,
            &cfg.state_dir,
            &worktree,
            &worktree.join(".git"),
        ),
        worktree,
        worktree_writable: false,
        task_home,
        // The runtime's login directory only exists after `eng login`; doctor must not create
        // state, and a runtime can start without it.
        read_allow_extra: launch
            .read_dirs
            .into_iter()
            .filter(|d| d.exists())
            .collect(),
        network: Network::None,
    };
    let wrapped = match sandbox_macos::wrap(&profile, plan) {
        Ok(w) => w,
        Err(e) => return fail(name, "spawn_error", format!("sandbox profile: {e}")),
    };
    let limits = Limits {
        deadline: VERSION_DEADLINE,
        max_output_bytes: 1024 * 1024,
        max_line_bytes: 64 * 1024,
    };
    let child = match supervisor::spawn_gated(&cfg.shim_bin, wrapped, limits, |_| Ok(())).await {
        Ok(c) => c,
        Err(_) => {
            return fail(
                name,
                "spawn_error",
                format!("could not start the gate shim {}", cfg.shim_bin.display()),
            );
        }
    };
    let mut sink = Collect {
        stdout: Vec::new(),
        stderr: Vec::new(),
    };
    let (exit, killed) = child.run(&mut sink, CancellationToken::new()).await;
    if killed.is_err() || exit.code != Some(0) {
        let stderr = String::from_utf8_lossy(&sink.stderr);
        return fail(
            name,
            "spawn_error",
            format!(
                "`{} --version` inside the sandbox exited {:?}: {}",
                runtime_name(kind),
                exit.code,
                stderr.trim().chars().take(300).collect::<String>()
            ),
        );
    }
    let out = String::from_utf8_lossy(&sink.stdout);
    match adapter.check_version(&out) {
        Ok(()) => pass(
            name,
            format!("{} (pinned {pinned}; ran inside the sandbox)", out.trim()),
        ),
        Err(_) => fail(
            name,
            "version_mismatch",
            format!("found {:?}, pinned {pinned}", out.trim()),
        ),
    }
}

fn shim(path: &Path) -> DoctorCheck {
    match fs::metadata(path) {
        Ok(m) if m.is_file() && m.permissions().mode() & 0o111 != 0 => {
            pass("shim", path.display().to_string())
        }
        Ok(_) => fail(
            "shim",
            "shim_missing",
            format!("{} is not an executable file", path.display()),
        ),
        Err(_) => fail(
            "shim",
            "shim_missing",
            format!(
                "{} not found (build it: cargo build --workspace)",
                path.display()
            ),
        ),
    }
}

fn state_dir(dir: &Path) -> DoctorCheck {
    match fs::metadata(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => pass(
            "state_dir",
            format!(
                "{} (absent; created on first use, mode 0700)",
                dir.display()
            ),
        ),
        Err(e) => fail(
            "state_dir",
            "state_dir_unusable",
            format!("{}: {e}", dir.display()),
        ),
        Ok(m) => {
            let mode = m.permissions().mode() & 0o777;
            if !m.is_dir() {
                fail(
                    "state_dir",
                    "state_dir_unusable",
                    format!("{} is not a directory", dir.display()),
                )
            } else if mode != 0o700 {
                fail(
                    "state_dir",
                    "state_dir_permissions",
                    format!("{} has mode {mode:o}, want 700", dir.display()),
                )
            } else {
                pass("state_dir", format!("{} (mode 700)", dir.display()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failing_checks_lead_with_a_code() {
        let c = shim(Path::new("/nonexistent/eng-shim"));
        assert!(!c.ok);
        assert!(c.detail.starts_with("shim_missing: "), "{}", c.detail);
        let d = state_dir(Path::new("/nonexistent/eng-state"));
        assert!(d.ok, "{}", d.detail);
    }
}
