//! Types shared by the supervisor, the sandbox and the workspace modules.

use std::path::PathBuf;

/// A fully resolved process launch: what to run, with which environment, where, and what to
/// write to its stdin. The sandbox wraps one `LaunchPlan` into another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    pub stdin: Vec<u8>,
}

/// Proof that every process of a supervised run is gone. Only the supervisor creates one
/// (after its kill sequence confirms the process tree is empty); a worktree may be reset or
/// unlocked only by presenting it.
#[derive(Debug)]
#[must_use]
pub struct TreeGone(());

impl TreeGone {
    pub(crate) fn confirmed() -> Self {
        TreeGone(())
    }

    /// For this crate's integration tests only.
    #[cfg(feature = "test-support")]
    pub fn for_tests() -> Self {
        TreeGone(())
    }
}
