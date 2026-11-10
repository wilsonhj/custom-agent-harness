//! Content-addressed object files: `objects/<first two hex>/<sha256>`. The layout is private
//! to this module; callers name an artifact only by its id.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read as _, Write as _};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use harness_core::ids::ArtifactId;

use crate::StoreError;

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn object_path(objects: &Path, id: &ArtifactId) -> PathBuf {
    let hex = id.as_str();
    objects.join(&hex[..2]).join(hex)
}

fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

fn ensure_dir(dir: &Path) -> io::Result<()> {
    match fs::create_dir(dir) {
        Ok(()) => {
            if let Some(parent) = dir.parent() {
                sync_dir(parent)?;
            }
            Ok(())
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

fn read_verified(path: &Path, id: &ArtifactId) -> Result<Option<Vec<u8>>, StoreError> {
    let mut bytes = Vec::new();
    match File::open(path) {
        Ok(mut f) => {
            f.read_to_end(&mut bytes)?;
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    if ArtifactId::of(&bytes) != *id {
        return Err(StoreError::CorruptArtifact(id.clone()));
    }
    Ok(Some(bytes))
}

/// Writes the object (exclusive create of a temporary file, fsync, link into place, fsync the
/// directory) unless an intact copy is already there. Returns once the bytes are durable.
pub(crate) fn write_object(
    objects: &Path,
    id: &ArtifactId,
    bytes: &[u8],
) -> Result<(), StoreError> {
    let path = object_path(objects, id);
    match read_verified(&path, id) {
        Ok(Some(_)) => return Ok(()),
        Ok(None) | Err(StoreError::CorruptArtifact(_)) => {}
        Err(e) => return Err(e),
    }
    let dir = objects.join(&id.as_str()[..2]);
    ensure_dir(&dir)?;
    let tmp_dir = objects.join("tmp");
    ensure_dir(&tmp_dir)?;
    let tmp = tmp_dir.join(format!(
        "{}.{}.{}",
        id.as_str(),
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    // `rename` replaces a corrupt copy; an intact concurrent copy has the same bytes.
    let renamed = fs::rename(&tmp, &path);
    if renamed.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    renamed?;
    sync_dir(&dir)?;
    Ok(())
}

/// The object's bytes, re-hashed: a mismatch is `CorruptArtifact`, a missing file `NotFound`.
pub(crate) fn read_object(objects: &Path, id: &ArtifactId) -> Result<Vec<u8>, StoreError> {
    read_verified(&object_path(objects, id), id)?.ok_or_else(|| StoreError::NotFound(id.clone()))
}
