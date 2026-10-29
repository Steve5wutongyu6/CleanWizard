//! Duplicate detection, staged by cost: size -> head/tail digest -> SHA-256.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::support::sha256::Sha256;
use crate::support::fsutil::CancelFlag;

#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    /// Fingerprint shared by every member.
    pub hash: String,
    pub size: u64,
    /// Original (first-seen) path; the copy kept by default.
    pub keeper: String,
    /// Redundant copies.
    pub copies: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DuplicateOptions {
    pub roots: Vec<PathBuf>,
    pub min_size: u64,
    pub limit_groups: usize,
    pub include_hidden: bool,
}

impl Default for DuplicateOptions {
    fn default() -> Self {
        DuplicateOptions {
            roots: Vec::new(),
            min_size: 1024,
            limit_groups: 200,
            include_hidden: false,
        }
    }
}

/// Group byte-identical files. The first path in each group is the keeper.
pub fn scan(options: &DuplicateOptions, cancel: &CancelFlag) -> Vec<DuplicateGroup> {
    // Stage 1: size buckets.
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
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
            if !options.include_hidden
                && entry.file_name().to_string_lossy().starts_with('.')
            {
                continue;
            }
            if meta.is_dir() {
                if !is_package(&path) {
                    stack.push(path);
                }
                continue;
            }
            if meta.is_file() && meta.len() >= options.min_size {
                by_size.entry(meta.len()).or_default().push(path);
            }
        }
    }

    let mut groups: Vec<DuplicateGroup> = Vec::new();
    for (size, paths) in by_size {
        if paths.len() < 2 || cancel.is_cancelled() {
            continue;
        }
        // Stage 2: cheap head/tail digest.
        let mut by_digest: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for p in paths {
            if let Some(d) = partial_digest(&p, size) {
                by_digest.entry(d).or_default().push(p);
            }
        }
        for (_digest, candidates) in by_digest {
            if candidates.len() < 2 {
                continue;
            }
            // Stage 3: full SHA-256.
            let mut by_hash: HashMap<String, Vec<PathBuf>> = HashMap::new();
            for p in candidates {
                if let Some(h) = full_hash(&p) {
                    by_hash.entry(h).or_default().push(p);
                }
            }
            for (hash, mut members) in by_hash {
                if members.len() < 2 {
                    continue;
                }
                members.sort();
                let keeper = members.remove(0);
                groups.push(DuplicateGroup {
                    hash,
                    size,
                    keeper: keeper.to_string_lossy().to_string(),
                    copies: members.iter().map(|p| p.to_string_lossy().to_string()).collect(),
                });
            }
        }
    }

    groups.sort_by(|a, b| {
        let a_total = a.size * a.copies.len() as u64;
        let b_total = b.size * b.copies.len() as u64;
        b_total.cmp(&a_total)
    });
    groups.truncate(options.limit_groups);
    groups
}

fn partial_digest(path: &Path, size: u64) -> Option<String> {
    const WINDOW: u64 = 64 * 1024;
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; WINDOW.min(size) as usize];
    let n = file.read(&mut buf).ok()?;
    hasher.update(&buf[..n]);
    if size > WINDOW {
        file.seek(SeekFrom::End(-(WINDOW as i64))).ok()?;
        let n = file.read(&mut buf).ok()?;
        hasher.update(&buf[..n]);
    }
    Some(crate::support::sha256::hex(&hasher.finish()))
}

fn full_hash(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = reader.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(crate::support::sha256::hex(&hasher.finish()))
}

fn is_package(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("app" | "framework" | "bundle" | "photoslibrary" | "xcodeproj")
    )
}
