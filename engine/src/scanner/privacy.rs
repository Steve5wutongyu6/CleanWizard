//! Privacy traces: browser history, cookies, session state and logs.

use std::path::{Path, PathBuf};

use crate::support::fsutil::CancelFlag;

#[derive(Debug, Clone)]
pub struct PrivacyTarget {
    /// Human label, e.g. "Safari History".
    pub title: String,
    /// App that owns the trace.
    pub app: String,
    /// Files or directories to clear.
    pub paths: Vec<String>,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct PrivacySpec {
    pub title: String,
    pub app: String,
    pub roots: Vec<PathBuf>,
}

impl PrivacySpec {
    pub fn new(title: &str, app: &str, roots: &[&str]) -> PrivacySpec {
        PrivacySpec {
            title: title.to_string(),
            app: app.to_string(),
            roots: roots.iter().map(PathBuf::from).collect(),
        }
    }
}

/// The built-in trace catalogue, resolved against the current home directory.
pub fn default_specs(home: &Path) -> Vec<PrivacySpec> {
    let h = |s: &str| home.join(s).to_string_lossy().to_string();
    vec![
        PrivacySpec::new("Safari History", "Safari", &[&h("Library/Safari/History.db")]),
        PrivacySpec::new(
            "Safari Cache",
            "Safari",
            &[&h("Library/Caches/com.apple.Safari")],
        ),
        PrivacySpec::new(
            "Chrome History",
            "Chrome",
            &[&h("Library/Application Support/Google/Chrome/Default/History")],
        ),
        PrivacySpec::new(
            "Chrome Cache",
            "Chrome",
            &[&h("Library/Caches/Google/Chrome")],
        ),
        PrivacySpec::new(
            "Firefox Cache",
            "Firefox",
            &[&h("Library/Caches/Firefox")],
        ),
        PrivacySpec::new(
            "Firefox Profiles",
            "Firefox",
            &[&h("Library/Application Support/Firefox/Profiles")],
        ),
        PrivacySpec::new("Recent Items", "macOS", &[&h("Library/Recent Items")]),
        PrivacySpec::new("Saved State", "macOS", &[&h("Library/Saved Application State")]),
    ]
}

/// Measure each existing trace; empty targets are dropped.
pub fn scan(specs: &[PrivacySpec], cancel: &CancelFlag) -> Vec<PrivacyTarget> {
    let mut out = Vec::new();
    for spec in specs {
        if cancel.is_cancelled() {
            break;
        }
        let mut paths = Vec::new();
        let mut size = 0u64;
        for root in &spec.roots {
            let Ok(meta) = std::fs::symlink_metadata(root) else {
                continue;
            };
            let (bytes, _) = crate::scanner::scan::tree_size(
                root,
                &crate::scanner::scan::ScanOptions::default(),
            );
            if meta.is_file() {
                size += meta.len();
            } else {
                size += bytes;
            }
            paths.push(root.to_string_lossy().to_string());
        }
        if !paths.is_empty() {
            out.push(PrivacyTarget {
                title: spec.title.clone(),
                app: spec.app.clone(),
                paths,
                size,
            });
        }
    }
    out
}
