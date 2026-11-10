//! Finding the runtimes, the shim and the extra read directories. Every path is canonicalised
//! because the sandbox denies `$HOME`, where `claude` and `nvm` install through symlinks.

use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use harness_engine::EngineConfig;

pub const ENV_CLAUDE: &str = "ENG_CLAUDE_BIN";
pub const ENV_CODEX: &str = "ENG_CODEX_BIN";
pub const ENV_NODE: &str = "ENG_NODE_BIN";
pub const ENV_SHIM: &str = "ENG_SHIM_BIN";
pub const ENV_EXTRA_READ: &str = "ENG_EXTRA_READ_DIRS";

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

/// What the environment resolved to. A runtime that cannot be found stays an `Err` so that
/// commands that never launch it (`status`, `audit`) still work.
pub struct Resolved {
    pub claude: Result<PathBuf, String>,
    pub codex: Result<PathBuf, String>,
    pub node: Result<PathBuf, String>,
    pub shim: PathBuf,
    pub extra_read_dirs: Result<Vec<PathBuf>, String>,
}

impl Resolved {
    pub fn from_env() -> Resolved {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let get = |k: &str| std::env::var_os(k);
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("eng"));
        Resolved {
            claude: pick(get(ENV_CLAUDE).as_deref(), "claude", &path),
            codex: pick(get(ENV_CODEX).as_deref(), "codex", &path),
            node: pick(get(ENV_NODE).as_deref(), "node", &path),
            shim: shim_path(get(ENV_SHIM).as_deref(), &exe),
            extra_read_dirs: extra_read_dirs(get(ENV_EXTRA_READ).as_deref()),
        }
    }

    /// Fails with the first reason a run cannot start.
    pub fn require_for_run(&self) -> Result<(), String> {
        self.claude.as_ref().map_err(|e| format!("claude: {e}"))?;
        self.codex.as_ref().map_err(|e| format!("codex: {e}"))?;
        self.extra_read_dirs.as_ref().map_err(Clone::clone)?;
        Ok(())
    }

    /// The engine's configuration. Unresolved runtimes become bare names, which the engine
    /// refuses at launch; `require_for_run` keeps a run from getting there.
    pub fn engine_config(&self, state_dir: PathBuf) -> EngineConfig {
        EngineConfig {
            state_dir,
            claude_bin: self
                .claude
                .clone()
                .unwrap_or_else(|_| PathBuf::from("claude")),
            codex_bin: self
                .codex
                .clone()
                .unwrap_or_else(|_| PathBuf::from("codex")),
            node_bin: self.node.clone().ok(),
            shim_bin: self.shim.clone(),
            extra_read_dirs: self.extra_read_dirs.clone().unwrap_or_default(),
        }
    }
}

/// `~/Library/Application Support/eng`.
pub fn default_state_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    Ok(PathBuf::from(home).join("Library/Application Support/eng"))
}

/// The state directory, created mode 0700 if absent.
pub fn ensure_state_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt as _;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
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
}
