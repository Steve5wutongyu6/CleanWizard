//! Cleaning: plan what will be removed, then execute through an injected
//! executor so the plan can be inspected and tested without touching disk.

pub mod exercise;

use std::path::{Path, PathBuf};

pub use exercise::{remove_to_trash, RecordingExecutor, TrashExecutor};

/// What a clean run is asked to do.
#[derive(Debug, Clone)]
pub struct PlanOptions {
    /// Keep one member per duplicate group (the keeper is never removed).
    pub keep_duplicate_keeper: bool,
    /// Refuse to remove anything inside these roots (safety net).
    pub protected_roots: Vec<PathBuf>,
}

impl Default for PlanOptions {
    fn default() -> Self {
        PlanOptions {
            keep_duplicate_keeper: true,
            protected_roots: Vec::new(),
        }
    }
}

/// A resolved clean plan: absolute paths that will be handed to the executor.
#[derive(Debug, Clone, Default)]
pub struct CleanPlan {
    pub paths: Vec<PathBuf>,
    pub skipped: Vec<(String, String)>,
}

impl CleanPlan {
    /// Build a plan from arbitrary requested paths, dropping anything unsafe.
    pub fn from_paths<S: AsRef<str>>(requested: &[S], options: &PlanOptions) -> CleanPlan {
        let mut plan = CleanPlan::default();
        for raw in requested {
            let path = Path::new(raw.as_ref());
            if !path.is_absolute() {
                plan.skipped
                    .push((raw.as_ref().to_string(), "not an absolute path".to_string()));
                continue;
            }
            if is_dangerous(path) {
                plan.skipped
                    .push((raw.as_ref().to_string(), "protected path".to_string()));
                continue;
            }
            if options
                .protected_roots
                .iter()
                .any(|root| path.starts_with(root))
            {
                plan.skipped
                    .push((raw.as_ref().to_string(), "inside a protected root".to_string()));
                continue;
            }
            plan.paths.push(path.to_path_buf());
        }
        plan
    }
}

/// Paths that must never be removed, whatever the rules say.
pub fn is_dangerous(path: &Path) -> bool {
    const FORBIDDEN: &[&str] = &[
        "/",
        "/System",
        "/bin",
        "/sbin",
        "/usr",
        "/etc",
        "/var",
        "/private",
        "/dev",
        "/Library",
        "/Applications",
        "/Users",
        "/Volumes",
    ];
    if FORBIDDEN.iter().any(|p| path == Path::new(p)) {
        return true;
    }
    // Never descend into anything the system owns.
    const SYSTEM_ROOTS: &[&str] = &[
        "/System",
        "/usr",
        "/bin",
        "/sbin",
        "/etc",
        "/dev",
        "/cores",
        "/private/etc",
        "/private/var/db",
        "/Applications",
        "/Users",
        "/Volumes",
    ];
    if SYSTEM_ROOTS.iter().any(|root| path.starts_with(root)) {
        return true;
    }
    // Refuse the home directory itself and its top-level containers.
    if let Some(home) = crate::support::fsutil::home_dir() {
        if path == home {
            return true;
        }
        for name in ["Library", "Documents", "Desktop", "Downloads", "Pictures", "Movies", "Music"] {
            if path == home.join(name) && name != "Downloads" {
                return true;
            }
        }
    }
    false
}

/// Outcome of one executor run.
#[derive(Debug, Clone, Default)]
pub struct CleanReport {
    pub removed: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub bytes: u64,
}

/// Apply a plan.
pub fn execute<E: exercise::Executor>(plan: &CleanPlan, executor: &E) -> CleanReport {
    let mut report = CleanReport::default();
    for path in &plan.paths {
        let (bytes, _) = crate::scanner::scan::tree_size(
            path,
            &crate::scanner::scan::ScanOptions::default(),
        );
        let size = if bytes == 0 {
            std::fs::symlink_metadata(path)
                .map(|m| m.len())
                .unwrap_or(0)
        } else {
            bytes
        };
        match executor.trash(path) {
            Ok(()) => {
                report.bytes += size;
                report.removed.push(path.to_string_lossy().to_string());
            }
            Err(reason) => report
                .failed
                .push((path.to_string_lossy().to_string(), reason)),
        }
    }
    report
}
