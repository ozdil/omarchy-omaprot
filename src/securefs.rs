//! HANCORE symlink- and owner-safe file I/O module for OmaProton.
//! Enforces O_NOFOLLOW, symlink_metadata verification, 1 MiB bounded reads (take(MAX + 1)),
//! and atomic .tmp_* writes with 0600 file and 0700 directory permissions.

use std::fs;
use std::io::Read;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

extern "C" {
    fn getuid() -> u32;
}

pub const O_NOFOLLOW: i32 = 0o400000;
pub const MAX_READ_BYTES: u64 = 1_048_576; // 1 MiB HANCORE ceiling

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn current_uid() -> u32 {
    // SAFETY: POSIX getuid() is thread-safe and has no undefined behavior.
    unsafe { getuid() }
}

/// Resolves `$<xdg_var>/omaprot/<file>`, falling back to `$HOME/<home_rel>/omaprot/<file>`.
pub fn xdg_path(xdg_var: &str, home_rel: &str, file: &str) -> Option<PathBuf> {
    if let Ok(base) = std::env::var(xdg_var) {
        if base.starts_with('/') && !base.contains("..") {
            return Some(PathBuf::from(base).join("omaprot").join(file));
        }
    }
    match std::env::var("HOME") {
        Ok(home) if home.starts_with('/') && !home.contains("..") => Some(
            PathBuf::from(home)
                .join(home_rel)
                .join("omaprot")
                .join(file),
        ),
        _ => None,
    }
}

/// Reads a system or procfs/sysfs file with a strict `take(max_bytes + 1)` ceiling.
/// Rejects symlinks when `reject_symlink` is true.
pub fn read_bounded_file(p: &Path, max_bytes: u64, reject_symlink: bool) -> Option<String> {
    let cap = max_bytes.min(MAX_READ_BYTES);
    let meta = fs::symlink_metadata(p).ok()?;
    if reject_symlink && meta.file_type().is_symlink() {
        return None;
    }
    if !meta.file_type().is_file() && !meta.file_type().is_symlink() {
        return None;
    }

    let mut opts = fs::OpenOptions::new();
    opts.read(true);
    if reject_symlink {
        opts.custom_flags(O_NOFOLLOW);
    }
    let f = opts.open(p).ok()?;

    let mut buf = Vec::with_capacity((cap as usize).min(8192));
    f.take(cap + 1).read_to_end(&mut buf).ok()?;
    if buf.len() as u64 > cap {
        return None;
    }
    String::from_utf8(buf).ok()
}

/// Reads a regular, non-symlink file owned by the current user up to `max_bytes`.
pub fn read_owned_file(p: &Path, max_bytes: u64) -> Option<String> {
    let cap = max_bytes.min(MAX_READ_BYTES);
    let uid = current_uid();

    let meta = fs::symlink_metadata(p).ok()?;
    if meta.file_type().is_symlink() || !meta.file_type().is_file() || meta.uid() != uid {
        return None;
    }

    let mut opts = fs::OpenOptions::new();
    opts.read(true).custom_flags(O_NOFOLLOW);
    let f = opts.open(p).ok()?;

    let fmeta = f.metadata().ok()?;
    if !fmeta.file_type().is_file() || fmeta.uid() != uid || fmeta.len() > cap {
        return None;
    }

    let mut buf = Vec::with_capacity((fmeta.len() as usize).min(8192));
    f.take(cap + 1).read_to_end(&mut buf).ok()?;
    if buf.len() as u64 > cap {
        return None;
    }
    String::from_utf8(buf).ok()
}

/// Safely creates a directory chain where the leaf directory is enforced as 0700 (or `dir_mode`)
/// and verified not to be a symlink.
pub fn ensure_owned_dir(dir: &Path, dir_mode: u32) -> bool {
    if !dir.exists() {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(dir_mode);
        if builder.create(dir).is_err() {
            return false;
        }
    }
    let meta = match fs::symlink_metadata(dir) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if meta.file_type().is_symlink() || !meta.file_type().is_dir() || meta.uid() != current_uid() {
        return false;
    }
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(dir_mode));
    true
}

/// Atomically writes `data` to `p` via a `.tmp_*` sibling file with `file_mode` permissions
/// (`0o600` for config/state, `0o755` for wrapper scripts inside owned tool directories).
pub fn write_owned_file_atomic_mode(p: &Path, data: &[u8], dir_mode: u32, file_mode: u32) -> bool {
    use std::io::Write;

    let parent = match p.parent() {
        Some(par) => par,
        None => return false,
    };

    if !ensure_owned_dir(parent, dir_mode) {
        return false;
    }

    let uid = current_uid();
    if let Ok(meta) = fs::symlink_metadata(p) {
        if meta.file_type().is_symlink() || !meta.file_type().is_file() || meta.uid() != uid {
            return false;
        }
    }

    struct TempFileGuard<'a> {
        path: &'a Path,
        active: bool,
    }

    impl<'a> Drop for TempFileGuard<'a> {
        fn drop(&mut self) {
            if self.active {
                let _ = fs::remove_file(self.path);
            }
        }
    }

    let pid = std::process::id();
    let stem = p
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());

    let mut created_file = None;
    let mut tmp_path_buf = PathBuf::new();

    for _ in 0..10 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".tmp_{}_{}_{}_{}", stem, pid, nanos, seq));

        let mut opts = fs::OpenOptions::new();
        opts.write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(O_NOFOLLOW);

        match opts.open(&candidate) {
            Ok(f) => {
                tmp_path_buf = candidate;
                created_file = Some(f);
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return false,
        }
    }

    let mut tmp_file = match created_file {
        Some(f) => f,
        None => return false,
    };

    let mut guard = TempFileGuard {
        path: &tmp_path_buf,
        active: true,
    };

    if tmp_file.write_all(data).is_err() {
        return false;
    }
    if tmp_file.sync_all().is_err() {
        return false;
    }

    if let Ok(meta) = tmp_file.metadata() {
        if meta.len() != data.len() as u64 {
            return false;
        }
    } else {
        return false;
    }

    let _ = tmp_file.set_permissions(fs::Permissions::from_mode(file_mode));
    drop(tmp_file);

    if let Ok(meta) = fs::symlink_metadata(p) {
        if meta.file_type().is_symlink() || !meta.file_type().is_file() || meta.uid() != uid {
            return false;
        }
    }

    if fs::rename(&tmp_path_buf, p).is_ok() {
        let _ = fs::set_permissions(p, fs::Permissions::from_mode(file_mode));
        guard.active = false;
        return true;
    }
    false
}

/// Atomically writes user config/state with strict HANCORE 0700 dir and 0600 file permissions.
pub fn write_owned_file_atomic(p: &Path, data: &[u8]) -> bool {
    write_owned_file_atomic_mode(p, data, 0o700, 0o600)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atomic_write_and_bounded_read() {
        let tmp_dir = std::env::temp_dir().join(format!("omaprot_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp_dir);
        let file_path = tmp_dir.join("config.json");

        let payload = b"{\"profile\":\"bleeding-edge\"}";
        assert!(write_owned_file_atomic(&file_path, payload));

        let dir_meta = fs::symlink_metadata(&tmp_dir).unwrap();
        assert_eq!(dir_meta.permissions().mode() & 0o777, 0o700);

        let file_meta = fs::symlink_metadata(&file_path).unwrap();
        assert_eq!(file_meta.permissions().mode() & 0o777, 0o600);

        let read_back = read_owned_file(&file_path, 1024).unwrap();
        assert_eq!(read_back, "{\"profile\":\"bleeding-edge\"}");

        // Verify ceiling rejection when max_bytes is smaller than file size
        assert!(read_owned_file(&file_path, 5).is_none());

        let _ = fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_symlink_rejection() {
        let tmp_dir = std::env::temp_dir().join(format!("omaprot_sym_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp_dir);
        assert!(ensure_owned_dir(&tmp_dir, 0o700));

        let real_file = tmp_dir.join("real.txt");
        let sym_file = tmp_dir.join("link.txt");
        assert!(write_owned_file_atomic(&real_file, b"secret"));
        std::os::unix::fs::symlink(&real_file, &sym_file).unwrap();

        assert!(read_owned_file(&sym_file, 1024).is_none());
        assert!(!write_owned_file_atomic(&sym_file, b"overwrite"));

        let _ = fs::remove_dir_all(&tmp_dir);
    }
}
