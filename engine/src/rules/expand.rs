//! Path macro expansion for rule value strings.
//!
//! Macros (confirmed from the original app's string table):
//! * tilde prefix      -> the caller's home directory
//! * SPACE (a whole path segment) -> application name (e.g. WeChat, Xcode)
//!
//! Also tolerated: @visit-HOME forms and the binary's own %@ format token.
//! Special form: <Xcode:MARKETING_VERSION> resolves from MacroContext::values.

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum ExpandError {
    UnknownMacro(String),
}

impl std::fmt::Display for ExpandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExpandError::UnknownMacro(m) => write!(f, "unknown macro '{}'", m),
        }
    }
}

/// Values substituted into a rule path.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MacroContext {
    pub home: PathBuf,
    /// Application name used to substitute the SPACE segment macro.
    pub app_name: Option<String>,
    /// Extra named macros, e.g. MARKETING_VERSION.
    pub values: Vec<(String, String)>,
}

impl MacroContext {
    pub fn new(home: impl Into<PathBuf>) -> Self {
        MacroContext {
            home: home.into(),
            app_name: None,
            values: Vec::new(),
        }
    }

    pub fn with_app(mut self, name: &str) -> Self {
        self.app_name = Some(name.to_string());
        self
    }

    pub fn with_value(mut self, name: &str, value: &str) -> Self {
        self.values.push((name.to_string(), value.to_string()));
        self
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn segments(path: &str) -> Vec<&str> {
    path.split(['/', '\\']).collect()
}

fn is_angle_macro(seg: &str) -> bool {
    match seg.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
        Some(inner) => !inner.is_empty(),
        None => false,
    }
}

fn looks_like_app_macro(seg: &str) -> bool {
    matches!(
        seg,
        "SPACE" | "space" | "Space" | "%@" | "@@APP@@" | "@@SPACE@@"
    )
}

fn home_forms() -> [&'static str; 3] {
    ["$HOME", "${HOME}", "@@HOME@@"]
}

/// Expand macros in `pattern`. Segments equal to SPACE (or the binary's %@)
/// are replaced by [MacroContext::app_name]; if the app name is unknown this
/// is an error so callers can group such rules per application.
pub fn expand(pattern: &str, ctx: &MacroContext) -> Result<String, ExpandError> {
    let raw = pattern.trim();
    if raw.is_empty() {
        return Ok(String::new());
    }

    let mut out: Vec<String> = Vec::new();
    // A home-macro-led path ("~/...", "$HOME/...") expands to an absolute
    // path, so it always gets the leading root marker.
    let mut absolute = raw.starts_with('/');
    for (idx, seg) in segments(raw).into_iter().enumerate() {
        if idx == 0 && seg.is_empty() {
            out.push(String::new());
            continue;
        }
        if looks_like_app_macro(seg) {
            match &ctx.app_name {
                Some(name) if !name.is_empty() => out.push(name.clone()),
                _ => return Err(ExpandError::UnknownMacro(seg.to_string())),
            }
            continue;
        }
        if is_angle_macro(seg) {
            let inner = seg.trim_start_matches('<').trim_end_matches('>');
            match resolve_angle(inner, ctx) {
                Some(v) => out.push(v),
                None => return Err(ExpandError::UnknownMacro(seg.to_string())),
            }
            continue;
        }
        let home_form = home_forms()
            .iter()
            .find(|f| seg.starts_with(**f))
            .copied();
        if let Some(form) = home_form {
            absolute = true;
            let rest = &seg[form.len()..];
            for part in segments(&ctx.home.to_string_lossy()) {
                if !part.is_empty() {
                    out.push(part.to_string());
                }
            }
            if !rest.is_empty() {
                out.push(rest.trim_start_matches('/').to_string());
            }
            continue;
        }
        if idx == 0 && seg.starts_with('~') {
            absolute = true;
            let rest = seg.strip_prefix('~').unwrap_or(seg);
            let rest = rest.strip_prefix('/').unwrap_or(rest);
            for part in segments(&ctx.home.to_string_lossy()) {
                if !part.is_empty() {
                    out.push(part.to_string());
                }
            }
            if !rest.is_empty() {
                out.push(rest.to_string());
            }
            continue;
        }
        out.push(seg.to_string());
    }

    // Drop trailing empty segments ("~/Library/Caches/" -> ".../Caches").
    while out.len() > 1 && out.last().map(|s| s.is_empty()).unwrap_or(false) {
        out.pop();
    }
    let rel = out.iter().filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("/");
    Ok(if absolute { format!("/{}", rel) } else { rel })
}

fn resolve_angle(inner: &str, ctx: &MacroContext) -> Option<String> {
    // "<Xcode:MARKETING_VERSION>" -> app name "Xcode", key "MARKETING_VERSION".
    let (maybe_app, key) = match inner.split_once(':') {
        Some((a, k)) => (Some(a), k),
        None => (None, inner),
    };
    if let Some(value) = ctx.get(key) {
        return Some(value.to_string());
    }
    match maybe_app {
        Some(app) if app == "Xcode" => ctx.get("XcodeVersion").map(|s| s.to_string()),
        _ => None,
    }
}

/// Convenience: expand a home-relative rule path. Unresolvable macros fall
/// back to the raw pattern.
pub fn expand_home_pattern(pattern: &str, home: &std::path::Path) -> String {
    let ctx = MacroContext::new(home);
    expand(pattern, &ctx).unwrap_or_else(|_| pattern.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn ctx() -> MacroContext {
        MacroContext::new("/Users/tester").with_app("WeChat")
    }

    #[test]
    fn expands_tilde_prefix() {
        let c = ctx();
        assert_eq!(
            expand("~/Library/Caches", &c).unwrap(),
            "/Users/tester/Library/Caches"
        );
        assert_eq!(expand("~", &c).unwrap(), "/Users/tester");
        assert_eq!(
            expand("~/Library/Application Support/", &c).unwrap(),
            "/Users/tester/Library/Application Support"
        );
        assert_eq!(expand("~/a/b/c", &c).unwrap(), "/Users/tester/a/b/c");
    }

    #[test]
    fn expands_space_macro_as_directory() {
        let c = ctx();
        assert_eq!(
            expand("~/Library/Containers/SPACE/Data/Library/Caches", &c).unwrap(),
            "/Users/tester/Library/Containers/WeChat/Data/Library/Caches"
        );
        assert_eq!(
            expand("~/Library/Application Support/SPACE/Caches", &c).unwrap(),
            "/Users/tester/Library/Application Support/WeChat/Caches"
        );
        // The binary's own %@ placeholder behaves like SPACE.
        assert_eq!(expand("/Applications/%@", &c).unwrap(), "/Applications/WeChat");
    }

    #[test]
    fn space_macro_requires_app_name() {
        let c = MacroContext::new("/Users/tester");
        let err = expand("~/Library/Logs/SPACE", &c).unwrap_err();
        assert_eq!(err, ExpandError::UnknownMacro("SPACE".into()));
        // Names that merely *contain* SPACE are untouched.
        assert_eq!(
            expand("~/Library/Logs/SPACESHIPS", &c).unwrap(),
            "/Users/tester/Library/Logs/SPACESHIPS"
        );
    }

    #[test]
    fn absolute_paths_and_leading_slash_are_preserved() {
        let c = ctx();
        assert_eq!(
            expand("/Library/Developer/Xcode/DerivedData/", &c).unwrap(),
            "/Library/Developer/Xcode/DerivedData"
        );
        assert_eq!(expand("/Library/Caches/", &c).unwrap(), "/Library/Caches");
    }

    #[test]
    fn home_variable_forms() {
        let c = ctx();
        assert_eq!(
            expand("$HOME/Library/Logs", &c).unwrap(),
            "/Users/tester/Library/Logs"
        );
        assert_eq!(
            expand("${HOME}/Library/Logs", &c).unwrap(),
            "/Users/tester/Library/Logs"
        );
        assert_eq!(expand("@@HOME@@/Library", &c).unwrap(), "/Users/tester/Library");
        assert_eq!(expand("$HOME", &c).unwrap(), "/Users/tester");
    }

    #[test]
    fn angle_macros_resolve_from_context() {
        let c = MacroContext::new("/Users/tester").with_value("MARKETING_VERSION", "16.0");
        assert_eq!(
            expand("~/Library/Developer/Xcode/<Xcode:MARKETING_VERSION>", &c).unwrap(),
            "/Users/tester/Library/Developer/Xcode/16.0"
        );
        let missing = MacroContext::new("/Users/tester");
        assert!(expand("~/x/<Missing:FOO>", &missing).is_err());
    }

    #[test]
    fn helper_expands_home_patterns() {
        assert_eq!(
            expand_home_pattern("~/Library/Caches/SPACE", Path::new("/Users/tester")),
            "~/Library/Caches/SPACE",
            "unresolvable macros fall back to the raw pattern"
        );
        assert_eq!(
            expand_home_pattern("~/Library/Caches", Path::new("/Users/tester")),
            "/Users/tester/Library/Caches"
        );
    }
}
