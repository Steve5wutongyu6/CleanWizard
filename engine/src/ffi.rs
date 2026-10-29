//! C ABI consumed by the Swift app.
//!
//! Every returned `char *` is heap-allocated and must be released with
//! `cw_engine_free`. All entry points are panic-safe and never perform
//! network I/O.

use std::ffi::{c_char, CStr, CString};
use std::path::Path;

use crate::cleaner::{self, CleanPlan, PlanOptions, TrashExecutor};
use crate::rules::RuleSet;
use crate::scanner::scan::{self, Scope};
use crate::support::json::{self, Json};

/// Returns the engine semantic version.
#[no_mangle]
pub extern "C" fn cw_engine_version() -> *mut c_char {
    to_c_string(crate::ENGINE_VERSION)
}

/// Parses a rule file (or a directory of rule files) and returns a summary.
#[no_mangle]
pub extern "C" fn cw_parse_rules(rules_path: *const c_char) -> *mut c_char {
    guard(|| {
        let path = unsafe { unsafe_str(rules_path) }?;
        let path = Path::new(&path);
        let rules = if path.is_dir() {
            let sets = RuleSet::load_dir(path).map_err(|errs| {
                errs.iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("; ")
            })?;
            let mut merged = RuleSet {
                version: String::new(),
                appstore_version: None,
                categories: Vec::new(),
                cautions: Vec::new(),
                global_filters: Vec::new(),
                source: None,
            };
            for set in sets {
                merged.merge(set);
            }
            merged
        } else {
            RuleSet::load_file(path).map_err(|e| e.to_string())?
        };
        Ok(summary_json(&rules))
    })
}

/// Scans every rule root and returns items plus totals.
#[no_mangle]
pub extern "C" fn cw_scan(rules_path: *const c_char, scope_json: *const c_char) -> *mut c_char {
    guard(|| {
        let rules_path = unsafe { unsafe_str(rules_path) }?;
        let scope_text = unsafe { unsafe_str(scope_json) }.unwrap_or_default();
        let rules = RuleSet::load_file(Path::new(&rules_path)).map_err(|e| e.to_string())?;
        let scope = parse_scope(&scope_text);
        let outcome = scan::scan(&rules, &scope);
        Ok(outcome_json(&outcome))
    })
}

/// Moves the given paths into the trash.
#[no_mangle]
pub extern "C" fn cw_trash(paths_json: *const c_char) -> *mut c_char {
    guard(|| {
        let text = unsafe { unsafe_str(paths_json) }?;
        let parsed = json::parse(&text)?;
        let mut requested: Vec<String> = Vec::new();
        if let Some(arr) = parsed.get("paths").and_then(|v| v.as_arr()) {
            for value in arr {
                if let Some(s) = value.as_str() {
                    requested.push(s.to_string());
                }
            }
        }
        let plan = CleanPlan::from_paths(&requested, &PlanOptions::default());
        let report = cleaner::execute(&plan, &TrashExecutor);

        let mut out = Json::obj();
        let mut removed = Json::arr();
        for path in &report.removed {
            removed.push(path.clone());
        }
        let mut failed = Json::arr();
        for (path, reason) in &report.failed {
            let mut item = Json::obj();
            item.insert("path", path.clone()).insert("reason", reason.clone());
            failed.push(item);
        }
        let mut skipped = Json::arr();
        for (path, reason) in &plan.skipped {
            let mut item = Json::obj();
            item.insert("path", path.clone()).insert("reason", reason.clone());
            skipped.push(item);
        }
        out.insert("trashed", removed)
            .insert("failed", failed)
            .insert("skipped", skipped)
            .insert("bytes", report.bytes as i64);
        Ok(out.to_string_compact())
    })
}

/// Releases a string produced by this library.
#[no_mangle]
pub extern "C" fn cw_engine_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(ptr));
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn guard<F: FnOnce() -> Result<String, String>>(body: F) -> *mut c_char {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(Ok(text)) => to_c_string(&text),
        Ok(Err(reason)) => to_c_string(&error_json(&reason)),
        Err(_) => to_c_string(&error_json("engine panicked")),
    }
}

fn error_json(reason: &str) -> String {
    let mut out = Json::obj();
    out.insert("error", reason.to_string());
    out.to_string_compact()
}

fn to_c_string(text: &str) -> *mut c_char {
    match CString::new(text) {
        Ok(value) => value.into_raw(),
        Err(_) => CString::new("{\"error\":\"interior nul byte\"}")
            .map(|v| v.into_raw())
            .unwrap_or(std::ptr::null_mut()),
    }
}

unsafe fn unsafe_str(ptr: *const c_char) -> Result<String, String> {
    if ptr.is_null() {
        return Err("null pointer".to_string());
    }
    CStr::from_ptr(ptr)
        .to_str()
        .map(|s| s.to_string())
        .map_err(|e| e.to_string())
}

fn parse_scope(text: &str) -> Scope {
    let Ok(parsed) = json::parse(text) else {
        return Scope::default();
    };
    let mut scope = Scope::default();
    let type_list = parsed
        .get("cleanTypes")
        .and_then(|v| v.as_arr())
        .or_else(|| parsed.get("categories").and_then(|v| v.as_arr()));
    if let Some(arr) = type_list {
        let types: Vec<_> = arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| crate::rules::model::CleanType::parse(Some(s)))
            .collect();
        if !types.is_empty() {
            scope.clean_types = Some(types);
        }
    }
    if let Some(arr) = parsed.get("items").and_then(|v| v.as_arr()) {
        let keys: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect();
        if !keys.is_empty() {
            scope.item_keys = Some(keys);
        }
    }
    if let Some(opts) = parsed.get("options") {
        scope.options.include_hidden = opts
            .get("hidden")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        scope.options.include_empty_dirs = opts
            .get("emptyFolder")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if let Some(n) = opts.get("sampleLimit").and_then(|v| v.as_i64()) {
            scope.options.sample_limit = n.max(1) as usize;
        }
        if let Some(n) = opts.get("workers").and_then(|v| v.as_i64()) {
            scope.options.workers = Some(n.max(1) as u32);
        }
    }
    scope
}

fn summary_json(rules: &RuleSet) -> String {
    let summary = rules.summary();
    let mut categories = Json::arr();
    for category in &rules.categories {
        let mut entry = Json::obj();
        entry
            .insert("id", category.key())
            .insert("name", category.title.clone())
            .insert("cleanType", category.clean_type.as_str());
        let mut item_list = Json::arr();
        for item in &category.items {
            let mut slot = Json::obj();
            slot.insert("id", item.key())
                .insert("name", item.title.clone())
                .insert("cleanType", item.clean_type.as_str())
                .insert("recommend", item.recommend)
                .insert("paths", item.paths.len() as i64);
            item_list.push(slot);
        }
        entry.insert("items", item_list);
        categories.push(entry);
    }

    let mut counts = Json::obj();
    counts
        .insert("categories", summary.categories as i64)
        .insert("items", summary.items as i64)
        .insert("paths", summary.paths as i64)
        .insert("cautions", summary.cautions as i64);

    let mut out = Json::obj();
    out.insert("version", rules.version.clone())
        .insert("categories", categories)
        .insert("counts", counts);
    let mut rejected = Json::arr();
    for r in &summary.rejected {
        let mut item = Json::obj();
        item.insert("item", r.item.clone())
            .insert("pattern", r.pattern.clone())
            .insert("reason", r.reason.clone());
        rejected.push(item);
    }
    out.insert("rejected", rejected);
    out.insert("rules", rules.to_json());
    out.to_string_compact()
}

fn outcome_json(outcome: &scan::ScanOutcome) -> String {
    let mut items = Json::arr();
    for item in &outcome.items {
        let mut entry = Json::obj();
        entry
            .insert("id", item.id.clone())
            .insert("title", item.title.clone())
            .insert("category", item.category_title.clone())
            .insert("cleanType", item.clean_type.as_str())
            .insert("size", item.size as i64)
            .insert("fileCount", item.file_count as i64)
            .insert("isCaution", item.is_caution)
            .insert("selected", item.selected);
        let mut paths = Json::arr();
        for p in &item.paths {
            paths.push(p.clone());
        }
        entry.insert("paths", paths);
        if let Some(tips) = &item.tips {
            entry.insert("tips", tips.clone());
        }
        items.push(entry);
    }
    let mut skipped = Json::arr();
    for (path, reason) in &outcome.skipped {
        let mut entry = Json::obj();
        entry.insert("path", path.clone()).insert("reason", reason.clone());
        skipped.push(entry);
    }
    let mut out = Json::obj();
    out.insert("items", items)
        .insert("totalSize", outcome.total_size as i64)
        .insert("totalFiles", outcome.total_files as i64)
        .insert("durationMs", outcome.duration_ms as i64)
        .insert("cancelled", outcome.cancelled)
        .insert("skipped", skipped);
    out.to_string_compact()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_round_trips() {
        let ptr = cw_engine_version();
        assert!(!ptr.is_null());
        let text = unsafe { unsafe_str(ptr) }.unwrap();
        assert_eq!(text, crate::ENGINE_VERSION);
        cw_engine_free(ptr);
    }

    #[test]
    fn trash_rejects_relative_and_system_paths() {
        let payload = CString::new("{\"paths\":[\"relative/path\",\"/System\"]}").unwrap();
        let ptr = cw_trash(payload.as_ptr());
        let text = unsafe { unsafe_str(ptr) }.unwrap();
        cw_engine_free(ptr);
        assert!(text.contains("skipped"), "{text}");
        assert!(text.contains("\"trashed\":[]"), "{text}");
    }
}
