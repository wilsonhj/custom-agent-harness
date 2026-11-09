//! Profile generation without running the sandbox (plan Task 6).

// The plan denies these lints in non-test code only (Global Constraints).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use harness_exec::LaunchPlan;
use harness_exec::sandbox_macos::{
    Network, ProfileError, SandboxProfile, default_read_deny, render_profile, resolve, wrap,
};

fn profile(worktree: &Path, task_home: &Path) -> SandboxProfile {
    SandboxProfile {
        worktree: worktree.to_path_buf(),
        worktree_writable: true,
        task_home: task_home.to_path_buf(),
        read_deny: Vec::new(),
        read_allow_extra: Vec::new(),
        network: Network::None,
    }
}

fn dirs() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let wt = tmp.path().join("wt");
    let home = tmp.path().join("home");
    fs::create_dir(&wt).unwrap();
    fs::create_dir(&home).unwrap();
    (tmp, wt, home)
}

fn plan() -> LaunchPlan {
    LaunchPlan {
        argv: vec!["/bin/echo".into(), "hi".into()],
        env: vec![("A".into(), "1".into())],
        cwd: PathBuf::from("/"),
        stdin: b"in".to_vec(),
    }
}

/// The value passed for `-D <key>=...` in a wrapped argv.
fn param<'a>(argv: &'a [String], key: &str) -> Option<&'a str> {
    let prefix = format!("{key}=");
    argv.windows(2)
        .find(|w| w[0] == "-D" && w[1].starts_with(&prefix))
        .map(|w| &w[1][prefix.len()..])
}

#[test]
fn profile_starts_with_deny_default() {
    let (_tmp, wt, home) = dirs();
    let text = render_profile(&profile(&wt, &home)).unwrap();
    let rules: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with(';'))
        .collect();
    assert_eq!(rules[0], "(version 1)");
    assert_eq!(rules[1], "(deny default)");
}

#[test]
fn profile_resolves_real_paths() {
    // A worktree reached through /tmp (a symlink to /private/tmp), through a further symlink,
    // and with a space and a double quote in its name.
    let tmp = tempfile::Builder::new()
        .prefix("eng-t6 profile")
        .tempdir_in("/tmp")
        .unwrap();
    let spelled = Path::new("/tmp").join(tmp.path().file_name().unwrap());
    let real_wt = spelled.join("work \"tree\" a");
    fs::create_dir(&real_wt).unwrap();
    let link = spelled.join("link");
    std::os::unix::fs::symlink(&real_wt, &link).unwrap();
    let home = spelled.join("home");
    fs::create_dir(&home).unwrap();

    let mut p = profile(&link, &home);
    // Deny entries that do not exist yet are still resolved through their real parent.
    p.read_deny = vec![link.join(".claude"), spelled.join("state")];
    p.network = Network::ProxyOnly { port: 4321 };

    let resolved = resolve(&p).unwrap();
    let canonical_wt = fs::canonicalize(&real_wt).unwrap();
    assert!(canonical_wt.starts_with("/private/tmp/"));
    assert_eq!(Path::new(&resolved.worktree), canonical_wt);
    assert_eq!(
        Path::new(&resolved.read_deny[0]),
        canonical_wt.join(".claude")
    );

    let wrapped = wrap(&p, plan()).unwrap();
    assert_eq!(param(&wrapped.argv, "WT"), canonical_wt.to_str());
    assert_eq!(param(&wrapped.argv, "PROXY_PORT"), Some("4321"));
    for arg in &wrapped.argv {
        assert!(!arg.contains("=/tmp/"), "alias survived in {arg:?}");
        assert!(!arg.contains("/link"), "symlink survived in {arg:?}");
    }

    // Paths travel only as -D parameters: the profile text contains none of them.
    let text = render_profile(&p).unwrap();
    assert!(!text.contains("/tmp/"));
    assert!(!text.contains("work \""));
    assert!(!text.contains("eng-t6"));
    assert!(text.contains("(param \"WT\")"));
}

#[test]
fn unresolvable_path_is_an_error_not_a_skipped_rule() {
    let (tmp, wt, home) = dirs();

    let missing_wt = profile(&tmp.path().join("missing"), &home);
    assert!(matches!(
        render_profile(&missing_wt),
        Err(ProfileError::Unresolvable { .. })
    ));
    assert!(matches!(
        wrap(&missing_wt, plan()),
        Err(ProfileError::Unresolvable { .. })
    ));

    let mut relative_deny = profile(&wt, &home);
    relative_deny.read_deny = vec![PathBuf::from("relative/secret")];
    assert!(matches!(
        render_profile(&relative_deny),
        Err(ProfileError::Unresolvable { .. })
    ));

    let mut dotdot_deny = profile(&wt, &home);
    dotdot_deny.read_deny = vec![wt.join("not-yet/../escape")];
    assert!(matches!(
        render_profile(&dotdot_deny),
        Err(ProfileError::Unresolvable { .. })
    ));

    let mut missing_extra = profile(&wt, &home);
    missing_extra.read_allow_extra = vec![tmp.path().join("nope")];
    assert!(matches!(
        render_profile(&missing_extra),
        Err(ProfileError::Unresolvable { .. })
    ));
}

#[test]
fn readonly_profile_has_no_worktree_write_rule() {
    let (_tmp, wt, home) = dirs();
    let mut p = profile(&wt, &home);
    let writable = render_profile(&p).unwrap();
    assert!(writable.contains("(allow file-write* (subpath (param \"WT\")))"));

    p.worktree_writable = false;
    let readonly = render_profile(&p).unwrap();
    for line in readonly.lines() {
        if line.contains("(allow file-write") {
            assert!(!line.contains("\"WT\""), "worktree write rule: {line}");
        }
    }
    assert!(readonly.contains("(allow file-write* (subpath (param \"TASKHOME\")))"));
}

#[test]
fn default_read_deny_lists_secrets_state_git_and_agent_config() {
    let deny = default_read_deny(
        Path::new("/h"),
        Path::new("/s"),
        Path::new("/w"),
        Path::new("/r/.git"),
    );
    for want in [
        "/h/.ssh",
        "/h/.aws",
        "/s",
        "/r/.git",
        "/w/.git",
        "/w/.claude",
        "/w/.mcp.json",
        "/w/.codex",
        "/w/.agents",
        "/w/.cursor",
    ] {
        assert!(deny.contains(&PathBuf::from(want)), "missing {want}");
    }
}

#[test]
fn network_rules_follow_the_policy() {
    let (_tmp, wt, home) = dirs();
    let mut p = profile(&wt, &home);
    let none = render_profile(&p).unwrap();
    assert!(!none.contains("network-outbound"));
    assert!(!none.contains("network-bind"));

    p.network = Network::ProxyOnly { port: 9 };
    let proxy = render_profile(&p).unwrap();
    assert!(proxy.contains(
        "(allow network-outbound (remote tcp (string-append \"localhost:\" (param \"PROXY_PORT\"))))"
    ));
    assert_eq!(proxy.matches("network-outbound").count(), 1);
}

#[test]
fn wrap_prefixes_sandbox_exec_and_adds_proxy_env() {
    let (_tmp, wt, home) = dirs();
    let mut p = profile(&wt, &home);
    p.network = Network::ProxyOnly { port: 5555 };
    let original = plan();
    let wrapped = wrap(&p, original.clone()).unwrap();

    assert_eq!(wrapped.argv[0], "/usr/bin/sandbox-exec");
    assert_eq!(wrapped.argv[1], "-p");
    assert!(wrapped.argv[2].contains("(deny default)"));
    let sep = wrapped.argv.iter().position(|a| a == "--").unwrap();
    assert_eq!(&wrapped.argv[sep + 1..], original.argv.as_slice());
    assert_eq!(wrapped.cwd, original.cwd);
    assert_eq!(wrapped.stdin, original.stdin);

    let env = |k: &str| {
        wrapped
            .env
            .iter()
            .rev()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(env("A"), Some("1"));
    assert_eq!(env("HTTPS_PROXY"), Some("http://127.0.0.1:5555"));
    assert_eq!(env("HTTP_PROXY"), Some("http://127.0.0.1:5555"));
    assert_eq!(env("NODE_USE_ENV_PROXY"), Some("1"));

    p.network = Network::None;
    let no_net = wrap(&p, original).unwrap();
    assert!(no_net.env.iter().all(|(k, _)| k != "HTTPS_PROXY"));
}
