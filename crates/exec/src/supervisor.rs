//! Process supervisor: gated spawn, bounded output streams, and process-tree termination that
//! ends with a [`TreeGone`] proof.
//!
//! # Launch
//!
//! [`spawn_gated`] starts the `eng-shim` binary (see [`crate::shim`]) with the plan's argv,
//! complete environment and working directory, and the read end of a gate pipe on fd 3. The
//! shim makes itself a session and process-group leader and waits. `on_identity` receives the
//! leader's [`ProcessIdentity`] before anything runs; if it fails, the gate is dropped and the
//! shim exits having run nothing. [`Supervised::run`] writes the go byte, after which the shim
//! execs the plan; the plan's stdin is then written from its own task (`EPIPE` ignored), and
//! stdout and stderr are drained concurrently through length-limited line framing.
//!
//! # Finding every descendant on macOS
//!
//! macOS has no PID namespaces or cgroups. A process that double-forks and calls `setsid`
//! leaves the process group, and once its parent exits it is reparented to launchd, so neither
//! `killpg` nor a parent-chain walk at teardown finds it (research 1a §6). The supervisor
//! therefore matches a process as "ours" if any of the following holds, evaluated against a
//! snapshot of every process taken with the public libproc calls `proc_listallpids` and
//! `proc_pidinfo(PROC_PIDTBSDINFO)` (pid, ppid, pgid, uid, status, start time):
//!
//! 1. **Recorded.** From before the go byte until teardown a background thread rescans every
//!    [`POLL_INTERVAL`] and records `(pid, start time)` of each match. A recorded pid matches
//!    for the rest of the run as long as its start time is unchanged (so a reused pid never
//!    does), even after it has been reparented to launchd.
//! 2. **Group or ancestry.** It is in the leader's process group, or its parent matches and it
//!    started no earlier than that parent.
//! 3. **Responsibility** (private SPI, isolated in [`responsibility_spi`]). The shim execs the
//!    plan with the "disclaim" spawn attribute, which makes the leader its own responsible
//!    process. Every descendant inherits that responsible pid and keeps it across `setsid` and
//!    reparenting, so an escaped grandchild still reports the leader. Probed on macOS 26: this
//!    holds only while the leader is alive; once the leader has exited (even as an unreaped
//!    zombie) the escapee reports itself.
//!
//! Together these catch an escapee if any scan sees it while its leader is still alive or
//! while its parent chain still reaches a match. The residual gap: a process that escapes and
//! whose leader then exits within one poll interval is never seen. Denying `setsid`/`setpgid`
//! in the sandbox profile (research 1a §6 option 1) closes it. [`kill_identity`], used after an
//! engine restart, has no recorded set, so it can find escapees only by responsibility while the
//! leader lives, and by group and ancestry.
//!
//! # Process group reuse
//!
//! POSIX does not reuse a pid while a process group with that id is non-empty, so the group
//! number is ours exactly as long as the group never emptied. After an engine restart that is
//! unknown, and the number may name an unrelated group of the same user. The rule, applied by
//! every scan, by every `killpg` and by [`kill_identity`]:
//!
//! - If a process with pid == the recorded pgid exists and its start time is not the recorded
//!   leader start time, the group is **foreign**: nothing is matched by group, no `killpg` is
//!   sent, and [`kill_identity`] signals nothing at all. It returns [`TreeGone`] if no live
//!   process has the recorded leader pid and start time (the only identity it knows), and
//!   [`KillFailed`] naming that pid otherwise.
//! - If no process with pid == pgid exists, the members of that group are taken to be orphans of
//!   our group (whose leader exited while they kept the group alive) and may be killed. This
//!   is ambiguous in one case: our group emptied, an unrelated process reused the number as its
//!   own group, and that leader then exited while its members live on. Only the start-time
//!   filter (no earlier than our leader) narrows it.
//!
//! Only processes with our uid (effective or real) that started no earlier than the leader are
//! ever candidates, and never the supervisor itself.
//!
//! # Kill sequence
//!
//! `SIGTERM` to the group and to every match; up to [`KILL_GRACE`] for the tree to empty;
//! `SIGKILL` to the group; then a sweep: `SIGSTOP` every match and rescan until no new match
//! appears (a stopped process cannot fork, so this reaches a fixpoint), `SIGKILL` them all, and
//! repeat until two scans [`SECOND_PASS_DELAY`] apart find nothing. The sweep is bounded by
//! [`SWEEP_BOUND`]; past it the result is [`KillFailed`]. Finally the leader is reaped and the
//! output pipes must reach end of file within [`SWEEP_BOUND`] (an open pipe means some process
//! of the tree still holds it, which is also [`KillFailed`]). Only then is a [`TreeGone`] made.

use std::collections::{HashMap, HashSet};
use std::io::Write as _;
use std::os::fd::{AsRawFd as _, RawFd};
use std::os::unix::process::ExitStatusExt as _;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use harness_core::outcome::FailureCode;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::plan::{LaunchPlan, TreeGone};
use crate::shim::GATE_FD;

/// Time between `SIGTERM` and `SIGKILL`.
pub const KILL_GRACE: Duration = Duration::from_secs(5);
/// Bound on the post-`SIGKILL` sweep, on reaping the leader, and on draining the pipes.
pub const SWEEP_BOUND: Duration = Duration::from_secs(2);
/// Interval of the background descendant scan while a run is live.
pub const POLL_INTERVAL: Duration = Duration::from_millis(5);
/// Delay before the confirming second empty scan of the sweep.
pub const SECOND_PASS_DELAY: Duration = Duration::from_millis(200);
const GRACE_POLL: Duration = Duration::from_millis(20);
const CHANNEL_DEPTH: usize = 256;
const READ_CHUNK: usize = 64 * 1024;

/// Resource limits of one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Wall-clock limit, measured from the go byte.
    pub deadline: Duration,
    /// Total bytes read from stdout and stderr together.
    pub max_output_bytes: u64,
    /// Longest line accepted, excluding its newline.
    pub max_line_bytes: usize,
}

/// Who the leader of a run is, durable enough to find (and kill) it after an engine restart.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessIdentity {
    pub pgid: i32,
    pub pid: i32,
    /// Process start time in microseconds since the Unix epoch (`proc_bsdinfo`).
    pub start_us: i64,
    /// `kern.bootsessionuuid` when the process was started.
    pub boot_id: String,
}

/// How a run ended.
///
/// `failure` is the supervisor's verdict: `DeadlineExceeded` or `OutputLimitExceeded` when it
/// stopped the run, otherwise `ExitNonzero` when the leader did not exit 0. A cancelled run has
/// `cancelled = true` and no failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitInfo {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub failure: Option<FailureCode>,
    pub cancelled: bool,
}

/// Which output stream a line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stream {
    Stdout,
    Stderr,
}

/// Receives each output line, without its trailing newline. A final line without a newline is
/// delivered at end of file.
pub trait LineSink: Send {
    fn line(&mut self, stream: Stream, bytes: &[u8]);
}

/// Returned by an `on_identity` callback to stop the launch before anything runs.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("spawn aborted: {0}")]
pub struct SpawnAbort(pub String);

/// The kill sequence could not prove the process tree empty within its bound.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("process tree not empty after the kill bound; survivors: {survivors:?}")]
pub struct KillFailed {
    /// Pids still alive, if known (empty when enumeration failed or a pipe stayed open).
    pub survivors: Vec<i32>,
}

/// A gated child whose plan has not started yet. Dropping it kills the waiting shim.
#[derive(Debug)]
pub struct Supervised {
    child: Child,
    identity: ProcessIdentity,
    limits: Limits,
    gate: Option<std::io::PipeWriter>,
    stdin: Vec<u8>,
}

/// Starts `shim` gated on fd 3 (see the module docs). `on_identity` runs before the plan can
/// start; if it fails, the shim exits having run nothing and this returns `SpawnError`.
pub async fn spawn_gated(
    shim: &Path,
    plan: LaunchPlan,
    limits: Limits,
    on_identity: impl FnOnce(ProcessIdentity) -> Result<(), SpawnAbort>,
) -> Result<Supervised, FailureCode> {
    if plan.argv.is_empty() {
        return Err(FailureCode::SpawnError);
    }
    let (gate_r, gate_w) = std::io::pipe().map_err(|_| FailureCode::SpawnError)?;
    let gate_fd = gate_r.as_raw_fd();
    let mut cmd = Command::new(shim);
    cmd.args(&plan.argv)
        .env_clear()
        .envs(plan.env.iter().map(|(k, v)| (k, v)))
        .current_dir(&plan.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // SAFETY: the closure runs between fork and exec and only calls dup2/fcntl, which are
    // async-signal-safe.
    unsafe {
        cmd.pre_exec(move || gate_onto_fd3(gate_fd));
    }
    let mut child = cmd.spawn().map_err(|_| FailureCode::SpawnError)?;
    drop(gate_r);

    let info = child.id().and_then(|pid| proc_info(pid as i32));
    let Some(info) = info else {
        drop(gate_w);
        let _ = child.kill().await;
        return Err(FailureCode::SpawnError);
    };
    let identity = ProcessIdentity {
        pgid: info.pid,
        pid: info.pid,
        start_us: info.start_us,
        boot_id: boot_id(),
    };
    if on_identity(identity.clone()).is_err() {
        drop(gate_w);
        // The shim reads end of file and exits having run nothing.
        let _ = child.wait().await;
        return Err(FailureCode::SpawnError);
    }
    Ok(Supervised {
        child,
        identity,
        limits,
        gate: Some(gate_w),
        stdin: plan.stdin,
    })
}

fn gate_onto_fd3(fd: RawFd) -> std::io::Result<()> {
    // SAFETY: plain fd syscalls on fds this process owns.
    let rc = unsafe {
        if fd == GATE_FD {
            libc::fcntl(fd, libc::F_SETFD, 0)
        } else {
            libc::dup2(fd, GATE_FD)
        }
    };
    if rc == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

enum Event {
    Line(Stream, Vec<u8>),
    Overflow,
}

impl Supervised {
    pub fn identity(&self) -> &ProcessIdentity {
        &self.identity
    }

    /// Lets the plan run, streams its output to `sink`, and stops it on exit, deadline, output
    /// limit or cancellation. Always runs the kill sequence before returning; the `TreeGone`
    /// is present only if that sequence proved the tree empty.
    pub async fn run(
        mut self,
        sink: &mut dyn LineSink,
        cancel: CancellationToken,
    ) -> (ExitInfo, Result<TreeGone, KillFailed>) {
        let identity = self.identity.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let poller = {
            let stop = Arc::clone(&stop);
            let mut tracker = Tracker::new(&identity);
            std::thread::Builder::new()
                .name("eng-descendants".into())
                .spawn(move || {
                    while !stop.load(Ordering::Acquire) {
                        let _ = tracker.scan();
                        std::thread::sleep(POLL_INTERVAL);
                    }
                    tracker
                })
                .ok()
        };

        let started = Instant::now();
        if let Some(mut gate) = self.gate.take() {
            let _ = gate.write_all(&[1]);
        }
        if let Some(mut stdin) = self.child.stdin.take() {
            let data = std::mem::take(&mut self.stdin);
            tokio::spawn(async move {
                let _ = stdin.write_all(&data).await;
                let _ = stdin.shutdown().await;
            });
        }
        let (tx, mut rx) = mpsc::channel(CHANNEL_DEPTH);
        let total = Arc::new(AtomicU64::new(0));
        if let Some(out) = self.child.stdout.take() {
            tokio::spawn(pump(
                out,
                Stream::Stdout,
                self.limits,
                Arc::clone(&total),
                tx.clone(),
            ));
        }
        if let Some(err) = self.child.stderr.take() {
            tokio::spawn(pump(
                err,
                Stream::Stderr,
                self.limits,
                Arc::clone(&total),
                tx.clone(),
            ));
        }
        drop(tx);

        let mut failure = None;
        let mut cancelled = false;
        let mut streams_open = true;
        let deadline = tokio::time::sleep_until(started + self.limits.deadline);
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                biased;
                () = cancel.cancelled() => { cancelled = true; break; }
                () = &mut deadline => { failure = Some(FailureCode::DeadlineExceeded); break; }
                ev = rx.recv(), if streams_open => match ev {
                    Some(Event::Line(stream, bytes)) => sink.line(stream, &bytes),
                    Some(Event::Overflow) => {
                        failure = Some(FailureCode::OutputLimitExceeded);
                        break;
                    }
                    None => streams_open = false,
                },
                _ = self.child.wait() => break,
            }
        }

        stop.store(true, Ordering::Release);
        let tracker = match poller {
            Some(handle) => tokio::task::spawn_blocking(move || handle.join())
                .await
                .ok(),
            None => None,
        };
        let mut tracker = match tracker {
            Some(Ok(tracker)) => tracker,
            _ => Tracker::new(&identity),
        };
        let mut killed = terminate(&mut tracker, Some(&mut self.child)).await;

        // Everything that could write to the pipes is dead, so they reach end of file.
        let drain_end = Instant::now() + SWEEP_BOUND;
        while streams_open {
            match tokio::time::timeout_at(drain_end, rx.recv()).await {
                Ok(Some(Event::Line(stream, bytes))) => sink.line(stream, &bytes),
                Ok(Some(Event::Overflow)) => {
                    failure.get_or_insert(FailureCode::OutputLimitExceeded);
                }
                Ok(None) => streams_open = false,
                Err(_) => {
                    if killed.is_ok() {
                        killed = Err(KillFailed {
                            survivors: Vec::new(),
                        });
                    }
                    break;
                }
            }
        }

        let status = self.child.try_wait().ok().flatten();
        let code = status.and_then(|s| s.code());
        let signal = status.and_then(|s| s.signal());
        if failure.is_none() && !cancelled && code != Some(0) {
            failure = Some(FailureCode::ExitNonzero);
        }
        let exit = ExitInfo {
            code,
            signal,
            failure,
            cancelled,
        };
        (exit, killed.map(|()| TreeGone::confirmed()))
    }
}

/// Reads `r` into length-limited lines. Sends `Overflow` and stops on a limit breach.
async fn pump<R: AsyncRead + Unpin>(
    mut r: R,
    stream: Stream,
    limits: Limits,
    total: Arc<AtomicU64>,
    tx: mpsc::Sender<Event>,
) {
    let mut pending: Vec<u8> = Vec::new();
    let mut chunk = vec![0u8; READ_CHUNK];
    loop {
        let n = match r.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let before = total.fetch_add(n as u64, Ordering::Relaxed);
        if before.saturating_add(n as u64) > limits.max_output_bytes {
            let _ = tx.send(Event::Overflow).await;
            return;
        }
        let scanned = pending.len();
        pending.extend_from_slice(&chunk[..n]);
        let mut start = 0;
        let mut from = scanned;
        while let Some(off) = pending[from..].iter().position(|&b| b == b'\n') {
            let end = from + off;
            if end - start > limits.max_line_bytes {
                let _ = tx.send(Event::Overflow).await;
                return;
            }
            if tx
                .send(Event::Line(stream, pending[start..end].to_vec()))
                .await
                .is_err()
            {
                return;
            }
            start = end + 1;
            from = start;
        }
        pending.drain(..start);
        if pending.len() > limits.max_line_bytes {
            let _ = tx.send(Event::Overflow).await;
            return;
        }
    }
    if !pending.is_empty() {
        let _ = tx.send(Event::Line(stream, pending)).await;
    }
}

/// `SIGTERM`, grace, `SIGKILL`, sweep, reap. See the module docs.
async fn terminate(tracker: &mut Tracker, mut child: Option<&mut Child>) -> Result<(), KillFailed> {
    tracker.signal_group(libc::SIGTERM);
    let mut termed: HashSet<i32> = HashSet::new();
    let grace_end = Instant::now() + KILL_GRACE;
    loop {
        if let Some(c) = child.as_mut() {
            let _ = c.try_wait();
        }
        match tracker.scan() {
            Some(live) if live.is_empty() => break,
            Some(live) => {
                for pid in live {
                    if termed.insert(pid) {
                        send(pid, libc::SIGTERM);
                    }
                }
            }
            None => {}
        }
        if Instant::now() >= grace_end {
            break;
        }
        tokio::time::sleep(GRACE_POLL).await;
    }
    tracker.signal_group(libc::SIGKILL);
    sweep(tracker).await?;
    if let Some(c) = child {
        match tokio::time::timeout(SWEEP_BOUND, c.wait()).await {
            Ok(Ok(_)) => {}
            _ => {
                return Err(KillFailed {
                    survivors: vec![tracker.leader_pid],
                });
            }
        }
    }
    Ok(())
}

/// Freeze-then-kill until two empty scans, bounded by [`SWEEP_BOUND`].
async fn sweep(tracker: &mut Tracker) -> Result<(), KillFailed> {
    let end = Instant::now() + SWEEP_BOUND;
    let mut empty_scans = 0;
    loop {
        if Instant::now() >= end {
            return match tracker.scan() {
                Some(live) if live.is_empty() => Ok(()),
                Some(live) => Err(KillFailed { survivors: live }),
                None => Err(KillFailed {
                    survivors: Vec::new(),
                }),
            };
        }
        let mut frozen: HashSet<i32> = HashSet::new();
        loop {
            let Some(live) = tracker.scan() else {
                return Err(KillFailed {
                    survivors: frozen.into_iter().collect(),
                });
            };
            let mut grew = false;
            for pid in live {
                if frozen.insert(pid) {
                    send(pid, libc::SIGSTOP);
                    grew = true;
                }
            }
            if !grew || Instant::now() >= end {
                break;
            }
        }
        if frozen.is_empty() {
            empty_scans += 1;
            if empty_scans >= 2 {
                return Ok(());
            }
            tokio::time::sleep(SECOND_PASS_DELAY).await;
            continue;
        }
        empty_scans = 0;
        for pid in &frozen {
            send(*pid, libc::SIGKILL);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn send(pid: i32, sig: i32) {
    // SAFETY: kill has no memory-safety preconditions; pid > 1 is enforced by the tracker.
    unsafe { libc::kill(pid, sig) };
}

/// The set of processes that belong to one run. See the module docs for the matching rules.
struct Tracker {
    leader_pid: i32,
    leader_start: i64,
    pgid: i32,
    uid: u32,
    members: HashMap<i32, i64>,
}

impl Tracker {
    fn new(id: &ProcessIdentity) -> Self {
        let mut members = HashMap::new();
        members.insert(id.pid, id.start_us);
        // SAFETY: getuid has no preconditions.
        let uid = unsafe { libc::getuid() };
        Tracker {
            leader_pid: id.pid,
            leader_start: id.start_us,
            pgid: id.pgid,
            uid,
            members,
        }
    }

    /// True if a process with pid == the recorded pgid exists with another start time: the
    /// group number now belongs to someone else (see "Process group reuse").
    fn group_foreign(&self) -> bool {
        proc_info(self.pgid).is_some_and(|p| p.start_us != self.leader_start)
    }

    /// `killpg` the run's group, unless the group is foreign.
    fn signal_group(&self, sig: i32) {
        if !self.group_foreign() && self.pgid > 1 {
            // SAFETY: killpg has no memory-safety preconditions.
            unsafe { libc::killpg(self.pgid, sig) };
        }
    }

    /// Returns the live (non-zombie) processes of the run and records them. `None` if the
    /// process table could not be read.
    fn scan(&mut self) -> Option<Vec<i32>> {
        let me = std::process::id() as i32;
        let mut procs: HashMap<i32, ProcInfo> = HashMap::new();
        for pid in all_pids()? {
            if pid <= 1 || pid == me {
                continue;
            }
            if let Some(p) = proc_info(pid) {
                let ours = p.uid == self.uid || p.ruid == self.uid;
                if ours && !p.zombie && p.start_us >= self.leader_start {
                    procs.insert(pid, p);
                }
            }
        }
        let leader = proc_info(self.leader_pid);
        let leader_reused = leader.is_some_and(|p| p.start_us != self.leader_start);
        let group_ours = !leader_reused && !self.group_foreign();
        let leader_alive = leader.is_some_and(|p| p.start_us == self.leader_start && !p.zombie);
        let spi = if leader_alive {
            responsibility_spi()
        } else {
            None
        };

        let mut matched: HashSet<i32> = HashSet::new();
        for p in procs.values() {
            let hit = self.members.get(&p.pid) == Some(&p.start_us)
                || (group_ours && p.pgid == self.pgid)
                // SAFETY: the SPI takes a pid by value and returns a pid.
                || spi.is_some_and(|s| unsafe { (s.responsible_pid)(p.pid) } == self.leader_pid);
            if hit {
                matched.insert(p.pid);
            }
        }
        loop {
            let mut grew = false;
            for p in procs.values() {
                if !matched.contains(&p.pid)
                    && matched.contains(&p.ppid)
                    && procs
                        .get(&p.ppid)
                        .is_some_and(|parent| p.start_us >= parent.start_us)
                {
                    matched.insert(p.pid);
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        for pid in &matched {
            if let Some(p) = procs.get(pid) {
                self.members.insert(*pid, p.start_us);
            }
        }
        Some(matched.into_iter().collect())
    }
}

#[derive(Debug, Clone, Copy)]
struct ProcInfo {
    pid: i32,
    ppid: i32,
    pgid: i32,
    uid: u32,
    ruid: u32,
    zombie: bool,
    start_us: i64,
}

/// `proc_pidinfo(PROC_PIDTBSDINFO)`; `None` if the process does not exist (or is not visible).
fn proc_info(pid: i32) -> Option<ProcInfo> {
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    // SAFETY: proc_bsdinfo is plain old data; proc_pidinfo writes at most `size` bytes into it.
    let (n, info) = unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let n = libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size);
        (n, info)
    };
    if n != size {
        return None;
    }
    Some(ProcInfo {
        pid: info.pbi_pid as i32,
        ppid: info.pbi_ppid as i32,
        pgid: info.pbi_pgid as i32,
        uid: info.pbi_uid,
        ruid: info.pbi_ruid,
        zombie: info.pbi_status == libc::SZOMB,
        start_us: (info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec) as i64,
    })
}

/// Every pid on the system (`proc_listallpids`); `None` if the table could not be read.
fn all_pids() -> Option<Vec<i32>> {
    // SAFETY: a null buffer asks for the current count only.
    let estimate = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if estimate <= 0 {
        return None;
    }
    let mut buf = vec![0i32; estimate as usize + 512];
    let bytes = (buf.len() * std::mem::size_of::<i32>()) as libc::c_int;
    // SAFETY: the buffer holds `bytes` bytes.
    let n = unsafe { libc::proc_listallpids(buf.as_mut_ptr().cast(), bytes) };
    if n <= 0 {
        return None;
    }
    buf.truncate((n as usize).min(buf.len()));
    Some(buf)
}

/// Function pointers to Apple's process-responsibility SPI.
pub(crate) struct ResponsibilitySpi {
    /// `pid_t responsibility_get_pid_responsible_for_pid(pid_t)`.
    pub(crate) responsible_pid: unsafe extern "C" fn(libc::pid_t) -> libc::pid_t,
    /// `int responsibility_spawnattrs_setdisclaim(posix_spawnattr_t *, int)`.
    pub(crate) set_disclaim:
        unsafe extern "C" fn(*mut libc::posix_spawnattr_t, libc::c_int) -> libc::c_int,
}

/// PRIVATE APPLE SPI: the only non-public API in this crate.
///
/// Both functions are exported by `/usr/lib/system/libquarantine.dylib` (reached through
/// libSystem) but appear in no public header; Chromium uses them the same way. They are looked
/// up with `dlsym` at run time so that a macOS release without them makes
/// [`descendant_tracking_available`] false (and `eng doctor` fail closed) instead of failing to
/// load. Signatures as probed on macOS 26 (research 1a §6 and this task's probe).
pub(crate) fn responsibility_spi() -> Option<&'static ResponsibilitySpi> {
    static SPI: OnceLock<Option<ResponsibilitySpi>> = OnceLock::new();
    SPI.get_or_init(|| {
        // SAFETY: dlsym with RTLD_DEFAULT and NUL-terminated names; each non-null symbol is
        // transmuted to the signature it has on macOS 26.
        unsafe {
            let get = libc::dlsym(
                libc::RTLD_DEFAULT,
                c"responsibility_get_pid_responsible_for_pid".as_ptr(),
            );
            let disclaim = libc::dlsym(
                libc::RTLD_DEFAULT,
                c"responsibility_spawnattrs_setdisclaim".as_ptr(),
            );
            if get.is_null() || disclaim.is_null() {
                return None;
            }
            Some(ResponsibilitySpi {
                responsible_pid: std::mem::transmute::<
                    *mut libc::c_void,
                    unsafe extern "C" fn(libc::pid_t) -> libc::pid_t,
                >(get),
                set_disclaim: std::mem::transmute::<
                    *mut libc::c_void,
                    unsafe extern "C" fn(*mut libc::posix_spawnattr_t, libc::c_int) -> libc::c_int,
                >(disclaim),
            })
        }
    })
    .as_ref()
}

/// Whether every mechanism the kill sequence relies on is present: the process table can be
/// enumerated and read, and the responsibility SPI resolves. `eng doctor` fails closed on false.
pub fn descendant_tracking_available() -> bool {
    let me = std::process::id() as i32;
    all_pids().is_some_and(|pids| pids.contains(&me))
        && proc_info(me).is_some()
        && responsibility_spi().is_some()
}

/// `kern.bootsessionuuid`, or an empty string if it cannot be read.
pub fn boot_id() -> String {
    let mut buf = [0u8; 64];
    let mut len = buf.len();
    // SAFETY: sysctlbyname writes at most `len` bytes into `buf` and updates `len`.
    let rc = unsafe {
        libc::sysctlbyname(
            c"kern.bootsessionuuid".as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return String::new();
    }
    let bytes = &buf[..len.min(buf.len())];
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// True if the identity names a boot other than the current one (so everything it ran is gone).
/// An unreadable boot id never counts as a reboot.
fn rebooted(id: &ProcessIdentity) -> bool {
    let now = boot_id();
    !id.boot_id.is_empty() && !now.is_empty() && now != id.boot_id
}

/// Whether the leader, or any process still in its group, is alive. Fails closed: true if the
/// process table cannot be read.
pub fn identity_is_alive(id: &ProcessIdentity) -> bool {
    if rebooted(id) {
        return false;
    }
    Tracker::new(id).scan().is_none_or(|live| !live.is_empty())
}

/// Runs the kill sequence against a run known only by its identity (after an engine restart).
/// See the module docs for what it can and cannot find, and "Process group reuse" for the
/// foreign-group rule: a group whose leader has another start time is never signalled.
pub async fn kill_identity(id: &ProcessIdentity) -> Result<TreeGone, KillFailed> {
    if rebooted(id) {
        return Ok(TreeGone::confirmed());
    }
    let mut tracker = Tracker::new(id);
    if tracker.group_foreign() {
        let leader_alive =
            proc_info(id.pid).is_some_and(|p| p.start_us == id.start_us && !p.zombie);
        return if leader_alive {
            Err(KillFailed {
                survivors: vec![id.pid],
            })
        } else {
            Ok(TreeGone::confirmed())
        };
    }
    terminate(&mut tracker, None).await?;
    Ok(TreeGone::confirmed())
}
