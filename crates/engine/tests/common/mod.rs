//! Shared fixtures for the engine's integration tests: a throwaway repository with a spec
//! packet, fake agents (`conformance/fixtures/fake-*.mjs`) behind scenario wrapper scripts,
//! and helpers to create and start a task.

#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use harness_core::artifact::MediaType;
use harness_core::decide::{Command, Task};
use harness_core::ids::{CommitSha, IdempotencyKey, TaskId};
use harness_core::lifecycle::{DecisionKind, Subject};
use harness_core::types::{Actor, Envelope, RepoRef, WIRE_VERSION};
use harness_engine::{Engine, EngineConfig, new_id, now};
use harness_exec::workspace::{encode_packet, packet_digest};

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(tag: &str) -> TempDir {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!(
            "eng-engine-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        TempDir(std::fs::canonicalize(&p).unwrap())
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The test's own Git: fixed identity, no user or system configuration.
pub fn tgit(dir: &Path, args: &[&str]) -> String {
    let out = Process::new("/usr/bin/git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

pub fn try_tgit(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Process::new("/usr/bin/git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env_clear()
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// `node` from the test's `PATH` (Node 24 is a repository requirement).
pub fn node() -> PathBuf {
    let path = std::env::var_os("PATH").expect("PATH");
    std::env::split_paths(&path)
        .map(|d| d.join("node"))
        .find(|p| p.is_file())
        .map(|p| std::fs::canonicalize(p).unwrap())
        .expect("node on PATH")
}

pub fn fixtures() -> PathBuf {
    std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/fixtures"))
        .unwrap()
}

pub fn shim() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_eng-test-shim"))
}

pub struct Fixture {
    pub tmp: TempDir,
    pub repo: PathBuf,
    pub base: CommitSha,
    pub state: PathBuf,
}

pub const PACKET: &str = "specs/demo";

pub fn fixture() -> Fixture {
    let tmp = TempDir::new("fx");
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join(PACKET)).unwrap();
    tgit(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("AGENTS.md"), "Use small commits.\n").unwrap();
    std::fs::write(
        repo.join(PACKET).join("requirements.md"),
        "# Demo\n\nCreate feature.txt.\n",
    )
    .unwrap();
    std::fs::write(repo.join("README.md"), "demo\n").unwrap();
    tgit(&repo, &["add", "-A"]);
    tgit(&repo, &["commit", "-q", "-m", "base"]);
    let base = CommitSha::parse(&tgit(&repo, &["rev-parse", "HEAD"])).unwrap();
    let state = tmp.path().join("state");
    Fixture {
        tmp,
        repo,
        base,
        state,
    }
}

/// A script that runs `fake` with `FAKE_SCENARIO=<scenario>`.
pub fn wrapper(f: &Fixture, fake: &str, scenario: &str) -> PathBuf {
    let dir = f.tmp.path().join("bin");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{fake}-{scenario}"));
    let script = format!(
        "#!/bin/sh\nFAKE_SCENARIO={scenario}\nexport FAKE_SCENARIO\nexec '{}' '{}' \"$@\"\n",
        node().display(),
        fixtures().join(format!("{fake}.mjs")).display()
    );
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Like [`wrapper`], with further exported variables.
pub fn wrapper_env(f: &Fixture, fake: &str, scenario: &str, vars: &[(&str, &str)]) -> PathBuf {
    let path = wrapper(f, fake, scenario);
    let script = std::fs::read_to_string(&path).unwrap();
    let exports: String = vars
        .iter()
        .map(|(k, v)| format!("{k}='{v}'\nexport {k}\n"))
        .collect();
    let script = script.replacen("exec ", &format!("{exports}exec "), 1);
    std::fs::write(&path, script).unwrap();
    path
}

/// Fake Claude and fake Codex in the given scenarios, launched through wrapper scripts.
pub fn config(f: &Fixture, claude: &str, codex: &str) -> EngineConfig {
    EngineConfig {
        state_dir: f.state.clone(),
        claude_bin: wrapper(f, "fake-claude", claude),
        codex_bin: wrapper(f, "fake-codex", codex),
        node_bin: None,
        codex_uses_node: false,
        shim_bin: shim(),
        extra_read_dirs: vec![fixtures(), node().parent().unwrap().to_path_buf()],
    }
}

pub fn pipeline(verify: &[&str]) -> String {
    let verify: Vec<String> = verify.iter().map(|a| format!("{a:?}")).collect();
    format!(
        "verify_commands = [[{}]]\nrun_deadline_ms = 60000\nmax_turns = 5\n\n\
         [implement]\nruntime = \"claude\"\n\n[review]\nruntime = \"codex\"\n",
        verify.join(", ")
    )
}

pub fn default_pipeline() -> String {
    pipeline(&["sh", "-c", "test -f feature.txt"])
}

pub fn human() -> Actor {
    Actor::Human {
        principal: "owner".into(),
    }
}

pub fn envelope(task: &TaskId, actor: Actor, body: Command) -> Envelope<Command> {
    Envelope {
        v: WIRE_VERSION,
        id: new_id(),
        task: task.clone(),
        at: now(),
        actor,
        body,
    }
}

pub async fn submit_ok(engine: &Engine, cmd: Envelope<Command>) {
    let r = engine.submit(cmd).await.unwrap();
    assert!(r.accepted, "rejected: {:?}", r.rejection);
}

/// Creates the task, approves its packet and submits `Run`.
pub async fn start_task(engine: &Engine, f: &Fixture, pipeline_toml: &str) -> TaskId {
    let (digest, files) = packet_digest(&f.repo, &f.base, PACKET).await.unwrap();
    let packet = engine
        .put_input(&encode_packet(&files), MediaType::Text)
        .unwrap();
    assert_eq!(packet, digest);
    let pipeline = engine
        .put_input(pipeline_toml.as_bytes(), MediaType::Text)
        .unwrap();
    let task = TaskId::new(new_id());
    let common = tgit(
        &f.repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    );
    submit_ok(
        engine,
        envelope(
            &task,
            human(),
            Command::CreateTask {
                repo: RepoRef {
                    path: f.repo.to_string_lossy().into_owned(),
                    git_common_dir: common,
                },
                base_commit: f.base.clone(),
                packet: packet.clone(),
                pipeline,
                key: IdempotencyKey::new("create"),
            },
        ),
    )
    .await;
    submit_ok(
        engine,
        envelope(
            &task,
            human(),
            Command::RecordHumanDecision {
                kind: DecisionKind::ApproveSpec,
                subject: Subject::PacketDigest(packet),
                key: IdempotencyKey::new("approve"),
            },
        ),
    )
    .await;
    submit_ok(
        engine,
        envelope(
            &task,
            human(),
            Command::Run {
                key: IdempotencyKey::new("run"),
            },
        ),
    )
    .await;
    task
}

pub fn worktree(f: &Fixture, task: &TaskId) -> PathBuf {
    std::fs::canonicalize(f.state.join("worktrees"))
        .unwrap()
        .join(task.as_str())
}

pub fn cp_ref(task: &TaskId, n: u32) -> String {
    format!("refs/harness/tasks/{task}/cp/{n}")
}

pub fn events(engine: &Engine) -> Vec<harness_core::decide::Event> {
    engine
        .events_after(harness_core::ids::Seq(0), 100_000)
        .unwrap()
        .into_iter()
        .map(|r| r.event.body)
        .collect()
}

pub fn task(engine: &Engine, id: &TaskId) -> Task {
    engine.task(id).unwrap().unwrap()
}

/// Opens the store of a closed engine. Another test's child process may hold a copy of the
/// engine lock descriptor for the instant between its fork and exec (the descriptor is
/// close-on-exec), so `Locked` is retried briefly.
pub fn reopen_store(state: &Path) -> harness_store::Store {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match harness_store::Store::open(state) {
            Err(harness_store::StoreError::Locked) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            other => return other.unwrap(),
        }
    }
}

pub fn pid_alive(pid: i32) -> bool {
    Process::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}
