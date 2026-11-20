//! Operator authorization for hosted task content (`docs/security/data-egress.md`).
//! Repository configuration cannot widen this grant. BAA configurations are unsupported.

use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use harness_core::ids::ArtifactId;
use harness_core::types::RuntimeKind;
use serde::Deserialize;

pub(crate) const POLICY_FILE: &str = "egress-policy.json";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    version: u32,
    project_root: PathBuf,
    egress_class: String,
    allowed_runtimes: Vec<RuntimeKind>,
    reviewed_by: String,
}

pub(crate) struct Authorization {
    pub bytes: Vec<u8>,
    repo: PathBuf,
    runtime: RuntimeKind,
}

impl Authorization {
    pub fn digest(&self) -> ArtifactId {
        ArtifactId::of(&self.bytes)
    }

    /// A changed, removed or newly invalid policy revokes this launch before the go byte.
    pub fn recheck(&self, state: &Path) -> Result<(), ()> {
        let current = authorize(state, &self.repo, self.runtime)?;
        (current.bytes == self.bytes).then_some(()).ok_or(())
    }
}

pub(crate) fn authorize(
    state: &Path,
    repo: &Path,
    runtime: RuntimeKind,
) -> Result<Authorization, ()> {
    let requested_repo = repo.to_path_buf();
    let state = fs::canonicalize(state).map_err(|_| ())?;
    let repo = fs::canonicalize(repo).map_err(|_| ())?;
    // A grant in source, or a source checkout inside state, is not operator-only policy.
    if state.starts_with(&repo) || repo.starts_with(&state) {
        return Err(());
    }
    let state_meta = fs::metadata(&state).map_err(|_| ())?;
    let path = state.join(POLICY_FILE);
    let meta = fs::symlink_metadata(&path).map_err(|_| ())?;
    // Hard links could let source content mutate the same inode through another path.
    if !meta.is_file()
        || meta.nlink() != 1
        || meta.uid() != state_meta.uid()
        || meta.mode() & 0o077 != 0
        || state_meta.mode() & 0o077 != 0
    {
        return Err(());
    }
    let bytes = fs::read(path).map_err(|_| ())?;
    let policy: Policy = serde_json::from_slice(&bytes).map_err(|_| ())?;
    if policy.version != 1
        || policy.project_root != repo
        || policy.egress_class != "any-provider"
        || !policy.allowed_runtimes.contains(&runtime)
        || policy.reviewed_by.trim().is_empty()
    {
        return Err(());
    }
    Ok(Authorization {
        bytes,
        repo: requested_repo,
        runtime,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::os::unix::fs::{PermissionsExt as _, symlink};

    use super::*;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("eng-egress-policy-{}", crate::new_id()));
            fs::create_dir(&root).unwrap();
            let root = fs::canonicalize(root).unwrap();
            fs::create_dir(root.join("repo")).unwrap();
            fs::create_dir(root.join("state")).unwrap();
            fs::set_permissions(root.join("state"), fs::Permissions::from_mode(0o700)).unwrap();
            let f = Self(root);
            f.write();
            f
        }

        fn write(&self) {
            let policy = serde_json::json!({
                "version": 1,
                "project_root": self.0.join("repo"),
                "egress_class": "any-provider",
                "allowed_runtimes": ["claude"],
                "reviewed_by": "test operator",
            });
            fs::write(self.path(), serde_json::to_vec(&policy).unwrap()).unwrap();
            fs::set_permissions(self.path(), fs::Permissions::from_mode(0o600)).unwrap();
        }

        fn path(&self) -> PathBuf {
            self.0.join("state").join(POLICY_FILE)
        }

        fn authorize(&self) -> Result<Authorization, ()> {
            authorize(
                &self.0.join("state"),
                &self.0.join("repo"),
                RuntimeKind::Claude,
            )
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_changed_deleted_or_malformed_policy_revokes_the_gate() {
        let f = Fixture::new();
        let grant = f.authorize().unwrap();
        assert!(grant.recheck(&f.0.join("state")).is_ok());
        fs::write(f.path(), [grant.bytes.as_slice(), b"\n"].concat()).unwrap();
        assert!(grant.recheck(&f.0.join("state")).is_err());
        fs::write(f.path(), b"{broken").unwrap();
        assert!(grant.recheck(&f.0.join("state")).is_err());
        fs::remove_file(f.path()).unwrap();
        assert!(grant.recheck(&f.0.join("state")).is_err());
    }

    #[test]
    fn linked_or_nonprivate_policy_is_not_an_operator_grant() {
        let f = Fixture::new();
        fs::set_permissions(f.path(), fs::Permissions::from_mode(0o620)).unwrap();
        assert!(f.authorize().is_err());
        f.write();
        let source = f.0.join("repo/policy.json");
        fs::rename(f.path(), &source).unwrap();
        symlink(&source, f.path()).unwrap();
        assert!(f.authorize().is_err());
        fs::remove_file(f.path()).unwrap();
        fs::hard_link(&source, f.path()).unwrap();
        assert!(f.authorize().is_err());
    }

    #[test]
    fn a_source_owned_state_directory_cannot_authorize_itself() {
        let f = Fixture::new();
        let state = f.0.join("repo/state");
        fs::rename(f.0.join("state"), &state).unwrap();
        assert!(authorize(&state, &f.0.join("repo"), RuntimeKind::Claude).is_err());
    }

    #[test]
    fn an_approved_runtime_does_not_authorize_another_provider() {
        let f = Fixture::new();
        assert!(f.authorize().is_ok());
        assert!(authorize(&f.0.join("state"), &f.0.join("repo"), RuntimeKind::Codex).is_err());
    }
}
