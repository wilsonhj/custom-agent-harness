//! `eng-shim`: the gate between the supervisor and the plan it runs.
//!
//! The supervisor starts `eng-shim <argv...>` with the plan's complete environment (cleared
//! first) and working directory already applied, and with the read end of a gate pipe on fd 3.
//! The shim:
//!
//! 1. calls `setsid`, so it leads a new session and process group (pgid == pid);
//! 2. blocks reading one byte from fd 3. End of file (the supervisor dropped the gate because
//!    its identity callback failed, or the supervisor died) means exit [`RAN_NOTHING`] without
//!    running anything;
//! 3. on the byte, replaces itself with `argv` (`posix_spawnp` with `POSIX_SPAWN_SETEXEC`, PATH
//!    taken from the plan's environment), resetting every signal to its default disposition
//!    and clearing the signal mask. When available it sets the responsibility "disclaim"
//!    attribute so the plan becomes its own responsible process (see the `supervisor` module
//!    docs for why).
//!
//! If the exec itself fails, the shim reports it on stderr and exits [`EXEC_FAILED`].

use std::ffi::CString;
use std::io;
use std::os::unix::ffi::OsStrExt;

use crate::supervisor::responsibility_spi;

/// The fd on which the shim waits for the go byte.
pub const GATE_FD: i32 = 3;
/// Exit code meaning the shim ran nothing (no go byte, or a malformed invocation).
pub const RAN_NOTHING: i32 = 125;
/// Exit code meaning the go byte arrived but `argv` could not be executed.
pub const EXEC_FAILED: i32 = 127;

/// Entry point of the `eng-shim` binary. Never returns.
pub fn run() -> ! {
    let argv: Result<Vec<CString>, _> = std::env::args_os()
        .skip(1)
        .map(|a| CString::new(a.as_bytes()))
        .collect();
    let argv = match argv {
        Ok(argv) if !argv.is_empty() => argv,
        _ => std::process::exit(RAN_NOTHING),
    };
    // SAFETY: setsid has no memory-safety preconditions.
    if unsafe { libc::setsid() } == -1 {
        std::process::exit(RAN_NOTHING);
    }
    if !wait_for_go() {
        std::process::exit(RAN_NOTHING);
    }
    let err = exec(&argv);
    let name = argv
        .first()
        .map(|a| a.to_string_lossy().into_owned())
        .unwrap_or_default();
    eprintln!("eng-shim: cannot execute {name}: {err}");
    std::process::exit(EXEC_FAILED);
}

/// True once one byte has been read from the gate; false on end of file or error.
fn wait_for_go() -> bool {
    let mut byte = 0u8;
    loop {
        // SAFETY: reads at most one byte into a valid one-byte buffer.
        let n = unsafe { libc::read(GATE_FD, (&raw mut byte).cast(), 1) };
        if n == 1 {
            // SAFETY: closing our own gate fd; nothing else uses it.
            unsafe { libc::close(GATE_FD) };
            return true;
        }
        if n == -1 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
            continue;
        }
        return false;
    }
}

/// Replaces this process image with `argv`. Returns only on failure.
fn exec(argv: &[CString]) -> io::Error {
    let mut ptrs: Vec<*mut libc::c_char> = argv.iter().map(|a| a.as_ptr().cast_mut()).collect();
    ptrs.push(std::ptr::null_mut());
    // SAFETY: every pointer passed below is valid for the duration of the call: `ptrs` is a
    // NUL-terminated array of NUL-terminated strings owned by `argv`, the attribute and signal
    // sets are initialised before use, and `_NSGetEnviron` returns the process environment.
    unsafe {
        let mut attr: libc::posix_spawnattr_t = std::ptr::null_mut();
        let rc = libc::posix_spawnattr_init(&mut attr);
        if rc != 0 {
            return io::Error::from_raw_os_error(rc);
        }
        let mut defaults: libc::sigset_t = std::mem::zeroed();
        libc::sigfillset(&mut defaults);
        libc::sigdelset(&mut defaults, libc::SIGKILL);
        libc::sigdelset(&mut defaults, libc::SIGSTOP);
        libc::posix_spawnattr_setsigdefault(&mut attr, &defaults);
        let mut mask: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut mask);
        libc::posix_spawnattr_setsigmask(&mut attr, &mask);
        let flags =
            libc::POSIX_SPAWN_SETEXEC | libc::POSIX_SPAWN_SETSIGDEF | libc::POSIX_SPAWN_SETSIGMASK;
        libc::posix_spawnattr_setflags(&mut attr, flags as libc::c_short);
        if let Some(spi) = responsibility_spi() {
            (spi.set_disclaim)(&mut attr, 1);
        }
        let env = *libc::_NSGetEnviron();
        let rc = libc::posix_spawnp(
            std::ptr::null_mut(),
            ptrs[0],
            std::ptr::null(),
            &attr,
            ptrs.as_ptr(),
            env.cast_const(),
        );
        libc::posix_spawnattr_destroy(&mut attr);
        io::Error::from_raw_os_error(rc)
    }
}
