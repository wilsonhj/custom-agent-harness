//! Runs `/usr/bin/sandbox-exec` with generated profiles around system binaries only
//! (`/bin/sh`, `/usr/bin/touch`, `/bin/cat`, `/usr/bin/nc`). Every secret, state directory and
//! Git directory here is a fake inside a temporary directory.

// The plan denies these lints in non-test code only (Global Constraints).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use harness_exec::LaunchPlan;
use harness_exec::sandbox_macos::{Network, SandboxProfile, default_read_deny, probe, wrap};

struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    wt: PathBuf,
    home: PathBuf,
}

/// root/{wt, home}; `root` is canonical so test assertions can use it directly.
fn fixture_in(parent: &Path, wt_name: &str) -> Fixture {
    let tmp = tempfile::Builder::new()
        .prefix("eng-t6-")
        .tempdir_in(parent)
        .unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    let wt = root.join(wt_name);
    let home = root.join("home");
    fs::create_dir(&wt).unwrap();
    fs::create_dir(&home).unwrap();
    Fixture {
        _tmp: tmp,
        root,
        wt,
        home,
    }
}

fn fixture() -> Fixture {
    fixture_in(&std::env::temp_dir(), "wt")
}

fn profile(f: &Fixture) -> SandboxProfile {
    SandboxProfile {
        worktree: f.wt.clone(),
        worktree_writable: true,
        task_home: f.home.clone(),
        read_deny: Vec::new(),
        read_allow_extra: Vec::new(),
        network: Network::None,
    }
}

fn run(p: &SandboxProfile, argv: &[&str]) -> Output {
    let plan = LaunchPlan {
        argv: argv.iter().map(|s| s.to_string()).collect(),
        env: Vec::new(),
        cwd: p.worktree.clone(),
        stdin: Vec::new(),
    };
    let w = wrap(p, plan).unwrap();
    Command::new(&w.argv[0])
        .args(&w.argv[1..])
        .envs(w.env.iter().map(|(k, v)| (k, v)))
        .current_dir(&w.cwd)
        .output()
        .unwrap()
}

fn sh(p: &SandboxProfile, script: &str) -> Output {
    run(p, &["/bin/sh", "-c", script])
}

fn ok(o: &Output) -> bool {
    o.status.success()
}

fn denied(o: &Output) -> bool {
    !o.status.success() && String::from_utf8_lossy(&o.stderr).contains("Operation not permitted")
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn probe_succeeds_on_this_machine() {
    let info = probe().unwrap();
    assert_eq!(info.sandbox_exec, PathBuf::from("/usr/bin/sandbox-exec"));
}

#[test]
fn write_outside_worktree_is_denied() {
    let f = fixture();
    let p = profile(&f);
    let inside = f.wt.join("inside");
    let in_home = f.home.join("cfg");
    let outside = f.root.join("outside");

    assert!(ok(&run(&p, &["/usr/bin/touch", s(&inside)])));
    assert!(ok(&run(&p, &["/usr/bin/touch", s(&in_home)])));
    let o = run(&p, &["/usr/bin/touch", s(&outside)]);
    assert!(denied(&o), "{o:?}");
    assert!(inside.exists() && in_home.exists() && !outside.exists());
}

#[test]
fn readonly_worktree_is_not_writable_but_task_home_is() {
    let f = fixture();
    let mut p = profile(&f);
    p.worktree_writable = false;
    fs::write(f.wt.join("README"), "r").unwrap();

    assert!(ok(&run(&p, &["/bin/cat", s(&f.wt.join("README"))])));
    assert!(denied(&run(&p, &["/usr/bin/touch", s(&f.wt.join("x"))])));
    assert!(ok(&run(&p, &["/usr/bin/touch", s(&f.home.join("x"))])));
}

#[test]
fn write_via_private_tmp_alias_is_denied() {
    // Every path is handed to the profile with the /tmp spelling; the writes go through the
    // /private/tmp spelling. A rule rendered with the alias would not match them.
    let f = fixture_in(Path::new("/tmp"), "wt");
    assert!(f.root.starts_with("/private/tmp/"));
    let alias = |p: &Path| Path::new("/tmp").join(p.strip_prefix("/private/tmp").unwrap());
    fs::write(f.wt.join(".git"), "gitdir: elsewhere\n").unwrap();

    let p = SandboxProfile {
        worktree: alias(&f.wt),
        worktree_writable: true,
        task_home: alias(&f.home),
        read_deny: vec![alias(&f.wt.join(".claude"))],
        read_allow_extra: Vec::new(),
        network: Network::None,
    };

    assert!(ok(&run(&p, &["/usr/bin/touch", s(&f.wt.join("ok"))])));
    let o = sh(&p, &format!("echo x > '{}'", s(&f.wt.join(".git"))));
    assert!(denied(&o), "{o:?}");
    let o = sh(&p, &format!("mkdir '{}'", s(&f.wt.join(".claude"))));
    assert!(denied(&o), "{o:?}");
    assert_eq!(
        fs::read_to_string(f.wt.join(".git")).unwrap(),
        "gitdir: elsewhere\n"
    );
}

#[test]
fn paths_with_space_and_quote_are_parameters_not_profile_text() {
    let f = fixture_in(&std::env::temp_dir(), "a \"b\" ) (allow default) c");
    let p = profile(&f);
    let inside = f.wt.join("inside");
    assert!(ok(&run(&p, &["/usr/bin/touch", s(&inside)])));
    assert!(inside.exists());
    assert!(denied(&run(
        &p,
        &["/usr/bin/touch", s(&f.root.join("outside"))]
    )));
}

#[test]
fn read_of_ssh_dir_and_state_dir_is_denied() {
    let f = fixture();
    let fake_home = f.root.join("user");
    let state = f.root.join("state");
    let common = f.root.join("repo.git");
    fs::create_dir_all(fake_home.join(".ssh")).unwrap();
    fs::create_dir_all(fake_home.join(".aws")).unwrap();
    fs::create_dir_all(&state).unwrap();
    fs::create_dir_all(&common).unwrap();
    fs::write(fake_home.join(".ssh/id_fake"), "k").unwrap();
    fs::write(fake_home.join(".aws/credentials"), "k").unwrap();
    fs::write(state.join("state.db"), "db").unwrap();
    fs::write(f.root.join("public.txt"), "pub").unwrap();

    let mut p = profile(&f);
    p.read_deny = default_read_deny(&fake_home, &state, &f.wt, &common);

    for secret in [
        fake_home.join(".ssh/id_fake"),
        fake_home.join(".aws/credentials"),
        state.join("state.db"),
    ] {
        let o = run(&p, &["/bin/cat", s(&secret)]);
        assert!(denied(&o), "{secret:?}: {o:?}");
    }
    assert!(denied(&run(&p, &["/bin/ls", s(&fake_home.join(".ssh"))])));
    let o = run(&p, &["/bin/cat", s(&f.root.join("public.txt"))]);
    assert_eq!(o.stdout, b"pub");
}

#[test]
fn write_to_git_file_and_common_dir_is_denied() {
    let f = fixture();
    let common = f.root.join("repo/.git");
    let wt_meta = common.join("worktrees/wt");
    fs::create_dir_all(&wt_meta).unwrap();
    fs::write(wt_meta.join("HEAD"), "ref: x\n").unwrap();
    fs::write(f.wt.join(".git"), format!("gitdir: {}\n", s(&wt_meta))).unwrap();

    let mut p = profile(&f);
    p.read_deny = default_read_deny(&f.root.join("user"), &f.root.join("state"), &f.wt, &common);

    let o = sh(&p, &format!("echo x > '{}'", s(&f.wt.join(".git"))));
    assert!(denied(&o), "{o:?}");
    let o = run(&p, &["/usr/bin/touch", s(&wt_meta.join("HEAD"))]);
    assert!(denied(&o), "{o:?}");
    let o = run(&p, &["/usr/bin/touch", s(&common.join("new"))]);
    assert!(denied(&o), "{o:?}");
    assert!(ok(&run(&p, &["/usr/bin/touch", s(&f.wt.join("src.rs"))])));
    assert!(!common.join("new").exists());
}

#[test]
fn read_of_repo_agent_config_is_denied_but_agents_md_is_readable() {
    let f = fixture();
    fs::create_dir_all(f.wt.join(".claude")).unwrap();
    fs::write(f.wt.join(".claude/settings.json"), "{}").unwrap();
    fs::write(f.wt.join(".mcp.json"), "{}").unwrap();
    fs::create_dir_all(f.wt.join(".codex")).unwrap();
    fs::write(f.wt.join(".codex/config.toml"), "").unwrap();
    fs::write(f.wt.join("AGENTS.md"), "agents").unwrap();
    fs::write(f.wt.join("CLAUDE.md"), "claude").unwrap();

    let mut p = profile(&f);
    p.read_deny = default_read_deny(
        &f.root.join("user"),
        &f.root.join("state"),
        &f.wt,
        &f.root.join("repo.git"),
    );

    for cfg in [".claude/settings.json", ".mcp.json", ".codex/config.toml"] {
        let o = run(&p, &["/bin/cat", s(&f.wt.join(cfg))]);
        assert!(denied(&o), "{cfg}: {o:?}");
    }
    // The agent cannot plant configuration either.
    assert!(denied(&run(
        &p,
        &["/usr/bin/touch", s(&f.wt.join(".cursor"))]
    )));
    assert_eq!(run(&p, &["/bin/cat", "AGENTS.md"]).stdout, b"agents");
    assert_eq!(run(&p, &["/bin/cat", "CLAUDE.md"]).stdout, b"claude");
}

#[test]
fn nested_sandbox_exec_fails_inside_profile() {
    // Documents research-2026-11-09/1a §4: Seatbelt does not nest, so every agent's own
    // sandbox must be off inside ours.
    let f = fixture();
    let o = run(
        &profile(&f),
        &[
            "/usr/bin/sandbox-exec",
            "-p",
            "(version 1)(allow default)",
            "/usr/bin/true",
        ],
    );
    assert!(!o.status.success());
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("Operation not permitted"),
        "{o:?}"
    );
}

#[test]
fn network_none_blocks_loopback_and_remote() {
    let f = fixture();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();

    // Control: the same probe outside the sandbox reaches the listener.
    let control = Command::new("/usr/bin/nc")
        .args(["-vz", "-G", "2", "127.0.0.1", &port])
        .output()
        .unwrap();
    assert!(control.status.success(), "{control:?}");

    let p = profile(&f);
    let o = run(&p, &["/usr/bin/nc", "-vz", "-G", "2", "127.0.0.1", &port]);
    assert!(denied(&o), "loopback: {o:?}");
    // 192.0.2.1 is TEST-NET-1 (RFC 5737): nothing answers there, and the sandbox refuses the
    // connect before any packet leaves.
    let o = run(&p, &["/usr/bin/nc", "-vz", "-G", "2", "192.0.2.1", "443"]);
    assert!(denied(&o), "remote: {o:?}");
}
