//! Task worktrees, the spec packet digest, the Git-metadata scan and checkpoint commits.
//!
//! Agents never run Git. The harness reads what they wrote with host Git ([`crate::hostgit`])
//! only after [`scan`] has confirmed there is no nested repository, gitlink or altered `.git`
//! link that would make host Git load configuration the agent controls.

use std::io;
use std::path::{Component, Path, PathBuf};

use harness_core::ids::{ArtifactId, CommitSha, TaskId, Timestamp};
use harness_core::outcome::FailureCode;

use crate::hostgit::{GitError, git, git_with_env};
use crate::plan::TreeGone;
use crate::secretscan::scan_for_secrets;

/// A detached worktree of the task's repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub task: TaskId,
    /// Canonical path, `<root>/<task-id>`.
    pub path: PathBuf,
    /// Canonical Git common dir of the source repository.
    pub git_common_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("task id {0:?} is not a safe path and ref component")]
    InvalidTaskId(String),
    #[error("worktree root {0} resolves inside the source repository")]
    RootInsideRepo(PathBuf),
    #[error("packet path {0:?} must be a relative directory path without `.` or `..`")]
    BadPacketPath(String),
    #[error("packet path {0:?} does not exist at the base commit")]
    PacketNotFound(String),
    #[error("packet entry {0:?} is not a UTF-8 text file")]
    PacketNotText(String),
    #[error("the worktree's .git link was changed")]
    GitMetadataTouched,
}

const IDENTITY_NAME: &str = "eng";
const IDENTITY_EMAIL: &str = "eng@localhost";
const ZERO_OID: &str = "0000000000000000000000000000000000000000";

/// `git worktree add --detach <root>/<task-id> <base>`. Moves no branch and runs no hook.
pub async fn create(
    repo: &Path,
    task: &TaskId,
    base: &CommitSha,
    root: &Path,
) -> Result<Workspace, WorkspaceError> {
    check_task_id(task)?;
    let toplevel = git_path(repo, &["rev-parse", "--show-toplevel"]).await?;
    let common = git_path(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .await?;
    let root_real = resolve_for_creation(root)?;
    if root_real.starts_with(&toplevel) || root_real.starts_with(&common) {
        return Err(WorkspaceError::RootInsideRepo(root.to_path_buf()));
    }
    std::fs::create_dir_all(&root_real)?;
    let path = root_real.join(task.as_str());
    git(
        repo,
        &[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            utf8(&path)?,
            base.as_str(),
        ],
    )
    .await?;
    Ok(Workspace {
        task: task.clone(),
        path,
        git_common_dir: common,
    })
}

/// The packet directory as stored in the `base` commit (never the working directory):
/// its files as `(path relative to packet_path, contents)` sorted by path, and the SHA-256 of
/// [`encode_packet`] over them.
pub async fn packet_digest(
    repo: &Path,
    base: &CommitSha,
    packet_path: &str,
) -> Result<(ArtifactId, Vec<(String, String)>), WorkspaceError> {
    let rel = packet_rel_path(packet_path)?;
    let spec = format!("{base}:{rel}");
    let kind = match git(repo, &["cat-file", "-t", &spec]).await {
        Ok(out) => String::from_utf8_lossy(&out).trim().to_owned(),
        Err(GitError::Failed { .. }) => {
            return Err(WorkspaceError::PacketNotFound(packet_path.to_owned()));
        }
        Err(e) => return Err(e.into()),
    };
    if kind != "tree" {
        return Err(WorkspaceError::BadPacketPath(packet_path.to_owned()));
    }
    let listing = git(repo, &["ls-tree", "-r", "-z", &spec]).await?;
    let mut files = Vec::new();
    for entry in listing.split(|&b| b == 0).filter(|e| !e.is_empty()) {
        let text = std::str::from_utf8(entry)
            .map_err(|_| WorkspaceError::PacketNotText(String::from_utf8_lossy(entry).into()))?;
        let (meta, path) = text
            .split_once('\t')
            .ok_or_else(|| WorkspaceError::PacketNotText(text.to_owned()))?;
        let mut fields = meta.split(' ');
        let (mode, kind, oid) = (fields.next(), fields.next(), fields.next());
        let regular = matches!(mode, Some("100644" | "100755")) && kind == Some("blob");
        let (true, Some(oid)) = (regular, oid) else {
            return Err(WorkspaceError::PacketNotText(path.to_owned()));
        };
        let bytes = git(repo, &["cat-file", "blob", oid]).await?;
        let contents =
            String::from_utf8(bytes).map_err(|_| WorkspaceError::PacketNotText(path.to_owned()))?;
        files.push((path.to_owned(), contents));
    }
    files.sort();
    Ok((ArtifactId::of(&encode_packet(&files)), files))
}

/// The canonical bytes [`packet_digest`] hashes: a version line, then for each file
/// `<path bytes> <content bytes>\n<path>\n<contents>\n`. Store these bytes to keep the
/// packet content-addressed under the same id.
pub fn encode_packet(files: &[(String, String)]) -> Vec<u8> {
    let mut out = b"eng-packet-v1\n".to_vec();
    for (path, contents) in files {
        out.extend_from_slice(format!("{} {}\n", path.len(), contents.len()).as_bytes());
        out.extend_from_slice(path.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(contents.as_bytes());
        out.push(b'\n');
    }
    out
}

/// `Err(GitMetadataTouched)` if the worktree holds a nested repository (any `.git` entry
/// below the root, any letter case), a gitlink in its index, a `.git` link that no longer
/// points at this worktree's entry in the common dir, or an entry directly under the root
/// that aliases one of [`PROTECTED_ROOT_NAMES`] (see [`fold_to_ascii`]). Run before any host
/// Git reads the worktree; I/O errors fail closed with the same code.
pub async fn scan(ws: &Workspace) -> Result<(), FailureCode> {
    let owned = ws.clone();
    let clean = tokio::task::spawn_blocking(move || {
        dot_git_is_ours(&owned)
            && no_nested_git(&owned.path, true)
            && no_protected_root_name(&owned.path)
    })
    .await
    .unwrap_or(false);
    if !clean {
        return Err(FailureCode::GitMetadataTouched);
    }
    let index = git(&ws.path, &["ls-files", "--stage", "-z"])
        .await
        .map_err(|_| FailureCode::GitMetadataTouched)?;
    if index
        .split(|&b| b == 0)
        .any(|entry| entry.starts_with(b"160000 "))
    {
        return Err(FailureCode::GitMetadataTouched);
    }
    Ok(())
}

/// Stages everything (ignored files excluded), commits it on `parent` as `eng <eng@localhost>`
/// at `author_time` (author and committer), and points `refs/harness/tasks/<task>/cp/<n>` at
/// it only if that ref does not exist yet (or already names this commit, so a retry is
/// idempotent). The same tree, parent and time always give the same SHA.
///
/// Requires proof that no process still writes in the worktree, so nothing can change it
/// between [`scan`] and `git add`. [`scan`] runs again after `write-tree`, over the index the
/// tree was written from: any nested repository, gitlink or altered `.git` link that appeared
/// in between fails the commit with `GitMetadataTouched` before a commit object is made.
///
/// Errors: `GitMetadataTouched` from either [`scan`]; `SecretInDiff` if the diff adds a
/// token-shaped string (no ref is created); `CandidateChanged` if the ref names another
/// commit; `SpawnError` if Git cannot run or fails.
pub async fn commit_candidate(
    ws: &Workspace,
    parent: &CommitSha,
    n: u32,
    author_time: Timestamp,
    _proof: TreeGone,
) -> Result<(CommitSha, String), FailureCode> {
    scan(ws).await?;
    check_task_id(&ws.task).map_err(|_| FailureCode::SpawnError)?;
    let dir = ws.path.as_path();
    let infra = |_: GitError| FailureCode::SpawnError;

    git(dir, &["add", "--all"]).await.map_err(infra)?;
    let tree = trimmed(git(dir, &["write-tree"]).await.map_err(infra)?);
    scan(ws).await?;
    let date = format!("@{} +0000", author_time.0.div_euclid(1_000_000));
    let message = format!("eng checkpoint for task {}", ws.task);
    let env = [
        ("GIT_AUTHOR_NAME", IDENTITY_NAME),
        ("GIT_AUTHOR_EMAIL", IDENTITY_EMAIL),
        ("GIT_AUTHOR_DATE", date.as_str()),
        ("GIT_COMMITTER_NAME", IDENTITY_NAME),
        ("GIT_COMMITTER_EMAIL", IDENTITY_EMAIL),
        ("GIT_COMMITTER_DATE", date.as_str()),
    ];
    let args = [
        "commit-tree",
        "--no-gpg-sign",
        "-p",
        parent.as_str(),
        "-m",
        &message,
        &tree,
    ];
    let commit = trimmed(git_with_env(dir, &args, &env).await.map_err(infra)?);
    let commit = CommitSha::parse(&commit).map_err(|_| FailureCode::SpawnError)?;

    let patch = diff(dir, parent, &commit, "--binary")
        .await
        .map_err(infra)?;
    // Scan a text rendering so a token inside a file Git calls binary is still seen.
    let text = diff(dir, parent, &commit, "--text").await.map_err(infra)?;
    if !scan_for_secrets(&text).is_empty() {
        return Err(FailureCode::SecretInDiff);
    }

    let name = format!("refs/harness/tasks/{}/cp/{n}", ws.task);
    if git(dir, &["update-ref", &name, commit.as_str(), ZERO_OID])
        .await
        .is_err()
    {
        let current = git(dir, &["rev-parse", "--verify", "-q", &name])
            .await
            .map(trimmed)
            .unwrap_or_default();
        if current != commit.as_str() {
            return Err(FailureCode::CandidateChanged);
        }
    }
    Ok((commit, patch))
}

/// Makes the worktree exactly `commit`: removes untracked and ignored files (and nested
/// repositories), then `reset --hard`. Requires proof that no process still writes there.
pub async fn reset_to(
    ws: &Workspace,
    commit: &CommitSha,
    _proof: TreeGone,
) -> Result<(), WorkspaceError> {
    let owned = ws.clone();
    let ours = tokio::task::spawn_blocking(move || dot_git_is_ours(&owned))
        .await
        .unwrap_or(false);
    if !ours {
        return Err(WorkspaceError::GitMetadataTouched);
    }
    git(&ws.path, &["clean", "-ffdxq"]).await?;
    git(&ws.path, &["reset", "--hard", "--quiet", commit.as_str()]).await?;
    git(&ws.path, &["clean", "-ffdxq"]).await?;
    Ok(())
}

async fn diff(
    dir: &Path,
    parent: &CommitSha,
    commit: &CommitSha,
    mode: &str,
) -> Result<String, GitError> {
    let out = git(
        dir,
        &[
            "-c",
            "core.quotePath=false",
            "diff",
            mode,
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--no-renames",
            "--no-relative",
            "--full-index",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            parent.as_str(),
            commit.as_str(),
        ],
    )
    .await?;
    Ok(String::from_utf8(out)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
}

/// A task id becomes a directory name and a ref component: `[A-Za-z0-9._-]`, at most 128
/// characters, not starting with `.` or `-`, no `..`, not ending in `.lock`.
fn check_task_id(task: &TaskId) -> Result<(), WorkspaceError> {
    let s = task.as_str();
    let ok = !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && !s.starts_with(['.', '-'])
        && !s.contains("..")
        && !s.ends_with(".lock");
    if ok {
        Ok(())
    } else {
        Err(WorkspaceError::InvalidTaskId(s.to_owned()))
    }
}

/// `a/b` or `a/b/`; rejects absolute paths, empty, `.` and `..` components.
fn packet_rel_path(packet_path: &str) -> Result<&str, WorkspaceError> {
    let rel = packet_path.strip_suffix('/').unwrap_or(packet_path);
    let ok = !rel.is_empty()
        && !rel.starts_with('/')
        && rel.split('/').all(|c| !matches!(c, "" | "." | ".."));
    if ok {
        Ok(rel)
    } else {
        Err(WorkspaceError::BadPacketPath(packet_path.to_owned()))
    }
}

/// Canonical form of a path that may not exist yet: its deepest existing ancestor resolved
/// through symlinks, plus the remaining plain components.
fn resolve_for_creation(path: &Path) -> Result<PathBuf, WorkspaceError> {
    let path = std::path::absolute(path)?;
    for ancestor in path.ancestors() {
        if std::fs::symlink_metadata(ancestor).is_err() {
            continue;
        }
        let mut real = std::fs::canonicalize(ancestor)?;
        let rest = path.strip_prefix(ancestor).map_err(io::Error::other)?;
        for c in rest.components() {
            match c {
                Component::Normal(part) => real.push(part),
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "worktree root has `.` or `..` components",
                    )
                    .into());
                }
            }
        }
        return Ok(real);
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "no existing ancestor").into())
}

async fn git_path(repo: &Path, args: &[&str]) -> Result<PathBuf, WorkspaceError> {
    let out = git(repo, args).await?;
    let text = String::from_utf8(out).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(std::fs::canonicalize(text.trim_end_matches('\n'))?)
}

fn utf8(path: &Path) -> Result<&str, WorkspaceError> {
    path.to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "path is not UTF-8").into())
}

fn trimmed(out: Vec<u8>) -> String {
    String::from_utf8_lossy(&out).trim().to_owned()
}

/// The worktree's `.git` is a regular file `gitdir: <common>/worktrees/<name>` whose entry
/// points back at this worktree.
fn dot_git_is_ours(ws: &Workspace) -> bool {
    let dot_git = ws.path.join(".git");
    let is_file = std::fs::symlink_metadata(&dot_git).is_ok_and(|m| m.file_type().is_file());
    if !is_file {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(&dot_git) else {
        return false;
    };
    let Some(target) = text
        .strip_prefix("gitdir: ")
        .and_then(|t| t.strip_suffix('\n'))
        .filter(|t| !t.contains('\n'))
    else {
        return false;
    };
    let entry = ws.path.join(target);
    let (Ok(entry), Ok(worktrees), Ok(dot_git)) = (
        std::fs::canonicalize(&entry),
        std::fs::canonicalize(ws.git_common_dir.join("worktrees")),
        std::fs::canonicalize(&dot_git),
    ) else {
        return false;
    };
    if entry.parent() != Some(worktrees.as_path()) {
        return false;
    }
    std::fs::read_to_string(entry.join("gitdir")).is_ok_and(|back| {
        let back = Path::new(back.trim_end_matches('\n'));
        std::fs::canonicalize(entry.join(back)).is_ok_and(|b| b == dot_git)
    })
}

/// Agent and Git configuration names the sandbox denies at the worktree root. The sandbox's
/// case-insensitive deny rules match ASCII letters only, so a name that the file system may
/// treat as the same name but spells differently must not exist at all.
pub const PROTECTED_ROOT_NAMES: &[&str] = &[
    ".git",
    ".claude",
    ".mcp.json",
    ".codex",
    ".agents",
    ".cursor",
];

/// No entry directly under `root` whose name folds ([`fold_to_ascii`]) to one of
/// [`PROTECTED_ROOT_NAMES`], except the worktree's own `.git` link (exactly `.git`, checked by
/// [`dot_git_is_ours`]). Unreadable directories and non-UTF-8 names count as touched.
fn no_protected_root_name(root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    for entry in entries {
        let Ok(entry) = entry else { return false };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return false;
        };
        if name == ".git" {
            continue;
        }
        if fold_to_ascii(name).is_some_and(|f| PROTECTED_ROOT_NAMES.contains(&f.as_str())) {
            return false;
        }
    }
    true
}

/// The name after NFC normalisation and full Unicode case folding, if that result is ASCII;
/// `None` if it cannot be (then it cannot equal an ASCII name). Every protected name is ASCII,
/// so only characters whose NFC form or full case fold (Unicode `CaseFolding.txt`, status C
/// and F) is entirely ASCII matter; they are, besides ASCII itself:
///
/// - U+212A KELVIN SIGN: NFC is `K`, folds to `k`.
/// - U+017F LATIN SMALL LETTER LONG S: folds to `s`.
/// - U+00DF and U+1E9E (sharp s): fold to `ss`.
/// - U+FB00..U+FB06 (Latin ligatures): fold to `ff`, `fi`, `fl`, `ffi`, `ffl`, `st`, `st`.
/// - U+0130 (I with dot above) and U+0131 (dotless i): folded to `i`. Full folding gives
///   U+0130 `i` + U+0307 and leaves U+0131 alone; the Turkic folding gives `i` and `ı`.
///   Treating both as `i` over-rejects, which fails closed.
///
/// Every other non-ASCII character, combining marks included, leaves a non-ASCII character
/// in the NFC + case-folded result, so the name cannot equal an ASCII name. Not covered,
/// because APFS does not equate them: compatibility forms such as fullwidth letters (NFKC,
/// not NFC) and default-ignorable characters such as U+200C (ignored by HFS+, not APFS).
fn fold_to_ascii(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii() {
            out.push(c.to_ascii_lowercase());
            continue;
        }
        out.push_str(match c {
            '\u{212A}' => "k",
            '\u{017F}' => "s",
            '\u{00DF}' | '\u{1E9E}' => "ss",
            '\u{FB00}' => "ff",
            '\u{FB01}' => "fi",
            '\u{FB02}' => "fl",
            '\u{FB03}' => "ffi",
            '\u{FB04}' => "ffl",
            '\u{FB05}' | '\u{FB06}' => "st",
            '\u{0130}' | '\u{0131}' => "i",
            _ => return None,
        });
    }
    Some(out)
}

/// No entry named `.git` (any case) anywhere below `dir`, skipping only the root's own link.
/// Symlinks are not followed. Unreadable directories count as touched.
fn no_nested_git(dir: &Path, is_root: bool) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries {
        let Ok(entry) = entry else { return false };
        let name = entry.file_name();
        let is_git = name
            .to_str()
            .is_some_and(|n| n.eq_ignore_ascii_case(".git"));
        if is_git {
            if is_root && name == ".git" {
                continue;
            }
            return false;
        }
        let Ok(kind) = entry.file_type() else {
            return false;
        };
        if kind.is_dir() && !no_nested_git(&entry.path(), false) {
            return false;
        }
    }
    true
}
