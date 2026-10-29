//! Concurrent scanning: turn `PathSpec` roots into sized, filtered items.
//!
//! Semantics mirror the original cleaner: a `<path>` names a scan root; the
//! filters attached to it decide which entries beneath that root are cleanable.
//! An included directory contributes its whole subtree and is not descended
//! into again (no double counting).

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::rules::matcher::{Candidate, Decision, FileMeta, Matcher, PathSpec};
use crate::rules::model::{CleanType, FilterGroup};
use crate::rules::RuleSet;
use crate::support::fsutil::CancelFlag;
use crate::support::threadpool::ThreadPool;

/// What the caller wants scanned.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    /// Only these clean types, when set.
    pub clean_types: Option<Vec<CleanType>>,
    /// Only these item keys, when set.
    pub item_keys: Option<Vec<String>>,
    pub options: ScanOptions,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Include dot-files and dot-directories.
    pub include_hidden: bool,
    /// Count empty directories as removable.
    pub include_empty_dirs: bool,
    /// How many representative paths to keep per item.
    pub sample_limit: usize,
    /// Worker threads; `None` picks a sensible default.
    pub workers: Option<u32>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            include_hidden: true,
            include_empty_dirs: false,
            sample_limit: 64,
            workers: None,
        }
    }
}

/// One cleanable unit, ready for display and cleaning.
#[derive(Debug, Clone)]
pub struct ItemResult {
    pub id: String,
    pub title: String,
    pub category_title: String,
    pub clean_type: CleanType,
    pub size: u64,
    pub file_count: u64,
    pub paths: Vec<String>,
    pub is_caution: bool,
    pub selected: bool,
    pub tips: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ScanOutcome {
    pub items: Vec<ItemResult>,
    pub total_size: u64,
    pub total_files: u64,
    pub duration_ms: u64,
    pub cancelled: bool,
    /// Roots that could not be scanned (permission denied, vanished, ...).
    pub skipped: Vec<(String, String)>,
}

#[derive(Debug, Default)]
struct Hits {
    size: u64,
    files: u64,
    paths: Vec<String>,
}

impl Hits {
    fn merge(&mut self, other: Hits) {
        self.size += other.size;
        self.files += other.files;
        if self.paths.len() < other.paths.len().saturating_add(self.paths.len()) {
            self.paths.extend(other.paths);
        }
    }
}

/// Scan every rule root and aggregate the result per item.
pub fn scan(rules: &RuleSet, scope: &Scope) -> ScanOutcome {
    let started = Instant::now();
    let cancel = CancelFlag::new();
    let options = scope.options.clone();

    let (specs, _rejected) = rules.path_specs();
    let specs: Vec<PathSpec> = specs
        .into_iter()
        .filter(|s| scope_allows(scope, s))
        .collect();

    let mut skipped: Vec<(String, String)> = Vec::new();
    let workers = crate::support::threadpool::worker_count_for_configured(options.workers);
    let pool = ThreadPool::new(workers);

    let keys: Vec<String> = specs.iter().map(|s| s.item_key.clone()).collect();
    let options_for_job = options.clone();
    let cancel_for_job = cancel.clone();
    let results = pool.map_ordered_total(specs, move |_idx, spec| {
        scan_one(&spec, &options_for_job, &cancel_for_job)
    });

    // Group by item key; warnings come back as the Err half.
    let mut buckets: Vec<(String, Hits)> = Vec::new();
    for (key, outcome) in keys.into_iter().zip(results) {
        match outcome {
            Ok(hits) => {
                if hits.size == 0 && hits.files == 0 {
                    continue;
                }
                match buckets.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, slot)) => slot.merge(hits),
                    None => buckets.push((key, hits)),
                }
            }
            Err((path, reason)) => skipped.push((path, reason)),
        }
    }

    let mut items: Vec<ItemResult> = Vec::new();
    for (category, item) in rules.items() {
        if !scope_allows_item(scope, &item.key()) {
            continue;
        }
        let Some((_, hits)) = buckets.iter().find(|(k, _)| *k == item.key()) else {
            continue;
        };
        let mut paths = hits.paths.clone();
        paths.sort();
        paths.dedup();
        paths.truncate(options.sample_limit.max(1));
        items.push(ItemResult {
            id: item.key(),
            title: item.title.clone(),
            category_title: category.title.clone(),
            clean_type: item.clean_type,
            size: hits.size,
            file_count: hits.files,
            paths,
            is_caution: item.is_cautious,
            selected: item.default_state && !item.is_cautious,
            tips: item.tips.clone(),
        });
    }

    items.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.title.cmp(&b.title)));

    let total_size = items.iter().map(|i| i.size).sum();
    let total_files = items.iter().map(|i| i.file_count).sum();
    let duration_ms = started.elapsed().as_millis() as u64;

    ScanOutcome {
        items,
        total_size,
        total_files,
        duration_ms,
        cancelled: cancel.is_cancelled(),
        skipped,
    }
}

fn scope_allows(scope: &Scope, spec: &PathSpec) -> bool {
    if let Some(types) = &scope.clean_types {
        if !types.contains(&spec.clean_type) {
            return false;
        }
    }
    scope_allows_item(scope, &spec.item_key)
}

fn scope_allows_item(scope: &Scope, key: &str) -> bool {
    match &scope.item_keys {
        Some(keys) => keys.iter().any(|k| k == key),
        None => true,
    }
}

/// Scan a single rule root.
fn scan_one(
    spec: &PathSpec,
    options: &ScanOptions,
    cancel: &CancelFlag,
) -> Result<Hits, (String, String)> {
    let root = spec.path.clone();
    let meta = match std::fs::symlink_metadata(&root) {
        Ok(m) => m,
        Err(e) => return Err((root.display().to_string(), e.to_string())),
    };

    let mut matcher = Matcher::new();
    let mut groups: Vec<FilterGroup> = spec.filters.clone();
    groups.extend(spec.global_filters.iter().cloned());

    let mut hits = Hits::default();
    let mut budget = options.sample_limit.max(1);

    if meta.file_type().is_symlink() {
        return Ok(hits);
    }

    if meta.is_file() {
        if let Some(path) = consider(&root, &mut matcher, &groups, options) {
            hits.size += meta.len();
            hits.files += 1;
            if budget > 0 {
                hits.paths.push(path);
            }
        }
        return Ok(hits);
    }

    visit(
        &root,
        spec,
        &mut matcher,
        &groups,
        options,
        cancel,
        &mut hits,
        &mut budget,
        0,
    );
    Ok(hits)
}

/// Depth-first walk; included directories swallow their subtree.
#[allow(clippy::too_many_arguments)]
fn visit(
    dir: &Path,
    spec: &PathSpec,
    matcher: &mut Matcher,
    groups: &[FilterGroup],
    options: &ScanOptions,
    cancel: &CancelFlag,
    hits: &mut Hits,
    budget: &mut usize,
    depth: u32,
) {
    if cancel.is_cancelled() || depth > 64 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !options.include_hidden && name.starts_with('.') {
            continue;
        }

        if let Some(candidate_path) = consider(&path, matcher, groups, options) {
            let (bytes, files) = tree_size(&path, options);
            if bytes == 0 && files == 0 && !options.include_empty_dirs {
                continue;
            }
            hits.size += bytes;
            hits.files += files;
            if *budget > 0 {
                hits.paths.push(candidate_path);
                *budget -= 1;
            }
            continue;
        }

        if meta.is_dir() {
            visit(
                &path, spec, matcher, groups, options, cancel, hits, budget, depth + 1,
            );
        }
    }
}

/// Return the candidate path when the filters include this entry.
fn consider(
    path: &Path,
    matcher: &mut Matcher,
    groups: &[FilterGroup],
    options: &ScanOptions,
) -> Option<String> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    let info = FileMeta {
        is_dir: meta.is_dir(),
        size: if meta.is_file() { meta.len() } else { 0 },
        children: 0,
        is_symlink: false,
        is_package: is_package(path),
        modified_secs: mtime_secs(&meta),
    };
    let shown = path.to_string_lossy().to_string();
    let candidate = Candidate::new(&shown, &info).with_last_component();
    match matcher.decide(&candidate, groups) {
        Decision::Include => Some(shown),
        _ if groups.is_empty() && options.include_empty_dirs && info.is_dir => None,
        _ => None,
    }
}

fn is_package(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("app" | "framework" | "bundle" | "xcodeproj" | "xcworkspace" | "plugin" | "kext" | "pkg")
    )
}

fn mtime_secs(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Total bytes and file count beneath `path` (symlinks excluded).
pub fn tree_size(path: &Path, options: &ScanOptions) -> (u64, u64) {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return (0, 0);
    };
    if meta.file_type().is_symlink() {
        return (0, 0);
    }
    if meta.is_file() {
        return (meta.len(), 1);
    }
    let mut bytes = 0u64;
    let mut files = 0u64;
    let mut stack: Vec<PathBuf> = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(m) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if m.file_type().is_symlink() {
                continue;
            }
            if !options.include_hidden {
                let name = entry.file_name();
                if name.to_string_lossy().starts_with('.') {
                    continue;
                }
            }
            if m.is_dir() {
                stack.push(entry.path());
            } else if m.is_file() {
                bytes += m.len();
                files += 1;
            }
        }
    }
    (bytes, files)
}
