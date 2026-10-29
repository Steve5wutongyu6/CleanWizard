
//! Filesystem helpers: unique scratch directories for tests, canonical
//! home-directory resolution, and lightweight path normalization.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;
#[cfg(test)]
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

#[cfg(test)]
static SCRATCH_SEQ: AtomicU64 = AtomicU64::new(0);

/// A self-deleting temporary directory used exclusively by tests.
///
/// It intentionally lives in the engine crate (instead of `tempfile`) so
/// `cargo test` runs with `--offline` and no registry.
#[cfg(test)]
pub struct TestDir {
    path: PathBuf,
}

#[cfg(test)]
impl TestDir {
    pub fn new(tag: &str) -> Self {
        let seq = SCRATCH_SEQ.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut path = PathBuf::from(std::env::temp_dir());
        path.push(format!(
            "cw-engine-test-{}-{}-{}-{}",
            tag,
            std::process::id(),
            nanos,
            seq
        ));
        std::fs::create_dir_all(&path).expect("create test dir");
        TestDir { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Create `name` with exactly `len` bytes of filler content.
    pub fn write_file(&self, name: &str, len: usize) -> PathBuf {
        let p = self.path.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("mkdirs for test file");
        }
        std::fs::write(&p, vec![b'x'; len]).expect("write test file");
        p
    }

    pub fn write_bytes(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.path.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("mkdirs for test file");
        }
        std::fs::write(&p, bytes).expect("write test file");
        p
    }

    pub fn mkdir(&self, name: &str) -> PathBuf {
        let p = self.path.join(name);
        std::fs::create_dir_all(&p).expect("mkdir");
        p
    }
}

#[cfg(test)]
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
impl std::fmt::Debug for TestDir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TestDir").field(&self.path).finish()
    }
}

/// Resolve the caller's home directory without shelling out.
pub fn home_dir() -> Option<PathBuf> {
    if let Some(h) = std::env::var_os("HOME") {
        let p = PathBuf::from(h);
        if !p.as_os_str().is_empty() {
            return Some(p);
        }
    }
    let user = std::env::var_os("USER")?;
    let p = PathBuf::from("/Users").join(user);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

/// Expand `~` / `$HOME` / `${HOME}` prefixes.
pub fn expand_home(raw: &str, home: &Path) -> String {
    let s = raw.trim();
    if let Some(rest) = s.strip_prefix("~/") {
        return home.join(rest).to_string_lossy().into_owned();
    }
    if s == "~" {
        return home.to_string_lossy().into_owned();
    }
    if let Some(rest) = s.strip_prefix("$HOME/") {
        return home.join(rest).to_string_lossy().into_owned();
    }
    if let Some(rest) = s.strip_prefix("${HOME}/") {
        return home.join(rest).to_string_lossy().into_owned();
    }
    if s == "$HOME" || s == "${HOME}" {
        return home.to_string_lossy().into_owned();
    }
    s.to_string()
}

/// Lexically normalize a path (`//` -> `/`, drop trailing slash) without
/// touching the filesystem. Symlinks are intentionally not resolved.
pub fn normalize_path(raw: &str, home: &Path) -> String {
    let expanded = expand_home(raw, home);
    let p = Path::new(&expanded);
    let mut buf = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !buf.pop() {
                    buf.push("..");
                }
            }
            other => buf.push(other.as_os_str()),
        }
    }
    let mut display = buf.to_string_lossy().into_owned();
    if display.is_empty() {
        display.push('.');
    }
    display
}

/// A shared, atomic "cancelled" flag consumable across scan threads.
#[derive(Clone, Debug, Default)]
pub struct CancelFlag(Arc<std::sync::atomic::AtomicBool>);

impl CancelFlag {
    pub fn new() -> Self {
        CancelFlag::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_tilde_and_home() {
        let home = Path::new("/Users/tester");
        assert_eq!(expand_home("~/Library/Caches", home), "/Users/tester/Library/Caches");
        assert_eq!(expand_home("~", home), "/Users/tester");
        assert_eq!(expand_home("$HOME/Library/Logs", home), "/Users/tester/Library/Logs");
        assert_eq!(expand_home("${HOME}/tmp", home), "/Users/tester/tmp");
        assert_eq!(expand_home("/Library/Caches", home), "/Library/Caches");
    }

    #[test]
    fn normalizes_paths() {
        let home = Path::new("/Users/tester");
        assert_eq!(normalize_path("~/Library/Caches/", home), "/Users/tester/Library/Caches");
        assert_eq!(normalize_path("~/Library//Caches", home), "/Users/tester/Library/Caches");
        assert_eq!(
            normalize_path("~/Library/../Library/Caches", home),
            "/Users/tester/Library/Caches"
        );
    }

    #[test]
    fn test_dir_creates_and_counts_bytes() {
        let dir = TestDir::new("fsutil");
        let f = dir.write_file("a/b.bin", 123);
        assert_eq!(std::fs::metadata(&f).unwrap().len(), 123);
        assert!(dir.path().exists());
    }

    #[test]
    fn home_dir_is_available_on_macos() {
        assert!(home_dir().is_some());
    }

    #[test]
    fn cancel_flag_is_shared() {
        let a = CancelFlag::new();
        let b = a.clone();
        assert!(!b.is_cancelled());
        a.cancel();
        assert!(b.is_cancelled());
    }
}
