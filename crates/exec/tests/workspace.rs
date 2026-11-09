//! Task 7: worktrees, hardened host Git, checkpoint refs, secret scan.
//!
//! Every test builds a throwaway repository under a temp dir and runs the real `/usr/bin/git`.
//! The test's own Git calls (`tgit`) set identity through `GIT_AUTHOR_*`/`GIT_COMMITTER_*` and
//! read no user or system configuration.
//!
//! Token-shaped fixtures are assembled with `concat!` so this source file never contains a
//! contiguous token that the harness's own secret scan would flag.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use harness_core::ids::{CommitSha, TaskId, Timestamp};
use harness_core::outcome::FailureCode;
use harness_exec::TreeGone;
use harness_exec::hostgit::git;
use harness_exec::secretscan::scan_for_secrets;
use harness_exec::workspace::{
    Workspace, WorkspaceError, commit_candidate, create, packet_digest, reset_to, scan,
};
use tempfile::TempDir;

const T0: Timestamp = Timestamp(1_700_000_000_000_000);
const ANT_KEY: &str = concat!("sk-", "ant-", "api03-", "AbCdEfGhIjKlMnOpQrStUvWxYz012345");

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// The test's own Git: fixed identity, no user or system configuration.
fn tgit_raw(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("/usr/bin/git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Fixture Author")
        .env("GIT_AUTHOR_EMAIL", "author@fixture.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture Committer")
        .env("GIT_COMMITTER_EMAIL", "committer@fixture.invalid")
        .output()
        .unwrap()
}

fn tgit(dir: &Path, args: &[&str]) -> String {
    let out = tgit_raw(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim_end().to_owned()
}

struct Fixture {
    _tmp: TempDir,
    base_dir: PathBuf,
    repo: PathBuf,
    root: PathBuf,
    base: CommitSha,
}

fn write(path: &Path, contents: impl AsRef<[u8]>) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn fixture() -> Fixture {
    let tmp = TempDir::new().unwrap();
    // macOS temp dirs live behind the /var -> /private/var symlink.
    let base_dir = fs::canonicalize(tmp.path()).unwrap();
    let repo = base_dir.join("repo");
    let root = base_dir.join("worktrees");
    fs::create_dir_all(&repo).unwrap();
    fs::create_dir_all(&root).unwrap();
    tgit(&repo, &["init", "-q", "-b", "main"]);
    for file in [
        "requirements.md",
        "design.md",
        "acceptance-tests.md",
        "handoff.md",
    ] {
        write(
            &repo.join("specs/example").join(file),
            format!("# {file}\n"),
        );
    }
    write(&repo.join("specs/example/tasks.json"), "[]\n");
    write(&repo.join("answer.txt"), "before\n");
    tgit(&repo, &["add", "--all"]);
    tgit(&repo, &["commit", "-qm", "synthetic base"]);
    let base = CommitSha::parse(&tgit(&repo, &["rev-parse", "HEAD"])).unwrap();
    Fixture {
        _tmp: tmp,
        base_dir,
        repo,
        root,
        base,
    }
}

fn recommit(repo: &Path, message: &str) -> CommitSha {
    tgit(repo, &["add", "--all"]);
    tgit(repo, &["commit", "-qm", message]);
    CommitSha::parse(&tgit(repo, &["rev-parse", "HEAD"])).unwrap()
}

fn task(id: &str) -> TaskId {
    TaskId::new(id)
}

async fn workspace(f: &Fixture, id: &str) -> Workspace {
    create(&f.repo, &task(id), &f.base, &f.root).await.unwrap()
}

fn cp_ref(id: &str, n: u32) -> String {
    format!("refs/harness/tasks/{id}/cp/{n}")
}

fn ref_value(repo: &Path, name: &str) -> Option<String> {
    let out = tgit_raw(repo, &["rev-parse", "--verify", "-q", name]);
    out.status
        .success()
        .then(|| String::from_utf8(out.stdout).unwrap().trim().to_owned())
}

fn all_refs(repo: &Path) -> String {
    tgit(repo, &["for-each-ref", "--format=%(refname) %(objectname)"])
}

fn tree_entries(repo: &Path, commit: &CommitSha) -> String {
    tgit(repo, &["ls-tree", "-r", commit.as_str()])
}

fn set_executable(path: &Path) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

// ---------------------------------------------------------------------------------------------
// Plan tests
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn create_detaches_at_base_and_moves_no_branch() {
    let f = fixture();
    let before = all_refs(&f.repo);
    let ws = workspace(&f, "t-create").await;

    assert_eq!(ws.task, task("t-create"));
    assert_eq!(ws.path, f.root.join("t-create"));
    assert_eq!(ws.git_common_dir, f.repo.join(".git"));
    assert_eq!(tgit(&ws.path, &["rev-parse", "HEAD"]), f.base.as_str());
    // Detached: HEAD is not a symbolic ref.
    assert!(
        !tgit_raw(&ws.path, &["symbolic-ref", "-q", "HEAD"])
            .status
            .success()
    );
    assert_eq!(all_refs(&f.repo), before, "no ref may move or appear");
    assert_eq!(
        fs::read_to_string(ws.path.join("answer.txt")).unwrap(),
        "before\n"
    );
}

#[tokio::test]
async fn create_rejects_an_unsafe_task_id() {
    let f = fixture();
    for bad in [
        "", ".", "..", "../x", "a/b", ".hidden", "x.lock", "a b", "-x",
    ] {
        let r = create(&f.repo, &task(bad), &f.base, &f.root).await;
        assert!(
            matches!(r, Err(WorkspaceError::InvalidTaskId(_))),
            "{bad:?}: {r:?}"
        );
    }
}

#[tokio::test]
async fn commit_candidate_is_deterministic_for_same_inputs() {
    let f = fixture();
    let ws = workspace(&f, "t-det").await;
    write(&ws.path.join("answer.txt"), "after\n");

    let (first, diff) = commit_candidate(&ws, &f.base, 1, T0).await.unwrap();
    // A retry of the same checkpoint (e.g. after a crash) yields the same commit.
    let (again, diff_again) = commit_candidate(&ws, &f.base, 1, T0).await.unwrap();
    // The same tree + parent + time under another checkpoint number is the same commit.
    let (second, _) = commit_candidate(&ws, &f.base, 2, T0).await.unwrap();
    assert_eq!(first, again);
    assert_eq!(first, second);
    assert_eq!(diff, diff_again);
    assert_eq!(
        ref_value(&f.repo, &cp_ref("t-det", 1)).as_deref(),
        Some(first.as_str())
    );
    assert_eq!(
        ref_value(&f.repo, &cp_ref("t-det", 2)).as_deref(),
        Some(first.as_str())
    );

    // Identity and time come from the arguments, not from any configuration.
    let header = tgit(&f.repo, &["cat-file", "commit", first.as_str()]);
    assert!(
        header.contains("\nauthor eng <eng@localhost> 1700000000 +0000\n"),
        "{header}"
    );
    assert!(
        header.contains("\ncommitter eng <eng@localhost> 1700000000 +0000\n"),
        "{header}"
    );
    assert!(header.contains(&format!("parent {}\n", f.base)), "{header}");

    // A different time is a different commit.
    let (later, _) = commit_candidate(&ws, &f.base, 3, Timestamp(T0.0 + 1_000_000))
        .await
        .unwrap();
    assert_ne!(first, later);

    // An independent clone of the same base with the same change produces the same SHA.
    let f2_repo = f.base_dir.join("clone");
    tgit(
        &f.base_dir,
        &[
            "clone",
            "-q",
            f.repo.to_str().unwrap(),
            f2_repo.to_str().unwrap(),
        ],
    );
    let ws2 = create(&f2_repo, &task("t-det"), &f.base, &f.base_dir.join("wt2"))
        .await
        .unwrap();
    write(&ws2.path.join("answer.txt"), "after\n");
    let (cloned, _) = commit_candidate(&ws2, &f.base, 1, T0).await.unwrap();
    assert_eq!(first, cloned);
}

#[tokio::test]
async fn update_ref_is_compare_and_swap() {
    let f = fixture();
    let ws = workspace(&f, "t-cas").await;
    let name = cp_ref("t-cas", 1);
    // Someone else already owns checkpoint 1.
    tgit(&f.repo, &["update-ref", &name, f.base.as_str()]);
    write(&ws.path.join("answer.txt"), "after\n");

    let r = commit_candidate(&ws, &f.base, 1, T0).await;
    assert_eq!(r.map(|_| ()), Err(FailureCode::CandidateChanged));
    assert_eq!(ref_value(&f.repo, &name).as_deref(), Some(f.base.as_str()));

    // The low-level primitive: a wrong expected-old value fails and leaves the ref alone.
    let (other, _) = commit_candidate(&ws, &f.base, 2, T0).await.unwrap();
    let wrong_old = other.as_str();
    let r = git(&f.repo, &["update-ref", &name, other.as_str(), wrong_old]).await;
    assert!(r.is_err());
    assert_eq!(ref_value(&f.repo, &name).as_deref(), Some(f.base.as_str()));
}

#[tokio::test]
async fn repo_hooks_do_not_run_on_host_commit() {
    let f = fixture();
    let marker = f.base_dir.join("HOOK-RAN");
    let hostile_hooks = f.base_dir.join("hostile-hooks");
    fs::create_dir_all(&hostile_hooks).unwrap();
    let script = format!(
        "#!/bin/sh\ntouch '{}'\ncat >/dev/null 2>&1\nexit 0\n",
        marker.display()
    );
    for dir in [hostile_hooks.clone(), f.repo.join(".git/hooks")] {
        for hook in [
            "pre-commit",
            "post-commit",
            "post-checkout",
            "reference-transaction",
            "post-index-change",
        ] {
            write(&dir.join(hook), &script);
            set_executable(&dir.join(hook));
        }
    }
    let fsmonitor = f.base_dir.join("fsmonitor.sh");
    write(&fsmonitor, &script);
    set_executable(&fsmonitor);
    // Hostile settings in the repository's own .git/config.
    let alias = format!("!touch '{}'", marker.display());
    tgit(
        &f.repo,
        &["config", "core.hooksPath", hostile_hooks.to_str().unwrap()],
    );
    tgit(
        &f.repo,
        &["config", "core.fsmonitor", fsmonitor.to_str().unwrap()],
    );
    for name in [
        "add",
        "write-tree",
        "commit-tree",
        "update-ref",
        "diff",
        "worktree",
    ] {
        tgit(&f.repo, &["config", &format!("alias.{name}"), &alias]);
    }
    assert!(!marker.exists());

    let ws = workspace(&f, "t-hooks").await;
    write(&ws.path.join("answer.txt"), "after\n");
    let (sha, _) = commit_candidate(&ws, &f.base, 1, T0).await.unwrap();
    reset_to(&ws, &f.base, TreeGone::for_tests()).await.unwrap();
    assert!(
        !marker.exists(),
        "a repository hook, alias or fsmonitor ran on the host"
    );
    assert_eq!(
        ref_value(&f.repo, &cp_ref("t-hooks", 1)).as_deref(),
        Some(sha.as_str())
    );

    // The trap is armed: ordinary Git in the same repository does run the hook.
    tgit(&f.repo, &["commit", "-q", "--allow-empty", "-m", "armed"]);
    assert!(
        marker.exists(),
        "fixture hooks never fire; the test proves nothing"
    );
}

#[tokio::test]
async fn nested_repo_or_gitlink_is_git_metadata_touched() {
    let f = fixture();

    // Clean worktree passes.
    let ws = workspace(&f, "t-clean").await;
    write(&ws.path.join("new.txt"), "x\n");
    assert_eq!(scan(&ws).await, Ok(()));

    // A nested repository, under any letter case.
    for (id, dir) in [("t-nested", "sub/.git"), ("t-nested-case", "deep/er/.GiT")] {
        let ws = workspace(&f, id).await;
        fs::create_dir_all(ws.path.join(dir)).unwrap();
        write(&ws.path.join(dir).join("HEAD"), "ref: refs/heads/main\n");
        assert_eq!(
            scan(&ws).await,
            Err(FailureCode::GitMetadataTouched),
            "{dir}"
        );
    }

    // A `.git` file (worktree-style link) below the root is also a nested repository.
    let ws = workspace(&f, "t-nested-file").await;
    write(&ws.path.join("sub/.git"), "gitdir: /tmp/elsewhere\n");
    assert_eq!(scan(&ws).await, Err(FailureCode::GitMetadataTouched));

    // A gitlink in the index.
    let ws = workspace(&f, "t-gitlink").await;
    let cacheinfo = format!("160000,{},vendored", f.base);
    tgit(
        &ws.path,
        &["update-index", "--add", "--cacheinfo", &cacheinfo],
    );
    assert_eq!(scan(&ws).await, Err(FailureCode::GitMetadataTouched));

    // A changed worktree `.git` file.
    let ws = workspace(&f, "t-dotgit").await;
    write(
        &ws.path.join(".git"),
        format!("gitdir: {}\n", f.base_dir.join("evil").display()),
    );
    assert_eq!(scan(&ws).await, Err(FailureCode::GitMetadataTouched));

    // `.git` replaced by a directory.
    let ws = workspace(&f, "t-dotgit-dir").await;
    fs::remove_file(ws.path.join(".git")).unwrap();
    fs::create_dir(ws.path.join(".git")).unwrap();
    assert_eq!(scan(&ws).await, Err(FailureCode::GitMetadataTouched));

    // commit_candidate refuses as well and creates no ref.
    let ws = workspace(&f, "t-nested-commit").await;
    fs::create_dir_all(ws.path.join("sub/.git")).unwrap();
    let r = commit_candidate(&ws, &f.base, 1, T0).await;
    assert_eq!(r.map(|_| ()), Err(FailureCode::GitMetadataTouched));
    assert_eq!(ref_value(&f.repo, &cp_ref("t-nested-commit", 1)), None);
}

#[tokio::test]
async fn hidden_ref_is_not_pushed_by_default_refspec() {
    let f = fixture();
    let remote = f.base_dir.join("remote.git");
    tgit(
        &f.base_dir,
        &["init", "-q", "--bare", remote.to_str().unwrap()],
    );
    tgit(
        &f.repo,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );

    let ws = workspace(&f, "t-push").await;
    write(&ws.path.join("answer.txt"), "after\n");
    commit_candidate(&ws, &f.base, 1, T0).await.unwrap();
    assert!(ref_value(&f.repo, &cp_ref("t-push", 1)).is_some());

    tgit(&f.repo, &["push", "-q", "-u", "origin", "main"]);
    tgit(&f.repo, &["push", "-q"]);
    tgit(&f.repo, &["push", "-q", "--all", "origin"]);
    tgit(&f.repo, &["push", "-q", "--tags", "origin"]);
    let listed = tgit(&f.base_dir, &["ls-remote", remote.to_str().unwrap()]);
    assert!(listed.contains("refs/heads/main"), "{listed}");
    assert!(!listed.contains("refs/harness"), "{listed}");
}

#[tokio::test]
async fn token_shaped_string_in_added_line_fails_with_secret_in_diff() {
    let f = fixture();
    let ws = workspace(&f, "t-secret").await;
    write(
        &ws.path.join("src/config.js"),
        format!("// settings\nconst key = \"{ANT_KEY}\";\n"),
    );
    let r = commit_candidate(&ws, &f.base, 1, T0).await;
    assert_eq!(r.map(|_| ()), Err(FailureCode::SecretInDiff));
    assert_eq!(
        ref_value(&f.repo, &cp_ref("t-secret", 1)),
        None,
        "no ref on a secret"
    );

    // A token inside a file Git treats as binary is still found.
    let ws = workspace(&f, "t-secret-bin").await;
    let mut bytes = b"\0\x01binary\n".to_vec();
    bytes.extend_from_slice(ANT_KEY.as_bytes());
    bytes.push(b'\n');
    write(&ws.path.join("blob.bin"), bytes);
    let r = commit_candidate(&ws, &f.base, 1, T0).await;
    assert_eq!(r.map(|_| ()), Err(FailureCode::SecretInDiff));
    assert_eq!(ref_value(&f.repo, &cp_ref("t-secret-bin", 1)), None);
}

#[tokio::test]
async fn token_shaped_string_in_removed_line_is_ignored() {
    let f = fixture();
    write(
        &f.repo.join("legacy.env"),
        format!("KEY={ANT_KEY}\nOTHER=1\n"),
    );
    let base = recommit(&f.repo, "legacy secret");
    let ws = create(&f.repo, &task("t-removed"), &base, &f.root)
        .await
        .unwrap();
    write(&ws.path.join("legacy.env"), "OTHER=1\n");
    let (_, diff) = commit_candidate(&ws, &base, 1, T0).await.unwrap();
    assert!(diff.contains(&format!("-KEY={ANT_KEY}")), "{diff}");
    assert!(scan_for_secrets(&diff).is_empty());
}

#[tokio::test]
async fn packet_digest_changes_when_any_packet_file_changes() {
    let f = fixture();
    let (d0, files) = packet_digest(&f.repo, &f.base, "specs/example")
        .await
        .unwrap();
    let names: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        names,
        [
            "acceptance-tests.md",
            "design.md",
            "handoff.md",
            "requirements.md",
            "tasks.json"
        ]
    );
    assert!(files.contains(&("tasks.json".to_owned(), "[]\n".to_owned())));
    // Trailing slash is the same packet.
    let (d_slash, _) = packet_digest(&f.repo, &f.base, "specs/example/")
        .await
        .unwrap();
    assert_eq!(d0, d_slash);

    // A change outside the packet does not change the digest.
    write(&f.repo.join("answer.txt"), "outside\n");
    let outside = recommit(&f.repo, "outside");
    assert_eq!(
        packet_digest(&f.repo, &outside, "specs/example")
            .await
            .unwrap()
            .0,
        d0
    );

    // Any change to any packet file (content, new file, rename) changes it.
    let mut seen = vec![d0.clone()];
    for file in [
        "requirements.md",
        "design.md",
        "acceptance-tests.md",
        "handoff.md",
        "tasks.json",
    ] {
        let path = f.repo.join("specs/example").join(file);
        let mut text = fs::read_to_string(&path).unwrap();
        text.push_str("changed\n");
        write(&path, text);
        let c = recommit(&f.repo, file);
        let (d, _) = packet_digest(&f.repo, &c, "specs/example").await.unwrap();
        assert!(
            !seen.contains(&d),
            "{file} change did not change the digest"
        );
        seen.push(d);
    }
    write(&f.repo.join("specs/example/nested/extra.md"), "extra\n");
    let c = recommit(&f.repo, "add nested");
    let (d, files) = packet_digest(&f.repo, &c, "specs/example").await.unwrap();
    assert!(!seen.contains(&d));
    assert!(files.iter().any(|(p, _)| p == "nested/extra.md"));
    seen.push(d);
    tgit(
        &f.repo,
        &[
            "mv",
            "specs/example/nested/extra.md",
            "specs/example/nested/moved.md",
        ],
    );
    let c = recommit(&f.repo, "rename");
    let (d, _) = packet_digest(&f.repo, &c, "specs/example").await.unwrap();
    assert!(!seen.contains(&d), "a rename must change the digest");

    // The digest reads the commit, not the working directory.
    write(&f.repo.join("specs/example/design.md"), "uncommitted\n");
    assert_eq!(
        packet_digest(&f.repo, &c, "specs/example").await.unwrap().0,
        d
    );
}

#[tokio::test]
async fn packet_path_must_be_relative_without_dotdot() {
    let f = fixture();
    for bad in [
        "/etc",
        "../repo/specs/example",
        "specs/../specs/example",
        "specs/example/..",
        "",
        "specs//example",
        "./specs/example",
    ] {
        let r = packet_digest(&f.repo, &f.base, bad).await;
        assert!(
            matches!(r, Err(WorkspaceError::BadPacketPath(_))),
            "{bad:?}: {r:?}"
        );
    }
    let r = packet_digest(&f.repo, &f.base, "specs/missing").await;
    assert!(matches!(r, Err(WorkspaceError::PacketNotFound(_))), "{r:?}");
    let r = packet_digest(&f.repo, &f.base, "answer.txt").await;
    assert!(matches!(r, Err(WorkspaceError::BadPacketPath(_))), "{r:?}");
}

#[tokio::test]
async fn packet_files_must_be_utf8_text_blobs() {
    let f = fixture();
    write(
        &f.repo.join("specs/example/logo.bin"),
        [0xff_u8, 0xfe, 0x00],
    );
    let c = recommit(&f.repo, "binary");
    let r = packet_digest(&f.repo, &c, "specs/example").await;
    assert!(matches!(r, Err(WorkspaceError::PacketNotText(_))), "{r:?}");

    let f = fixture();
    std::os::unix::fs::symlink("design.md", f.repo.join("specs/example/link.md")).unwrap();
    let c = recommit(&f.repo, "symlink");
    let r = packet_digest(&f.repo, &c, "specs/example").await;
    assert!(matches!(r, Err(WorkspaceError::PacketNotText(_))), "{r:?}");
}

#[tokio::test]
async fn reset_to_restores_the_commit_exactly() {
    let f = fixture();
    let ws = workspace(&f, "t-reset").await;
    write(&ws.path.join("answer.txt"), "after\n");
    let (cp, _) = commit_candidate(&ws, &f.base, 1, T0).await.unwrap();
    write(&ws.path.join("answer.txt"), "scribbled\n");
    write(&ws.path.join("stray/new.txt"), "stray\n");
    write(&ws.path.join(".gitignore"), "ignored/\n");
    write(&ws.path.join("ignored/build.o"), "obj\n");
    fs::create_dir_all(ws.path.join("nested/.git")).unwrap();

    reset_to(&ws, &cp, TreeGone::for_tests()).await.unwrap();
    assert_eq!(tgit(&ws.path, &["rev-parse", "HEAD"]), cp.as_str());
    assert_eq!(
        fs::read_to_string(ws.path.join("answer.txt")).unwrap(),
        "after\n"
    );
    for gone in ["stray", ".gitignore", "ignored", "nested"] {
        assert!(!ws.path.join(gone).exists(), "{gone} survived reset");
    }
    assert_eq!(tgit(&ws.path, &["status", "--porcelain", "--ignored"]), "");
    assert_eq!(scan(&ws).await, Ok(()));
}

#[test]
fn secret_rules_report_every_hit_with_file_and_line() {
    let diff = [
        "diff --git a/a.txt b/a.txt",
        "--- a/a.txt",
        "+++ b/a.txt",
        "@@ -1,2 +1,4 @@",
        " keep",
        &format!("+{}", concat!("sk-", "proj-", "abcdefghijklmnopqrstuvwxyz")),
        "-gone",
        &format!(
            "+x {} y",
            concat!("sk-", "or-", "v1-abcdefghijklmnopqrstuv")
        ),
        &format!("+{}", concat!("AKIA", "ABCDEFGHIJKLMNOP")),
        "diff --git a/b.json b/b.json",
        "--- /dev/null",
        "+++ b/b.json",
        "@@ -0,0 +1,6 @@",
        &format!(
            "+{}",
            concat!("gh", "p_", "0123456789abcdefghijklmnopqrstuvwxyz")
        ),
        &format!(
            "+{}",
            concat!("gh", "o_", "0123456789abcdefghijklmnopqrstuvwxyz")
        ),
        &format!(
            "+{}",
            concat!("github", "_pat_", "11ABCDEFG0123456789_abcdefghij")
        ),
        &format!("+{}", concat!("-----BEGIN ", "PRIVATE KEY-----")),
        &format!("+{}", concat!("-----BEGIN OPENSSH ", "PRIVATE KEY-----")),
        &format!(
            "+{{\"{}\": \"{}\", \"{}\" : \"{}\"}}",
            concat!("access", "Token"),
            "abcdefghijklmnopqrstu",
            concat!("id", "_token"),
            "eyJhbGciOiJIUzI1NiJ9xyz"
        ),
        "",
    ]
    .join("\n");
    let hits: Vec<(String, u32, &str)> = scan_for_secrets(&diff)
        .into_iter()
        .map(|h| (h.file, h.line, h.rule))
        .collect();
    let at: Vec<(&str, u32)> = hits.iter().map(|(f, l, _)| (f.as_str(), *l)).collect();
    assert_eq!(
        at,
        [
            ("a.txt", 2),
            ("a.txt", 3),
            ("a.txt", 4),
            ("b.json", 1),
            ("b.json", 2),
            ("b.json", 3),
            ("b.json", 4),
            ("b.json", 5),
            ("b.json", 6),
        ],
        "{hits:?}"
    );
}

#[test]
fn secret_scan_ignores_headers_context_removed_lines_and_short_values() {
    let diff = [
        &format!("diff --git a/{0} b/{0}", "x"),
        &format!(
            "--- a/{}",
            concat!("sk-", "ant-", "abcdefghijklmnopqrstuvwxyz")
        ),
        "+++ b/x",
        "@@ -1,2 +1,4 @@",
        &format!(" {}", concat!("sk-", "ant-", "abcdefghijklmnopqrstuvwxyz")),
        &format!("-{}", concat!("AKIA", "ABCDEFGHIJKLMNOP")),
        "+mentions the sk-ant- prefix in prose",
        &format!("+{{\"{}\": \"short\"}}", concat!("refresh", "Token")),
        "+task-ant-is-not-a-key",
        "",
    ]
    .join("\n");
    assert_eq!(scan_for_secrets(&diff), vec![]);

    // An added line whose own text starts with "++" is content, not a header.
    let diff = format!(
        "diff --git a/y b/y\n--- a/y\n+++ b/y\n@@ -0,0 +1 @@\n+++{}\n",
        concat!("sk-", "ant-", "abcdefghijklmnopqrstuvwxyz")
    );
    let hits = scan_for_secrets(&diff);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!((hits[0].file.as_str(), hits[0].line), ("y", 1));
}

// ---------------------------------------------------------------------------------------------
// Pilot regressions (tests/local-workflow-regressions.test.mjs), one per case, same names.
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn bug1_nested_repo_clean_filter_cannot_write_to_a_host_path() {
    let f = fixture();
    let marker = f.base_dir.join("ESCAPED");
    let ws = workspace(&f, "t-bug1").await;
    let sub = ws.path.join("sub");
    fs::create_dir_all(&sub).unwrap();
    tgit(&sub, &["init", "-q"]);
    write(&sub.join("file.txt"), "original\n");
    tgit(&sub, &["add", "file.txt"]);
    tgit(&sub, &["commit", "-qm", "nested"]);
    let filter = format!("sh -c \"touch '{}'; cat\"", marker.display());
    tgit(&sub, &["config", "filter.x.clean", &filter]);
    write(&sub.join(".gitattributes"), "* filter=x\n");
    // Same size, newer mtime: Git would have to re-hash the file, running the clean filter.
    let later = SystemTime::now() + Duration::from_secs(60);
    fs::File::options()
        .write(true)
        .open(sub.join("file.txt"))
        .unwrap()
        .set_modified(later)
        .unwrap();
    write(&ws.path.join("answer.txt"), "after\n");

    let r = commit_candidate(&ws, &f.base, 1, T0).await;
    assert_eq!(r.map(|_| ()), Err(FailureCode::GitMetadataTouched));
    assert!(!marker.exists(), "nested repository filter ran on the host");
    reset_to(&ws, &f.base, TreeGone::for_tests()).await.unwrap();
    assert!(!marker.exists(), "nested repository filter ran on the host");
    assert!(!sub.exists());
}

#[tokio::test]
async fn bug2a_an_ignored_file_inside_the_packet_directory_still_yields_a_successful_run() {
    let f = fixture();
    write(&f.repo.join(".gitignore"), "ignored.txt\n");
    let base = recommit(&f.repo, "add gitignore");
    let (before, files_before) = packet_digest(&f.repo, &base, "specs/example")
        .await
        .unwrap();
    write(&f.repo.join("specs/example/ignored.txt"), "local scratch\n");
    assert_eq!(
        tgit(&f.repo, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    let (after, files_after) = packet_digest(&f.repo, &base, "specs/example")
        .await
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(files_before, files_after);
    let ws = create(&f.repo, &task("t-bug2a"), &base, &f.root)
        .await
        .unwrap();
    write(&ws.path.join("answer.txt"), "after\n");
    commit_candidate(&ws, &base, 1, T0).await.unwrap();
}

#[tokio::test]
async fn bug2b_a_packet_file_with_mode_0600_in_the_source_still_yields_a_successful_run() {
    let f = fixture();
    let (before, _) = packet_digest(&f.repo, &f.base, "specs/example")
        .await
        .unwrap();
    let file = f.repo.join("specs/example/requirements.md");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        tgit(&f.repo, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    let (after, files) = packet_digest(&f.repo, &f.base, "specs/example")
        .await
        .unwrap();
    assert_eq!(before, after);
    assert!(files.iter().any(|(p, _)| p == "requirements.md"));
    let ws = workspace(&f, "t-bug2b").await;
    write(&ws.path.join("answer.txt"), "after\n");
    commit_candidate(&ws, &f.base, 1, T0).await.unwrap();
}

#[tokio::test]
async fn bug3_a_candidate_file_with_mode_0o654_passes_the_snapshot_check() {
    let f = fixture();
    let ws = workspace(&f, "t-bug3").await;
    write(&ws.path.join("answer.txt"), "after\n");
    write(&ws.path.join("extra.bin"), "payload\n");
    fs::set_permissions(ws.path.join("extra.bin"), fs::Permissions::from_mode(0o654)).unwrap();
    let (cp, _) = commit_candidate(&ws, &f.base, 1, T0).await.unwrap();
    let tree = tree_entries(&f.repo, &cp);
    assert!(
        tree.lines()
            .any(|l| l.starts_with("100644 blob ") && l.ends_with("\textra.bin")),
        "{tree}"
    );
}

#[tokio::test]
async fn bug4_reviewer_patch_ends_with_a_newline_and_applies_with_git_apply_check() {
    let f = fixture();
    let ws = workspace(&f, "t-bug4").await;
    write(&ws.path.join("answer.txt"), "after\n");
    write(&ws.path.join("no-eol.txt"), "no trailing newline");
    write(&ws.path.join("image.bin"), [0u8, 1, 2, 3, 255, 0, 10]);
    fs::remove_file(ws.path.join("specs/example/tasks.json")).unwrap();
    write(&ws.path.join("dir with space/file.md"), "spaced\n");
    let (_, patch) = commit_candidate(&ws, &f.base, 1, T0).await.unwrap();
    assert!(patch.ends_with('\n'), "patch must terminate with a newline");

    let check = f.base_dir.join("applycheck");
    tgit(
        &f.repo,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            check.to_str().unwrap(),
            f.base.as_str(),
        ],
    );
    let patch_file = f.base_dir.join("candidate.patch");
    write(&patch_file, &patch);
    assert_eq!(
        tgit(&check, &["apply", "--check", patch_file.to_str().unwrap()]),
        ""
    );
}

#[test]
#[ignore = "pilot config: toolsDirectory does not exist here; runtimes are pinned executables \
            checked by version (Tasks 8 and 10)"]
fn bug5_prepare_rejects_a_tools_directory_that_contains_the_repository() {}

#[test]
#[ignore = "pilot config: runtimePaths does not exist here; sandbox paths are canonicalised by \
            Task 6 (profile_resolves_real_paths)"]
fn bug6a_runtime_paths_entry_escaping_via_dotdot_or_a_symlink_is_rejected() {}

#[test]
#[ignore = "pilot-only argv templating: LaunchPlan argv is never templated with {workspace}"]
fn bug6b_a_verifier_reviewer_argv_containing_workspace_is_rejected() {}

#[test]
#[ignore = "human decisions are store commands for an exact digest or commit (Tasks 3 and 9), \
            not signed JSON files parsed by exec"]
fn bug6c_a_decision_statement_with_a_duplicate_decision_key_is_rejected() {}

#[test]
#[ignore = "design differs: ignored files (build output such as target/) are never part of a \
            candidate, and reset_to removes them, so tested bytes equal committed bytes; see \
            ignored_worker_artifact_is_excluded_from_the_candidate_and_removed_by_reset"]
fn bug7_an_ignored_worker_artifact_fails_the_run_and_the_error_names_the_path() {}

#[tokio::test]
async fn ignored_worker_artifact_is_excluded_from_the_candidate_and_removed_by_reset() {
    let f = fixture();
    let ws = workspace(&f, "t-bug7").await;
    write(&ws.path.join("answer.txt"), "after\n");
    write(&ws.path.join(".gitignore"), "__pycache__/\n");
    write(&ws.path.join("__pycache__/a.pyc"), "bytecode\n");
    let (cp, diff) = commit_candidate(&ws, &f.base, 1, T0).await.unwrap();
    assert!(!tree_entries(&f.repo, &cp).contains("__pycache__"));
    assert!(!diff.contains("__pycache__/a.pyc"));
    reset_to(&ws, &cp, TreeGone::for_tests()).await.unwrap();
    assert!(!ws.path.join("__pycache__").exists());
    assert_eq!(tgit(&ws.path, &["status", "--porcelain", "--ignored"]), "");
}

#[tokio::test]
async fn bug8_a_packet_file_missing_from_the_commit_is_rejected_even_if_present_and_ignored_on_disk()
 {
    let f = fixture();
    tgit(
        &f.repo,
        &["rm", "-q", "--cached", "specs/example/handoff.md"],
    );
    write(&f.repo.join(".gitignore"), "specs/example/handoff.md\n");
    let c = recommit(&f.repo, "untrack handoff");
    assert_eq!(
        tgit(&f.repo, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    assert!(f.repo.join("specs/example/handoff.md").exists());
    let (_, files) = packet_digest(&f.repo, &c, "specs/example").await.unwrap();
    assert!(
        files.iter().all(|(p, _)| p != "handoff.md"),
        "the packet must come from the commit, not the disk"
    );
}

#[tokio::test]
async fn bug9_a_store_reached_through_a_symlink_into_the_repository_is_rejected_before_creation() {
    // Ported to the worktree root: a root that resolves into the source checkout is refused
    // before anything is created there.
    let f = fixture();
    let link = f.base_dir.join("into-repo");
    std::os::unix::fs::symlink(&f.repo, &link).unwrap();
    let before = tgit(
        &f.repo,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--ignored",
        ],
    );
    let r = create(
        &f.repo,
        &task("t-bug9"),
        &f.base,
        &link.join("state/worktrees"),
    )
    .await;
    assert!(matches!(r, Err(WorkspaceError::RootInsideRepo(_))), "{r:?}");
    assert!(!f.repo.join("state").exists());
    let r = create(&f.repo, &task("t-bug9"), &f.base, &link).await;
    assert!(matches!(r, Err(WorkspaceError::RootInsideRepo(_))), "{r:?}");
    assert!(!f.repo.join("t-bug9").exists());
    assert_eq!(
        tgit(
            &f.repo,
            &[
                "status",
                "--porcelain",
                "--untracked-files=all",
                "--ignored"
            ]
        ),
        before
    );
    assert_eq!(
        tgit(&f.repo, &["worktree", "list", "--porcelain"])
            .matches("worktree ")
            .count(),
        1
    );
}
