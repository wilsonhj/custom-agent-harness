//! Finding the runtimes, the shim and the extra read directories. Every path is canonicalised
//! because the sandbox denies `$HOME`, where `claude` and `nvm` install through symlinks.

use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use crate::{EngineConfig, EngineError};

pub const ENV_CLAUDE: &str = "ENG_CLAUDE_BIN";
pub const ENV_CODEX: &str = "ENG_CODEX_BIN";
pub const ENV_NODE: &str = "ENG_NODE_BIN";
pub const ENV_SHIM: &str = "ENG_SHIM_BIN";
pub const ENV_EXTRA_READ: &str = "ENG_EXTRA_READ_DIRS";

/// The configuration the environment describes: `ENG_CLAUDE_BIN`, `ENG_CODEX_BIN`,
/// `ENG_NODE_BIN` (each else found on `PATH`), `ENG_SHIM_BIN` (else `eng-shim` beside the
/// running executable) and `ENG_EXTRA_READ_DIRS` (colon-separated).
///
/// A runtime that cannot be found does not fail resolution, so commands that never launch it
/// (`status`, `audit`, `doctor` itself) still work: it stays a bare name, which `doctor` and
/// [`require_runtimes`] report and a launch refuses. An unusable `ENG_EXTRA_READ_DIRS` is an
/// error.
pub fn resolve_config(state_dir: PathBuf) -> Result<EngineConfig, EngineError> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let get = |k: &str| std::env::var_os(k);
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("eng"));
    let codex_bin =
        pick(get(ENV_CODEX).as_deref(), "codex", &path).unwrap_or_else(|_| PathBuf::from("codex"));
    let node_bin = node_for_codex(
        &codex_bin,
        pick(get(ENV_NODE).as_deref(), "node", &path).ok(),
    );
    // The sandbox resolver rejects relative paths, so a relative `--state-dir` is anchored here.
    let state_dir = std::path::absolute(&state_dir)?;
    Ok(EngineConfig {
        state_dir,
        claude_bin: pick(get(ENV_CLAUDE).as_deref(), "claude", &path)
            .unwrap_or_else(|_| PathBuf::from("claude")),
        codex_bin,
        node_bin,
        shim_bin: shim_path(get(ENV_SHIM).as_deref(), &exe),
        extra_read_dirs: extra_read_dirs(get(ENV_EXTRA_READ).as_deref())
            .map_err(EngineError::Workspace)?,
    })
}

/// `node` only when Codex's entry point is a JavaScript file (a `.js`/`.mjs`/`.cjs` extension,
/// or a first line `#!` naming `node`); a native binary or a shell wrapper is run directly.
fn node_for_codex(codex: &Path, node: Option<PathBuf>) -> Option<PathBuf> {
    node.filter(|_| is_js_entry(codex))
}

fn is_js_entry(path: &Path) -> bool {
    use std::io::Read as _;
    if path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|e| matches!(e, "js" | "mjs" | "cjs"))
    {
        return true;
    }
    let mut head = [0u8; 256];
    let Ok(n) = std::fs::File::open(path).and_then(|mut f| f.read(&mut head)) else {
        return false;
    };
    let line = head[..n].split(|b| *b == b'\n').next().unwrap_or_default();
    let Some(rest) = line.strip_prefix(b"#!") else {
        return false;
    };
    String::from_utf8_lossy(rest)
        .split_whitespace()
        .any(|w| w.rsplit('/').next() == Some("node"))
}

/// Fails with the first runtime that [`resolve_config`] could not find.
pub fn require_runtimes(cfg: &EngineConfig) -> Result<(), String> {
    for (name, bin) in [("claude", &cfg.claude_bin), ("codex", &cfg.codex_bin)] {
        if !bin.is_absolute() {
            return Err(format!(
                "{name}: not found (set ENG_{}_BIN, or put `{name}` on PATH)",
                name.to_uppercase()
            ));
        }
    }
    Ok(())
}

/// The first executable file called `name` in an absolute `PATH` entry.
pub fn find_on_path(name: &str, path: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .filter(|d| d.is_absolute())
        .map(|d| d.join(name))
        .find(|p| is_executable(p))
}

fn is_executable(p: &Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// `env_value` if set (non-empty), else `name` on `path`; canonicalised.
pub fn pick(env_value: Option<&OsStr>, name: &str, path: &OsStr) -> Result<PathBuf, String> {
    let chosen = match env_value.filter(|v| !v.is_empty()) {
        Some(v) => PathBuf::from(v),
        None => find_on_path(name, path).ok_or_else(|| format!("`{name}` not found on PATH"))?,
    };
    std::fs::canonicalize(&chosen).map_err(|e| format!("{}: {e}", chosen.display()))
}

/// `eng-shim` beside the running `eng`, unless `ENG_SHIM_BIN` names another.
pub fn shim_path(env_value: Option<&OsStr>, current_exe: &Path) -> PathBuf {
    match env_value.filter(|v| !v.is_empty()) {
        Some(v) => PathBuf::from(v),
        None => current_exe.with_file_name("eng-shim"),
    }
}

/// Colon-separated directories, each canonicalised.
pub fn extra_read_dirs(env_value: Option<&OsStr>) -> Result<Vec<PathBuf>, String> {
    let Some(v) = env_value.filter(|v| !v.is_empty()) else {
        return Ok(Vec::new());
    };
    std::env::split_paths(v)
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| {
            std::fs::canonicalize(&p).map_err(|e| format!("{ENV_EXTRA_READ}: {}: {e}", p.display()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, "#!/bin/sh\n").unwrap_or_default();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap_or_default();
        p
    }

    fn tmp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("eng-cli-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap_or_default();
        std::fs::canonicalize(&p).unwrap_or(p)
    }

    #[test]
    fn env_beats_path_and_symlinks_are_resolved() {
        let d = tmp("pick");
        let real = exe(&d, "real-claude");
        let link = d.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap_or_default();
        let other = tmp("pick-other");
        exe(&other, "claude");
        let path = other.as_os_str().to_owned();
        assert_eq!(pick(Some(link.as_os_str()), "claude", &path), Ok(real));
        assert_eq!(pick(None, "claude", &path), Ok(other.join("claude")));
        assert!(pick(None, "nonesuch", &path).is_err());
        assert!(pick(Some(OsStr::new("/nonexistent/claude")), "claude", &path).is_err());
    }

    #[test]
    fn path_search_skips_relative_entries_and_non_executables() {
        let d = tmp("search");
        std::fs::write(d.join("tool"), "x").unwrap_or_default();
        assert_eq!(find_on_path("tool", d.as_os_str()), None);
        assert_eq!(find_on_path("tool", OsStr::new("rel/dir")), None);
    }

    #[test]
    fn shim_is_beside_eng_unless_overridden() {
        let exe = Path::new("/x/target/debug/eng");
        assert_eq!(
            shim_path(None, exe),
            PathBuf::from("/x/target/debug/eng-shim")
        );
        assert_eq!(
            shim_path(Some(OsStr::new("/y/shim")), exe),
            PathBuf::from("/y/shim")
        );
    }

    #[test]
    fn extra_read_dirs_are_canonical_and_must_exist() {
        let d = tmp("extra");
        let link = d.with_extension("link");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&d, &link).unwrap_or_default();
        let joined = std::env::join_paths([&link, &d]).unwrap_or_default();
        assert_eq!(
            extra_read_dirs(Some(&joined)),
            Ok(vec![d.clone(), d.clone()])
        );
        assert_eq!(extra_read_dirs(None), Ok(vec![]));
        let bad = std::env::join_paths([d.join("missing")]).unwrap_or_default();
        assert!(extra_read_dirs(Some(&bad)).is_err());
    }

    #[test]
    fn unresolved_runtimes_are_reported_by_name() {
        let mut cfg = EngineConfig {
            state_dir: "/s".into(),
            claude_bin: "/real/claude".into(),
            codex_bin: "codex".into(),
            node_bin: None,
            shim_bin: "/x/eng-shim".into(),
            extra_read_dirs: vec![],
        };
        assert!(
            require_runtimes(&cfg).is_err_and(|e| e.starts_with("codex: not found")),
            "codex is unresolved"
        );
        cfg.codex_bin = "/real/codex".into();
        assert_eq!(require_runtimes(&cfg), Ok(()));
    }

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap_or_default();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap_or_default();
        p
    }

    #[test]
    fn node_is_used_only_for_a_javascript_codex() {
        let d = tmp("node");
        let node = Some(PathBuf::from("/real/node"));
        let js = write(&d, "codex.js", b"console.log(1)\n");
        assert_eq!(node_for_codex(&js, node.clone()), node);
        let env_node = write(&d, "codex-env", b"#!/usr/bin/env node\nrequire('x')\n");
        assert_eq!(node_for_codex(&env_node, node.clone()), node);
        let native = write(
            &d,
            "codex-native",
            b"\xcf\xfa\xed\xfe\x07\x00\x00\x01 binary",
        );
        assert_eq!(node_for_codex(&native, node.clone()), None);
        let sh = write(&d, "codex-sh", b"#!/bin/sh\nexec /opt/codex \"$@\"\n");
        assert_eq!(node_for_codex(&sh, node.clone()), None);
        // A bare, unresolved name has nothing to read.
        assert_eq!(node_for_codex(Path::new("codex"), node), None);
    }

    #[test]
    fn relative_state_dir_becomes_absolute() {
        let cfg = resolve_config(PathBuf::from("./eng-state-relative"))
            .unwrap_or_else(|e| panic!("resolve: {e}"));
        assert!(cfg.state_dir.is_absolute());
        assert!(cfg.state_dir.ends_with("eng-state-relative"));
        assert!(!cfg.state_dir.exists());
    }
}
