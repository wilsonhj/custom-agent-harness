//! `eng-shim`: the gate between the supervisor and the plan it runs, and the anchor the plan's
//! descendants stay attributed to until the supervisor has killed them all.
//!
//! The supervisor starts `eng-shim <argv...>` with the plan's complete environment (cleared
//! first) and working directory already applied, the read end of a gate pipe on [`GATE_FD`] and
//! the write end of a status pipe on [`STATUS_FD`]. The shim:
//!
//! 1. replaces its own image with itself (`POSIX_SPAWN_SETEXEC`, same pid and start time) with
//!    the responsibility "disclaim" attribute, so the shim becomes its own responsible process
//!    and everything it starts inherits it as theirs (see the `supervisor` module docs). The
//!    re-executed image is recognised by [`ANCHOR_ARGV0`]; without the SPI this step is skipped;
//! 2. calls `setsid`, so it leads a new session and process group (pgid == pid);
//! 3. blocks reading one byte from the gate. End of file (the supervisor dropped the gate because
//!    its identity callback failed, or the supervisor died) means exit [`RAN_NOTHING`] without
//!    running anything;
//! 4. on the byte, starts `argv` as its child (`posix_spawnp`, PATH taken from the plan's
//!    environment, every signal reset to its default disposition, an empty signal mask; the
//!    gate and status fds are not inherited), then points its own stdin, stdout and stderr at
//!    `/dev/null` so it holds none of the supervisor's pipes. If the spawn fails, the shim
//!    reports it on stderr and exits [`EXEC_FAILED`];
//! 5. waits for the plan, writes its raw wait status (an `i32` in native byte order) to the
//!    status pipe and closes it;
//! 6. stays alive, never signalled by the supervisor, until the gate reaches end of file (the
//!    supervisor releases it after its sweep, or died) **and** no live process names the shim as
//!    its responsible process; then exits 0. While the shim lives, every descendant that did
//!    not disclaim responsibility itself can be found, whatever its session, group or parent.

use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::time::Duration;

use crate::supervisor::{responsibility_spi, responsible_descendant_alive};

/// The fd on which the shim waits for the go byte, and later for its release (end of file).
pub const GATE_FD: i32 = 3;
/// The fd on which the shim reports the plan's wait status.
pub const STATUS_FD: i32 = 4;
/// `argv[0]` of the re-executed, self-responsible shim image.
pub const ANCHOR_ARGV0: &str = "eng-shim(anchor)";
/// Exit code meaning the shim ran nothing (no go byte, or a malformed invocation).
pub const RAN_NOTHING: i32 = 125;
/// Exit code meaning the go byte arrived but `argv` could not be started.
pub const EXEC_FAILED: i32 = 127;

const LINGER_POLL: Duration = Duration::from_millis(5);

/// Entry point of the `eng-shim` binary. Never returns.
pub fn run() -> ! {
    let mut args = std::env::args_os();
    let argv0 = args.next();
    let argv: Result<Vec<CString>, _> = args.map(|a| CString::new(a.as_bytes())).collect();
    let argv = match argv {
        Ok(argv) if !argv.is_empty() => argv,
        _ => std::process::exit(RAN_NOTHING),
    };
    if argv0.as_deref() != Some(std::ffi::OsStr::new(ANCHOR_ARGV0)) {
        // Returns only if the re-exec failed; the shim then runs without the anchor and the
        // supervisor's descendant tracking is degraded (`eng doctor` reports it).
        reexec_as_anchor(&argv);
    }
    // SAFETY: setsid has no memory-safety preconditions.
    if unsafe { libc::setsid() } == -1 {
        std::process::exit(RAN_NOTHING);
    }
    // SAFETY: fcntl on our own fds; the plan must not inherit either.
    unsafe {
        libc::fcntl(GATE_FD, libc::F_SETFD, libc::FD_CLOEXEC);
        libc::fcntl(STATUS_FD, libc::F_SETFD, libc::FD_CLOEXEC);
    }
    if !wait_for_go() {
        std::process::exit(RAN_NOTHING);
    }
    let plan = match spawn(&argv) {
        Ok(pid) => pid,
        Err(err) => {
            let name = argv[0].to_string_lossy().into_owned();
            eprintln!("eng-shim: cannot execute {name}: {err}");
            std::process::exit(EXEC_FAILED);
        }
    };
    detach_stdio();
    let status = wait_for(plan);
    let bytes = status.to_ne_bytes();
    // SAFETY: writes from a valid 4-byte buffer to our own fd, then closes it.
    unsafe {
        libc::write(STATUS_FD, bytes.as_ptr().cast(), bytes.len());
        libc::close(STATUS_FD);
    }
    wait_for_release();
    let me = std::process::id() as i32;
    while responsible_descendant_alive(me) {
        std::thread::sleep(LINGER_POLL);
    }
    std::process::exit(0);
}

/// Re-executes this binary with the disclaim attribute and `argv[0] = ANCHOR_ARGV0`. Returns
/// only if that is not possible.
fn reexec_as_anchor(argv: &[CString]) {
    let Some(spi) = responsibility_spi() else {
        return;
    };
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Ok(exe) = CString::new(exe.as_os_str().as_bytes()) else {
        return;
    };
    let Ok(anchor) = CString::new(ANCHOR_ARGV0) else {
        return;
    };
    let mut ptrs: Vec<*mut libc::c_char> = std::iter::once(&anchor)
        .chain(argv)
        .map(|a| a.as_ptr().cast_mut())
        .collect();
    ptrs.push(std::ptr::null_mut());
    // SAFETY: `ptrs` is a NUL-terminated array of NUL-terminated strings that outlive the call,
    // the attribute is initialised before use, and `_NSGetEnviron` is the process environment.
    unsafe {
        let mut attr: libc::posix_spawnattr_t = std::ptr::null_mut();
        if libc::posix_spawnattr_init(&mut attr) != 0 {
            return;
        }
        libc::posix_spawnattr_setflags(&mut attr, libc::POSIX_SPAWN_SETEXEC as libc::c_short);
        (spi.set_disclaim)(&mut attr, 1);
        libc::posix_spawn(
            std::ptr::null_mut(),
            exe.as_ptr(),
            std::ptr::null(),
            &attr,
            ptrs.as_ptr(),
            (*libc::_NSGetEnviron()).cast_const(),
        );
        libc::posix_spawnattr_destroy(&mut attr);
    }
}

/// True once one byte has been read from the gate; false on end of file or error.
fn wait_for_go() -> bool {
    let mut byte = 0u8;
    loop {
        // SAFETY: reads at most one byte into a valid one-byte buffer.
        let n = unsafe { libc::read(GATE_FD, (&raw mut byte).cast(), 1) };
        if n == 1 {
            return true;
        }
        if n == -1 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
            continue;
        }
        return false;
    }
}

/// Blocks until the gate reaches end of file (or fails).
fn wait_for_release() {
    let mut buf = [0u8; 16];
    loop {
        // SAFETY: reads into a valid buffer of the given length.
        let n = unsafe { libc::read(GATE_FD, buf.as_mut_ptr().cast(), buf.len()) };
        if n > 0 || (n == -1 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted) {
            continue;
        }
        return;
    }
}

/// Starts `argv` as a child with default signal dispositions and an empty mask.
fn spawn(argv: &[CString]) -> io::Result<libc::pid_t> {
    let mut ptrs: Vec<*mut libc::c_char> = argv.iter().map(|a| a.as_ptr().cast_mut()).collect();
    ptrs.push(std::ptr::null_mut());
    // SAFETY: every pointer passed below is valid for the duration of the call: `ptrs` is a
    // NUL-terminated array of NUL-terminated strings owned by `argv`, the attribute and signal
    // sets are initialised before use, and `_NSGetEnviron` returns the process environment.
    unsafe {
        let mut attr: libc::posix_spawnattr_t = std::ptr::null_mut();
        let rc = libc::posix_spawnattr_init(&mut attr);
        if rc != 0 {
            return Err(io::Error::from_raw_os_error(rc));
        }
        let mut defaults: libc::sigset_t = std::mem::zeroed();
        libc::sigfillset(&mut defaults);
        libc::sigdelset(&mut defaults, libc::SIGKILL);
        libc::sigdelset(&mut defaults, libc::SIGSTOP);
        libc::posix_spawnattr_setsigdefault(&mut attr, &defaults);
        let mut mask: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut mask);
        libc::posix_spawnattr_setsigmask(&mut attr, &mask);
        let flags = libc::POSIX_SPAWN_SETSIGDEF | libc::POSIX_SPAWN_SETSIGMASK;
        libc::posix_spawnattr_setflags(&mut attr, flags as libc::c_short);
        let env = *libc::_NSGetEnviron();
        let mut pid: libc::pid_t = 0;
        let rc = libc::posix_spawnp(
            &mut pid,
            ptrs[0],
            std::ptr::null(),
            &attr,
            ptrs.as_ptr(),
            env.cast_const(),
        );
        libc::posix_spawnattr_destroy(&mut attr);
        if rc == 0 {
            Ok(pid)
        } else {
            Err(io::Error::from_raw_os_error(rc))
        }
    }
}

/// Points fds 0, 1 and 2 at `/dev/null`.
fn detach_stdio() {
    // SAFETY: open/dup2/close on a NUL-terminated path and our own fds.
    unsafe {
        let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR);
        if null >= 0 {
            for fd in 0..=2 {
                libc::dup2(null, fd);
            }
            if null > 2 {
                libc::close(null);
            }
        }
    }
}

/// The plan's raw wait status.
fn wait_for(pid: libc::pid_t) -> i32 {
    let mut status = 0;
    loop {
        // SAFETY: waitpid writes the status into a valid i32.
        let rc = unsafe { libc::waitpid(pid, &mut status, 0) };
        if rc == pid {
            return status;
        }
        if rc == -1 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            return status;
        }
    }
}
