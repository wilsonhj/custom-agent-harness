//! `eng login claude|codex`: the official CLI's own interactive login, with the harness-owned
//! config directory. `eng` reads nothing the login produces.

use std::path::PathBuf;

use harness_engine::EngineConfig;

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

/// A runtime the engine's configuration resolved (an absolute, canonical path).
fn resolved(bin: &std::path::Path, name: &str) -> Result<PathBuf, String> {
    if bin.is_absolute() {
        Ok(bin.to_path_buf())
    } else {
        Err(format!("{name}: not found"))
    }
}

/// Pure: the argv and environment of the login. Claude runs the CLI itself (the user types
/// `/login`); Codex runs `codex login`, as `node <codex.js> login` for a JavaScript entry point when Node is available (the
/// same launch shape the engine uses).
pub fn plan(runtime: LoginRuntime, cfg: &EngineConfig) -> Result<LoginPlan, String> {
    let runtimes = cfg.state_dir.join("runtimes");
    match runtime {
        LoginRuntime::Claude => {
            let claude = resolved(&cfg.claude_bin, "claude")?;
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
            let codex = resolved(&cfg.codex_bin, "codex")?;
            let dir = runtimes.join("codex");
            let (program, args) = match cfg.node_bin.as_ref().filter(|_| cfg.codex_uses_node) {
                Some(node) => (
                    node.clone(),
                    vec![codex.to_string_lossy().into_owned(), "login".into()],
                ),
                None => (codex, vec!["login".into()]),
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
///
/// The one sanctioned exception to "only `harness-exec` spawns processes": `eng login` hands
/// the terminal to the official CLI's own interactive login, which must run unsandboxed
/// (it opens a browser and writes the credential store). It is an `exec`, so no child
/// outlives it, and no agent run, supervisor or sandbox is involved.
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

    fn cfg(node: bool) -> EngineConfig {
        EngineConfig {
            state_dir: "/s".into(),
            claude_bin: "/real/claude".into(),
            codex_bin: "/real/pkg/bin/codex.js".into(),
            node_bin: node.then(|| "/real/node".into()),
            codex_uses_node: true,
            shim_bin: "/x/eng-shim".into(),
            extra_read_dirs: vec![],
        }
    }

    #[test]
    fn claude_login_runs_the_cli_with_the_harness_config_dir() {
        let p = plan(LoginRuntime::Claude, &cfg(true)).unwrap();
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
        let p = plan(LoginRuntime::Codex, &cfg(true)).unwrap();
        assert_eq!(p.program, PathBuf::from("/real/node"));
        assert_eq!(p.args, ["/real/pkg/bin/codex.js", "login"]);
        assert_eq!(
            p.env_set,
            [("CODEX_HOME".to_owned(), PathBuf::from("/s/runtimes/codex"))]
        );
        assert_eq!(p.env_remove, ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"]);
        let p = plan(LoginRuntime::Codex, &cfg(false)).unwrap();
        assert_eq!(
            (p.program, p.args),
            (
                PathBuf::from("/real/pkg/bin/codex.js"),
                vec!["login".to_owned()]
            )
        );
    }

    #[test]
    fn native_codex_login_runs_directly_with_node_available() {
        let mut c = cfg(true);
        c.codex_bin = "/real/codex-native".into();
        c.codex_uses_node = false;
        let p = plan(LoginRuntime::Codex, &c).unwrap();
        assert_eq!(p.program, c.codex_bin);
        assert_eq!(p.args, ["login"]);
    }

    #[test]
    fn missing_runtime_is_an_error() {
        let mut c = cfg(true);
        c.claude_bin = "claude".into();
        assert!(plan(LoginRuntime::Claude, &c).is_err());
    }
}
