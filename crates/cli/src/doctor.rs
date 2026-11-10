//! `eng doctor`: does this Mac satisfy what a run needs? Nothing here calls a model.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use harness_adapters::adapter_for;
use harness_core::types::RuntimeKind;
use harness_exec::sandbox_macos;
use harness_exec::supervisor::descendant_tracking_available;
use serde_json::{Value, json};

use crate::runtimes::Resolved;

pub struct Check {
    pub name: &'static str,
    /// A stable machine-readable reason when the check fails.
    pub code: Option<&'static str>,
    pub detail: String,
}

impl Check {
    fn pass(name: &'static str, detail: impl Into<String>) -> Check {
        Check {
            name,
            code: None,
            detail: detail.into(),
        }
    }
    fn fail(name: &'static str, code: &'static str, detail: impl Into<String>) -> Check {
        Check {
            name,
            code: Some(code),
            detail: detail.into(),
        }
    }
    pub fn ok(&self) -> bool {
        self.code.is_none()
    }
    pub fn line(&self) -> String {
        match self.code {
            None => format!("ok    {:<20} {}", self.name, self.detail),
            Some(c) => format!("FAIL  {:<20} {c}: {}", self.name, self.detail),
        }
    }
    pub fn to_json(&self) -> Value {
        json!({"name": self.name, "ok": self.ok(), "code": self.code, "detail": self.detail})
    }
}

pub fn run_checks(state_dir: &Path, r: &Resolved) -> Vec<Check> {
    vec![
        sandbox(),
        descendants(),
        version(RuntimeKind::Claude, r),
        version(RuntimeKind::Codex, r),
        shim(&r.shim),
        state_dir_check(state_dir),
    ]
}

fn sandbox() -> Check {
    match sandbox_macos::probe() {
        Ok(_) => Check::pass("sandbox", "sandbox-exec applies a deny-default profile"),
        Err(_) => Check::fail(
            "sandbox",
            "sandbox_unavailable",
            "/usr/bin/sandbox-exec cannot apply a profile here (missing, or already inside a sandbox)",
        ),
    }
}

fn descendants() -> Check {
    if descendant_tracking_available() {
        Check::pass(
            "descendant_tracking",
            "process-table scan and responsible-process link work",
        )
    } else {
        Check::fail(
            "descendant_tracking",
            "descendant_tracking_unavailable",
            "cannot enumerate descendants; the harness would not be able to prove a process tree gone",
        )
    }
}

fn version(kind: RuntimeKind, r: &Resolved) -> Check {
    let (name, bin) = match kind {
        RuntimeKind::Claude => ("claude_version", &r.claude),
        RuntimeKind::Codex => ("codex_version", &r.codex),
    };
    let bin = match bin {
        Ok(b) => b,
        Err(e) => return Check::fail(name, "runtime_not_found", e.clone()),
    };
    let adapter = adapter_for(kind);
    let mut argv: Vec<String> = Vec::new();
    match (kind, &r.node) {
        (RuntimeKind::Codex, Ok(node)) => {
            argv.push(node.to_string_lossy().into_owned());
            argv.push(bin.to_string_lossy().into_owned());
        }
        _ => argv.push(bin.to_string_lossy().into_owned()),
    }
    argv.extend(adapter.version_argv().into_iter().skip(1));
    let out = match run_version(&argv, r.node.as_ref().ok().and_then(|n| n.parent())) {
        Ok(o) => o,
        Err(e) => return Check::fail(name, "spawn_error", e),
    };
    let pinned = adapter.capabilities().pinned_version;
    match adapter.check_version(&out) {
        Ok(()) => Check::pass(name, format!("{} (pinned {pinned})", out.trim())),
        Err(_) => Check::fail(
            name,
            "version_mismatch",
            format!("found {:?}, pinned {pinned}", out.trim()),
        ),
    }
}

/// `<bin> --version` with a clean environment and a deadline.
fn run_version(argv: &[String], node_dir: Option<&Path>) -> Result<String, String> {
    let mut path = String::new();
    if let Some(d) = node_dir {
        path.push_str(&d.to_string_lossy());
        path.push(':');
    }
    path.push_str("/usr/bin:/bin");
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .env_clear()
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(h) = std::env::var_os("HOME") {
        cmd.env("HOME", h);
    }
    let mut child = cmd.spawn().map_err(|e| format!("{}: {e}", argv[0]))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("`--version` did not finish in 10 s".into());
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn shim(path: &PathBuf) -> Check {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() && m.permissions().mode() & 0o111 != 0 => {
            Check::pass("shim", path.display().to_string())
        }
        Ok(_) => Check::fail(
            "shim",
            "shim_missing",
            format!("{} is not an executable file", path.display()),
        ),
        Err(_) => Check::fail(
            "shim",
            "shim_missing",
            format!(
                "{} not found (build it: cargo build --workspace)",
                path.display()
            ),
        ),
    }
}

fn state_dir_check(dir: &Path) -> Check {
    match std::fs::metadata(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Check::pass(
            "state_dir",
            format!(
                "{} (absent; created on first use, mode 0700)",
                dir.display()
            ),
        ),
        Err(e) => Check::fail(
            "state_dir",
            "state_dir_unusable",
            format!("{}: {e}", dir.display()),
        ),
        Ok(m) => {
            let mode = m.permissions().mode() & 0o777;
            if !m.is_dir() {
                Check::fail(
                    "state_dir",
                    "state_dir_unusable",
                    format!("{} is not a directory", dir.display()),
                )
            } else if mode != 0o700 {
                Check::fail(
                    "state_dir",
                    "state_dir_permissions",
                    format!("{} has mode {mode:o}, want 700", dir.display()),
                )
            } else {
                Check::pass("state_dir", format!("{} (mode 700)", dir.display()))
            }
        }
    }
}
