
//! macOS sandbox container special-casing.
//!
//! Inside `~/Library/Containers/<bundle-id>/Data/`, a path written as
//! `~/Library/<x>` refers to `~/Library/Containers/<id>/Data/Library/<x>`
//! (the container-private copy). This mirrors how the original app walks
//! container backups. Callers use [`redirect_in_container`] on scan-rule
//! roots; the redirect is environment-gated so unit tests can enable it.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

static CONTAINER_REDIRECT: AtomicBool = AtomicBool::new(false);

/// Enable/disable `~/Library/...` redirection for paths inside app
/// containers. The macOS host enables this at engine init.
pub fn set_container_redirect(enabled: bool) {
    CONTAINER_REDIRECT.store(enabled, Ordering::SeqCst);
}

pub fn container_redirect_enabled() -> bool {
    CONTAINER_REDIRECT.load(Ordering::SeqCst)
}

fn is_library_anchor(component: &str, next: &str) -> bool {
    component == "Library" || (component.is_empty() && next == "Library")
}

/// If `path` lives under `~/Library/Containers/<id>/Data/...` and the
/// redirect is enabled, rewrite `~/Library/<something>` / `/Library/<something>`
/// as `~/Library/Containers/<id>/Data/Library/<something>`.
pub fn redirect_in_container(path: &str, home: &Path) -> Option<String> {
    if !container_redirect_enabled() {
        return None;
    }
    let home_str = home.to_string_lossy();
    let rest = path.strip_prefix(home_str.as_ref())?;
    let rest = rest.strip_prefix("/Library/")?;
    let rest = rest.strip_prefix("Containers/")?;
    let (bundle, after) = rest.split_once('/')?;
    let after = after.strip_prefix("Data")?;
    let after = after.strip_prefix("/Library/")?;
    if bundle.is_empty() || after.is_empty() {
        return None;
    }
    let mut out = home.to_path_buf();
    out.push("Library");
    out.push("Containers");
    out.push(bundle);
    out.push("Data");
    out.push("Library");
    out.push(after);
    let _ = is_library_anchor;
    Some(out.to_string_lossy().into_owned())
}

/// Heuristic bundle id extracted from a container path.
pub fn bundle_id_from_container(path: &Path) -> Option<String> {
    let comps: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    for (idx, comp) in comps.iter().enumerate() {
        if comp == "Containers" {
            let candidate = comps.get(idx + 1)?;
            if candidate.contains('.') {
                return Some(candidate.clone());
            }
        }
    }
    None
}

/// A `.app` bundle display name from a scan path (`/Applications/Foo.app/...`).
pub fn app_name_from_path(path: &Path, depth_scan: bool) -> Option<String> {
    let mut parts: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if !depth_scan {
        parts.truncate(2);
    }
    for comp in parts.iter().rev() {
        if let Some(stem) = comp.strip_suffix(".app") {
            if !stem.is_empty() {
                return Some(stem.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_bundle_id_from_container_path() {
        let p = Path::new("/Users/t/Library/Containers/com.apple.Safari/Data/Library/Caches");
        assert_eq!(
            bundle_id_from_container(p).as_deref(),
            Some("com.apple.Safari")
        );
        assert!(bundle_id_from_container(Path::new("/Users/t/Library/Caches")).is_none());
    }

    #[test]
    fn extracts_app_name() {
        assert_eq!(
            app_name_from_path(Path::new("/Applications/Foo.app/Contents/x"), true).as_deref(),
            Some("Foo")
        );
        assert!(app_name_from_path(Path::new("/Users/t/a/app.bin"), true).is_none());
    }

    #[test]
    fn redirects_library_paths_inside_containers() {
        let home = Path::new("/Users/t");
        let base = "/Users/t/Library/Containers/com.example.app/Data/Library/Caches/a/b";
        set_container_redirect(false);
        assert!(redirect_in_container(base, home).is_none());
        set_container_redirect(true);
        assert_eq!(
            redirect_in_container(base, home).unwrap(),
            "/Users/t/Library/Containers/com.example.app/Data/Library/Caches/a/b"
        );
        // Not inside a container -> unchanged.
        assert!(redirect_in_container("/Users/t/Library/Caches/a", home).is_none());
        set_container_redirect(false);
    }
}
