//! Large-file scan.

use std::path::{Path, PathBuf};

use crate::support::fsutil::CancelFlag;

#[derive(Debug, Clone)]
pub struct BigFile {
    pub path: String,
    pub size: u64,
    pub modified_secs: i64,
}

#[derive(Debug, Clone)]
pub struct BigFileOptions {
    pub roots: Vec<PathBuf>,
    pub min_size: u64,
    pub limit: usize,
    pub include_hidden: bool,
}

impl Default for BigFileOptions {
    fn default() -> Self {
        BigFileOptions {
            roots: Vec::new(),
            min_size: 50 * 1024 * 1024,
            limit: 500,
            include_hidden: false,
        }
    }
}

/// Files at or above `min_size`, largest first.
pub fn scan(options: &BigFileOptions, cancel: &CancelFlag) -> Vec<BigFile> {
    let mut found: Vec<BigFile> = Vec::new();
    let mut stack: Vec<PathBuf> = options.roots.clone();

    while let Some(dir) = stack.pop() {
        if cancel.is_cancelled() {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if !options.include_hidden {
                if entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
            }
            if meta.is_dir() {
                if !is_package(&path) {
                    stack.push(path);
                }
                continue;
            }
            if meta.is_file() && meta.len() >= options.min_size {
                found.push(BigFile {
                    path: path.to_string_lossy().to_string(),
                    size: meta.len(),
                    modified_secs: mtime(&meta),
                });
            }
        }
    }

    found.sort_by(|a, b| b.size.cmp(&a.size));
    found.truncate(options.limit);
    found
}

fn is_package(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("app" | "framework" | "bundle" | "photoslibrary" | "xcodeproj")
    )
}

fn mtime(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
