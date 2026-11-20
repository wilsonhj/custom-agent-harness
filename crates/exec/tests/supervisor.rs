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
#[ignore = "R-11: strong descendant containment backend unavailable; production execution is disabled"]
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
#[ignore = "R-11: strong descendant containment backend unavailable; production execution is disabled"]
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

/// The child double-forks; the grandchild calls setsid (new session and group), is reparented
/// to launchd when its parent exits, closes our pipes, and records its pid. `linger` is how long
/// the plan stays alive after that; with none, the plan exits within microseconds of the escape.
fn detached_grandchild_script(linger: &str) -> String {
    format!(
        r#"/usr/bin/perl -MPOSIX -e '
        if (fork() == 0) {{
            setsid();
            if (fork() == 0) {{
                open(STDIN, "<", "/dev/null"); open(STDOUT, ">", "/dev/null"); open(STDERR, ">", "/dev/null");
                open(F, ">", "gc.tmp"); print F $$; close(F); rename("gc.tmp", "gc.pid");
                exec("sleep", "60");
            }}
            exit 0;
        }}
        wait();
        select(undef, undef, undef, 0.001) until -e "gc.pid";
    '{linger}"#
    )
}

/// Runs the script and asserts that a `TreeGone` was returned only with the grandchild dead.
async fn assert_grandchild_dead_at_tree_gone(linger: &str) {
    let dir = tempfile::tempdir().unwrap();
    let supervised = spawn_gated(
        shim(),
        sh(&detached_grandchild_script(linger), dir.path()),
        limits(Duration::from_secs(20)),
        |_| Ok(()),
    )
    .await
    .expect("spawn");
    let mut sink = Collect::default();
    let (exit, gone) = supervised.run(&mut sink, CancellationToken::new()).await;
    assert_eq!(exit.code, Some(0), "{exit:?}");
    assert_eq!(exit.failure, None);
    let gc: i32 = std::fs::read_to_string(dir.path().join("gc.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let dead = pid_is_dead(gc);
    if !dead {
        // SAFETY: kill on the pid this test's own plan recorded.
        unsafe { libc::kill(gc, libc::SIGKILL) };
    }
    let _proof = gone.expect("tree gone");
    assert!(dead, "detached grandchild {gc} survived TreeGone");
}

#[tokio::test]
#[ignore = "R-11: strong descendant containment backend unavailable; production execution is disabled"]
async fn detached_grandchild_is_killed_before_tree_gone() {
    assert!(descendant_tracking_available());
    assert_grandchild_dead_at_tree_gone("; sleep 0.2").await;
}

/// The escape and the plan's exit fall within one poll interval: no scan ever sees the
/// grandchild while its parent chain is alive.
#[tokio::test]
#[ignore = "R-11: strong descendant containment backend unavailable; production execution is disabled"]
async fn detached_grandchild_escaping_as_the_plan_exits_is_killed_before_tree_gone() {
    assert!(descendant_tracking_available());
    for _ in 0..5 {
        assert_grandchild_dead_at_tree_gone("").await;
    }
}

#[tokio::test]
#[ignore = "R-11: strong descendant containment backend unavailable; production execution is disabled"]
async fn plan_inherits_neither_the_gate_nor_the_status_fd() {
    let dir = tempfile::tempdir().unwrap();
    // A plan that could write fd 4 could fake its own exit status; one that held fd 3 could
    // keep the anchor from being released.
    let (exit, sink) = run(
        sh(
            "for fd in 3 4; do if (: >&$fd) 2>/dev/null; then echo open$fd; fi; done; exit 7",
            dir.path(),
        ),
        limits(Duration::from_secs(20)),
        CancellationToken::new(),
    )
    .await;
    assert_eq!(sink.count(Stream::Stdout), 0, "{:?}", sink.0);
    assert_eq!(exit.code, Some(7), "{exit:?}");
    assert_eq!(exit.failure, Some(FailureCode::ExitNonzero));
}

#[tokio::test]
#[ignore = "R-11: strong descendant containment backend unavailable; production execution is disabled"]
async fn plan_that_cannot_start_reports_exec_failed_and_tree_gone() {
    let dir = tempfile::tempdir().unwrap();
    let mut plan = sh("", dir.path());
    plan.argv = vec!["/nonexistent/eng-no-such-binary".into()];
    let (exit, _) = run(
        plan,
        limits(Duration::from_secs(20)),
        CancellationToken::new(),
    )
    .await;
    assert_eq!(exit.code, Some(harness_exec::shim::EXEC_FAILED), "{exit:?}");
}

/// Without the anchor the responsibility rule no longer holds, so the supervisor cannot
/// establish that nothing escaped: it must fail closed even though the tree is in fact empty.
#[tokio::test]
#[ignore = "R-11: strong descendant containment backend unavailable; production execution is disabled"]
async fn plan_that_kills_the_anchor_yields_kill_failed() {
    let dir = tempfile::tempdir().unwrap();
    let supervised = spawn_gated(
        shim(),
        sh("kill -9 $PPID; sleep 0.2", dir.path()),
        limits(Duration::from_secs(20)),
        |_| Ok(()),
    )
    .await
    .expect("spawn");
    let mut sink = Collect::default();
    let (_, gone) = supervised.run(&mut sink, CancellationToken::new()).await;
    assert!(gone.is_err(), "TreeGone without the anchor");
}

#[tokio::test]
#[ignore = "R-11: strong descendant containment backend unavailable; production execution is disabled"]
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
#[ignore = "R-11: strong descendant containment backend unavailable; production execution is disabled"]
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
#[ignore = "R-11: strong descendant containment backend unavailable; production execution is disabled"]
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
#[ignore = "R-11: strong descendant containment backend unavailable; production execution is disabled"]
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

/// `proc_bsdinfo` start time in microseconds, if the process exists.
fn start_us(pid: i32) -> Option<i64> {
    // SAFETY: proc_pidinfo writes at most `size` bytes into a zeroed, correctly sized struct.
    unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let n = libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size);
        (n == size).then(|| (info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec) as i64)
    }
}

/// After a restart the recorded process group number may have been reused by an unrelated
/// process group of the same user. `kill_identity` must leave that group alone, whether the
/// recorded leader pid is the reused group leader itself or a pid that no longer exists.
#[tokio::test]
async fn kill_identity_spares_an_unrelated_group_that_reused_the_pgid() {
    use std::os::unix::process::CommandExt as _;
    let dir = tempfile::tempdir().unwrap();
    // An unrelated, long-lived process group: a leader and one member.
    let mut unrelated = std::process::Command::new("/bin/sh")
        .args(["-c", "sleep 60 & echo $! > member.pid; wait"])
        .current_dir(dir.path())
        .process_group(0)
        .spawn()
        .unwrap();
    let leader = unrelated.id() as i32;
    let member_file = dir.path().join("member.pid");
    let started = Instant::now();
    while !member_file.exists() && started.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(50));
    let member: i32 = std::fs::read_to_string(&member_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let leader_start = start_us(leader).unwrap();

    // A pid that existed once and is gone now.
    let mut gone = std::process::Command::new("/usr/bin/true").spawn().unwrap();
    let gone_pid = gone.id() as i32;
    gone.wait().unwrap();

    // Recorded start times earlier than the unrelated processes', so no start-time filter
    // excludes them: only the group-leader check can.
    for recorded_pid in [leader, gone_pid] {
        let forged = ProcessIdentity {
            pgid: leader,
            pid: recorded_pid,
            start_us: leader_start - 1_000_000,
            boot_id: boot_id(),
        };
        assert!(
            !identity_is_alive(&forged),
            "pid {recorded_pid}: an unrelated group is not our run"
        );
        let killed = harness_exec::supervisor::kill_identity(&forged).await;
        assert!(
            killed.is_err(),
            "same-boot identity cannot prove containment: {killed:?}"
        );
        assert!(
            !pid_is_dead(leader),
            "pid {recorded_pid}: unrelated leader was killed"
        );
        assert!(
            !pid_is_dead(member),
            "pid {recorded_pid}: unrelated member was killed"
        );
    }
    // SAFETY: killpg on the group this test created.
    unsafe { libc::killpg(leader, libc::SIGKILL) };
    let _ = unrelated.wait();
}

#[tokio::test]
#[ignore = "R-11: strong descendant containment backend unavailable; production execution is disabled"]
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

#[tokio::test]
async fn unavailable_containment_denies_before_shim_callback_or_plan() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let fake_shim = dir.path().join("shim");
    std::fs::write(&fake_shim, "#!/bin/sh\ntouch shim-marker\nexit 0\n").unwrap();
    std::fs::set_permissions(&fake_shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    for shim_path in [fake_shim.as_path(), shim()] {
        let mut callback_ran = false;
        let result = spawn_gated(
            shim_path,
            sh("touch plan-marker", dir.path()),
            limits(Duration::from_secs(1)),
            |_| {
                callback_ran = true;
                Err(SpawnAbort("test stops any baseline launch".into()))
            },
        )
        .await;
        assert!(!callback_ran, "identity callback ran without containment");
        assert!(
            !dir.path().join("shim-marker").exists(),
            "shim ran without containment"
        );
        assert!(!dir.path().join("plan-marker").exists());
        assert_eq!(result.err(), Some(FailureCode::ContainmentUnavailable));
    }
}

#[tokio::test]
async fn same_or_unknown_boot_without_a_leader_cannot_prove_tree_gone() {
    for recorded_boot in [boot_id(), String::new()] {
        let id = ProcessIdentity {
            pid: i32::MAX,
            pgid: i32::MAX,
            start_us: 1,
            boot_id: recorded_boot,
        };
        assert!(harness_exec::supervisor::kill_identity(&id).await.is_err());
    }
}

#[tokio::test]
async fn a_confirmed_reboot_proves_old_processes_gone() {
    let current = boot_id();
    assert!(
        !current.is_empty(),
        "test requires a readable boot identity"
    );
    let id = ProcessIdentity {
        pid: i32::MAX,
        pgid: i32::MAX,
        start_us: 1,
        boot_id: format!("previous-{current}"),
    };
    assert!(harness_exec::supervisor::kill_identity(&id).await.is_ok());
}
