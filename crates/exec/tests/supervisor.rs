//! Process supervisor: gated spawn, bounded streams, process-tree kill (plan Task 5).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::time::{Duration, Instant};

use harness_core::outcome::FailureCode;
use harness_exec::LaunchPlan;
use harness_exec::supervisor::{
    ExitInfo, KillFailed, Limits, LineSink, ProcessIdentity, SpawnAbort, Stream, boot_id,
    descendant_tracking_available, identity_is_alive, spawn_gated,
};
use tokio_util::sync::CancellationToken;

fn shim() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_eng-shim"))
}

fn sh(script: &str, cwd: &Path) -> LaunchPlan {
    LaunchPlan {
        argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
        env: vec![("PATH".into(), "/usr/bin:/bin".into())],
        cwd: cwd.to_path_buf(),
        stdin: Vec::new(),
    }
}

fn limits(deadline: Duration) -> Limits {
    Limits {
        deadline,
        max_output_bytes: 64 << 20,
        max_line_bytes: 1 << 20,
    }
}

#[derive(Default)]
struct Collect(Vec<(Stream, Vec<u8>)>);

impl LineSink for Collect {
    fn line(&mut self, stream: Stream, bytes: &[u8]) {
        self.0.push((stream, bytes.to_vec()));
    }
}

impl Collect {
    fn count(&self, stream: Stream) -> usize {
        self.0.iter().filter(|(s, _)| *s == stream).count()
    }
}

/// Spawns, runs to completion, and returns the exit info; panics if the kill sequence failed.
async fn run(plan: LaunchPlan, limits: Limits, cancel: CancellationToken) -> (ExitInfo, Collect) {
    let supervised = spawn_gated(shim(), plan, limits, |_| Ok(()))
        .await
        .expect("spawn");
    let mut sink = Collect::default();
    let (exit, gone) = supervised.run(&mut sink, cancel).await;
    let _proof = gone.expect("tree gone");
    (exit, sink)
}

/// Dead means: no such process, or a zombie (exited, not yet reaped by its new parent).
fn pid_is_dead(pid: i32) -> bool {
    // SAFETY: proc_pidinfo writes at most `size` bytes into a zeroed, correctly sized struct.
    unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let n = libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size);
        n != size || info.pbi_status == libc::SZOMB
    }
}

#[tokio::test]
async fn shim_runs_nothing_if_identity_callback_fails() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("marker");
    let plan = sh("touch marker", dir.path());

    let mut seen: Option<ProcessIdentity> = None;
    let result = spawn_gated(
        shim(),
        plan.clone(),
        limits(Duration::from_secs(10)),
        |id| {
            seen = Some(id);
            Err(SpawnAbort("intent not recorded".into()))
        },
    )
    .await;
    assert_eq!(result.err(), Some(FailureCode::SpawnError));
    let id = seen.expect("callback ran with an identity");
    assert_eq!(id.pgid, id.pid);
    assert_eq!(id.boot_id, boot_id());
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !marker.exists(),
        "the shim must not run the plan when the callback fails"
    );
    assert!(!identity_is_alive(&id));

    // Control: the same plan runs when the callback succeeds.
    let (exit, _) = run(
        plan,
        limits(Duration::from_secs(10)),
        CancellationToken::new(),
    )
    .await;
    assert_eq!(exit.code, Some(0));
    assert!(marker.exists());
}

#[tokio::test]
async fn deadline_kills_group_and_reports_deadline_exceeded() {
    let dir = tempfile::tempdir().unwrap();
    // A background child in the same group must die too.
    let plan = sh("sleep 100 & echo $! > bg.pid; sleep 100", dir.path());
    let started = Instant::now();
    let (exit, _) = run(
        plan,
        limits(Duration::from_millis(400)),
        CancellationToken::new(),
    )
    .await;
    assert_eq!(exit.failure, Some(FailureCode::DeadlineExceeded));
    assert!(!exit.cancelled);
    assert!(exit.code.is_none() && exit.signal.is_some(), "{exit:?}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "SIGTERM should end it within the grace"
    );
    let bg: i32 = std::fs::read_to_string(dir.path().join("bg.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(pid_is_dead(bg));
}

#[tokio::test]
async fn detached_grandchild_is_killed_before_tree_gone() {
    assert!(descendant_tracking_available());
    let dir = tempfile::tempdir().unwrap();
    // The child double-forks; the grandchild calls setsid (new session and group), is reparented
    // to launchd when its parent exits, closes our pipes, and records its pid. The leader then
    // lingers briefly and exits 0, leaving the grandchild running.
    let script = r#"/usr/bin/perl -MPOSIX -e '
        if (fork() == 0) {
            setsid();
            if (fork() == 0) {
                open(STDIN, "<", "/dev/null"); open(STDOUT, ">", "/dev/null"); open(STDERR, ">", "/dev/null");
                open(F, ">", "gc.tmp"); print F $$; close(F); rename("gc.tmp", "gc.pid");
                exec("sleep", "60");
            }
            exit 0;
        }
        wait();
        select(undef, undef, undef, 0.01) until -e "gc.pid";
    '; sleep 0.2"#;
    let (exit, _) = run(
        sh(script, dir.path()),
        limits(Duration::from_secs(20)),
        CancellationToken::new(),
    )
    .await;
    assert_eq!(exit.code, Some(0), "{exit:?}");
    assert_eq!(exit.failure, None);
    let gc: i32 = std::fs::read_to_string(dir.path().join("gc.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(
        pid_is_dead(gc),
        "detached grandchild {gc} survived TreeGone"
    );
}

#[tokio::test]
async fn output_cap_triggers_kill() {
    let dir = tempfile::tempdir().unwrap();
    let mut l = limits(Duration::from_secs(20));
    l.max_output_bytes = 1 << 20;
    let (exit, sink) = run(sh("yes", dir.path()), l, CancellationToken::new()).await;
    assert_eq!(exit.failure, Some(FailureCode::OutputLimitExceeded));
    let delivered: usize = sink.0.iter().map(|(_, b)| b.len() + 1).sum();
    assert!(delivered as u64 <= 1 << 20);
}

#[tokio::test]
async fn oversized_line_is_a_failure_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    let mut l = limits(Duration::from_secs(20));
    l.max_line_bytes = 1000;
    let plan = sh(
        "echo ok; head -c 100000 /dev/zero | tr '\\0' a; echo; sleep 100",
        dir.path(),
    );
    let (exit, sink) = run(plan, l, CancellationToken::new()).await;
    assert_eq!(exit.failure, Some(FailureCode::OutputLimitExceeded));
    assert_eq!(sink.0.first(), Some(&(Stream::Stdout, b"ok".to_vec())));
    assert!(sink.0.iter().all(|(_, b)| b.len() <= 1000));
}

#[tokio::test]
async fn stderr_flood_does_not_deadlock() {
    let dir = tempfile::tempdir().unwrap();
    // 5 MB of stderr in 100-byte lines while stdout and stdin are also in use.
    let mut plan = sh(
        "cat > in.txt; head -c 5000000 /dev/zero | tr '\\0' e | fold -w 100 >&2; echo done",
        dir.path(),
    );
    plan.stdin = vec![b'i'; 1 << 20];
    let (exit, sink) = tokio::time::timeout(
        Duration::from_secs(60),
        run(
            plan,
            limits(Duration::from_secs(50)),
            CancellationToken::new(),
        ),
    )
    .await
    .expect("no deadlock");
    assert_eq!(exit.code, Some(0), "{exit:?}");
    assert_eq!(exit.failure, None);
    assert_eq!(sink.count(Stream::Stderr), 50_000);
    // Ordering across the two streams is not defined; within stdout it is.
    let stdout: Vec<&[u8]> = sink
        .0
        .iter()
        .filter(|(s, _)| *s == Stream::Stdout)
        .map(|(_, b)| b.as_slice())
        .collect();
    assert_eq!(stdout, vec![b"done".as_slice()]);
    assert_eq!(
        std::fs::metadata(dir.path().join("in.txt")).unwrap().len(),
        1 << 20
    );
}

#[tokio::test]
async fn cancel_token_yields_cancelled_exit() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        trigger.cancel();
    });
    let started = Instant::now();
    let (exit, _) = run(
        sh("sleep 100", dir.path()),
        limits(Duration::from_secs(60)),
        cancel,
    )
    .await;
    assert!(exit.cancelled, "{exit:?}");
    assert_eq!(exit.failure, None);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn kill_identity_ends_a_run_from_its_identity_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mut identity = None;
    let supervised = spawn_gated(
        shim(),
        sh("sleep 100", dir.path()),
        limits(Duration::from_secs(60)),
        |id| {
            identity = Some(id);
            Ok(())
        },
    )
    .await
    .unwrap();
    let id = identity.unwrap();
    let cancel = CancellationToken::new();
    let runner = tokio::spawn(async move {
        let mut sink = Collect::default();
        supervised.run(&mut sink, cancel).await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(identity_is_alive(&id));
    let killed: Result<_, KillFailed> = harness_exec::supervisor::kill_identity(&id).await;
    let _proof = killed.expect("killed");
    assert!(!identity_is_alive(&id));
    let (exit, gone) = runner.await.unwrap();
    assert!(gone.is_ok());
    assert!(exit.signal.is_some(), "{exit:?}");
}
