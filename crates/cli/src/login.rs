//! `eng login claude|codex`: the official CLI's own interactive login, with the harness-owned
//! config directory. `eng` reads nothing the login produces.

use std::path::{Path, PathBuf};

use crate::runtimes::Resolved;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum LoginRuntime {
    Claude,
    Codex,
}

/// Variables removed so the login (and later runs) cannot silently switch to API billing.
pub const REMOVED_ENV: &[&str] = &["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Set in the child's environment.
    pub env_set: Vec<(String, PathBuf)>,
    /// Removed from the child's environment.
    pub env_remove: Vec<&'static str>,
    /// Created mode 0700 before the exec.
    pub config_dir: PathBuf,
    /// What to tell the user before handing over the terminal.
    pub hint: &'static str,
}

/// Pure: the argv and environment of the login. Claude runs the CLI itself (the user types
/// `/login`); Codex runs `codex login`, as `node <codex.js> login` when `node` is known (the
/// same launch shape the engine uses).
pub fn plan(runtime: LoginRuntime, state_dir: &Path, r: &Resolved) -> Result<LoginPlan, String> {
    let runtimes = state_dir.join("runtimes");
    match runtime {
        LoginRuntime::Claude => {
            let claude = r.claude.clone().map_err(|e| format!("claude: {e}"))?;
            let dir = runtimes.join("claude");
            Ok(LoginPlan {
                program: claude,
                args: vec![],
                env_set: vec![("CLAUDE_CONFIG_DIR".into(), dir.clone())],
                env_remove: REMOVED_ENV.to_vec(),
                config_dir: dir,
                hint: "Claude Code is starting. Type /login, finish signing in, then /exit.",
            })
        }
        LoginRuntime::Codex => {
            let codex = r.codex.clone().map_err(|e| format!("codex: {e}"))?;
            let dir = runtimes.join("codex");
            let (program, args) = match &r.node {
                Ok(node) => (
                    node.clone(),
                    vec![codex.to_string_lossy().into_owned(), "login".into()],
                ),
                Err(_) => (codex, vec!["login".into()]),
            };
            Ok(LoginPlan {
                program,
                args,
                env_set: vec![("CODEX_HOME".into(), dir.clone())],
                env_remove: REMOVED_ENV.to_vec(),
                config_dir: dir,
                hint: "Starting `codex login`.",
            })
        }
    }
}

/// Replaces this process with the login. Only returns on failure.
pub fn exec(p: &LoginPlan) -> std::io::Error {
    use std::os::unix::fs::DirBuilderExt as _;
    use std::os::unix::process::CommandExt as _;
    if let Err(e) = std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&p.config_dir)
    {
        return e;
    }
    eprintln!("eng: {}", p.hint);
    let mut cmd = std::process::Command::new(&p.program);
    cmd.args(&p.args);
    for (k, v) in &p.env_set {
        cmd.env(k, v);
    }
    for k in &p.env_remove {
        cmd.env_remove(k);
    }
    cmd.exec()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn resolved(node: bool) -> Resolved {
        Resolved {
            claude: Ok("/real/claude".into()),
            codex: Ok("/real/pkg/bin/codex.js".into()),
            node: if node {
                Ok("/real/node".into())
            } else {
                Err("none".into())
            },
            shim: "/x/eng-shim".into(),
            extra_read_dirs: Ok(vec![]),
        }
    }

    #[test]
    fn claude_login_runs_the_cli_with_the_harness_config_dir() {
        let p = plan(LoginRuntime::Claude, Path::new("/s"), &resolved(true)).unwrap();
        assert_eq!(p.program, PathBuf::from("/real/claude"));
        assert!(p.args.is_empty());
        assert_eq!(
            p.env_set,
            [(
                "CLAUDE_CONFIG_DIR".to_owned(),
                PathBuf::from("/s/runtimes/claude")
            )]
        );
        assert_eq!(p.env_remove, ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"]);
        assert_eq!(p.config_dir, PathBuf::from("/s/runtimes/claude"));
    }

    #[test]
    fn codex_login_runs_codex_login_with_codex_home() {
        let p = plan(LoginRuntime::Codex, Path::new("/s"), &resolved(true)).unwrap();
        assert_eq!(p.program, PathBuf::from("/real/node"));
        assert_eq!(p.args, ["/real/pkg/bin/codex.js", "login"]);
        assert_eq!(
            p.env_set,
            [("CODEX_HOME".to_owned(), PathBuf::from("/s/runtimes/codex"))]
        );
        assert_eq!(p.env_remove, ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"]);
        let p = plan(LoginRuntime::Codex, Path::new("/s"), &resolved(false)).unwrap();
        assert_eq!(
            (p.program, p.args),
            (
                PathBuf::from("/real/pkg/bin/codex.js"),
                vec!["login".to_owned()]
            )
        );
    }

    #[test]
    fn missing_runtime_is_an_error() {
        let mut r = resolved(true);
        r.claude = Err("not found".into());
        assert!(plan(LoginRuntime::Claude, Path::new("/s"), &r).is_err());
    }
}
