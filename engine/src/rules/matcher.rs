//! Filter matching: evaluate rule filters against a filesystem candidate.
//!
//! A candidate is one file or directory that a scan root discovered. Path
//! segments can be tested individually via [Target::PathHash], mirroring the
//! original app's filter model.
//!
//! Combination rules:
//! * Leaves inside one \`<filters>\` group combine with that group's relation;
//! * nested \`<filters>\` groups evaluate first and are then combined;
//! * when a leaf's own relation differs from its group's, the first leaf
//!   seeds the accumulator and following leaves apply their own relation.
//!
//! *Unsupported columns* (\`signed\` — codesign inspection — and \`app\` —
//! installed-application state) evaluate to the group relation's neutral
//! value so they never flip a decision, and are counted in
//! [MatchStats::unsupported] so callers can surface engine coverage.

use super::expand::{self, ExpandError, MacroContext};
use super::model::{Column, Filter, FilterGroup, Relation, RuleCategory, ScanPath, SubCategory};
use crate::support::fsutil::normalize_path;
use std::path::{Path, PathBuf};

/// How a filter evaluates a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Full normalized path string, with "/" as separator.
    PathHash,
    /// Last path component only.
    LastComponent,
}

/// One evaluable scan root produced from a rule \`<path>\`.
#[derive(Debug, Clone, PartialEq)]
pub struct PathSpec {
    pub category_key: String,
    pub category_title: String,
    pub item_key: String,
    pub clean_type: super::model::CleanType,
    /// Expanded absolute pattern (before container redirection).
    pub path: PathBuf,
    pub level: u32,
    pub filename: Option<String>,
    /// Filters attached to this \`<path>\`.
    pub filters: Vec<FilterGroup>,
    /// Document-level \`<filters>\` (global excludes / whitelist).
    pub global_filters: Vec<FilterGroup>,
    /// Select-able by default (from \`defaultState\`).
    pub default_selected: bool,
}

impl PathSpec {
    pub fn from_scan_path(
        category: &RuleCategory,
        item: &SubCategory,
        path: &ScanPath,
        global_filters: &[FilterGroup],
    ) -> Result<PathSpec, String> {
        let home = crate::support::fsutil::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let ctx = MacroContext::new(home);
        let expanded = expand::expand(&path.value, &ctx).map_err(|e: ExpandError| e.to_string())?;
        if expanded.is_empty() {
            return Err("empty path value".to_string());
        }
        Ok(PathSpec {
            category_key: category.key(),
            category_title: category.title.clone(),
            item_key: item.key(),
            clean_type: item.clean_type,
            path: PathBuf::from(normalize_path(&expanded, Path::new("/"))),
            level: path.level,
            filename: path.filename.clone(),
            filters: path.filters.clone(),
            global_filters: global_filters.to_vec(),
            default_selected: item.default_state,
        })
    }

    /// Expand a path with an explicit macro context (tests, SPACE rules).
    pub fn from_pattern(
        category: &RuleCategory,
        item: &SubCategory,
        pattern: &str,
        ctx: &MacroContext,
    ) -> Result<PathSpec, ExpandError> {
        let expanded = expand::expand(pattern, ctx)?;
        let base: PathBuf = ctx.home.clone();
        Ok(PathSpec {
            category_key: category.key(),
            category_title: category.title.clone(),
            item_key: item.key(),
            clean_type: item.clean_type,
            path: PathBuf::from(normalize_path(&expanded, base.as_path())),
            level: 1,
            filename: None,
            filters: Vec::new(),
            global_filters: Vec::new(),
            default_selected: item.default_state,
        })
    }
}

/// Attributes of a filesystem candidate. Injected so filters are testable
/// without touching disk.
pub trait FileInfo {
    fn is_dir(&self) -> bool;
    fn size(&self) -> u64;
    /// Immediate child count (files + dirs).
    fn children(&self) -> u64;
    fn is_symlink(&self) -> bool;
    fn is_package(&self) -> bool;
    fn modified_secs(&self) -> i64;
}

/// Concrete metadata snapshot used by the scanner.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileMeta {
    pub is_dir: bool,
    pub size: u64,
    pub children: u64,
    pub is_symlink: bool,
    pub is_package: bool,
    pub modified_secs: i64,
}

impl FileMeta {
    pub fn file(size: u64) -> FileMeta {
        FileMeta {
            size,
            ..Default::default()
        }
    }
    pub fn dir(children: u64) -> FileMeta {
        FileMeta {
            is_dir: true,
            children,
            ..Default::default()
        }
    }
}

impl FileInfo for FileMeta {
    fn is_dir(&self) -> bool {
        self.is_dir
    }
    fn size(&self) -> u64 {
        self.size
    }
    fn children(&self) -> u64 {
        self.children
    }
    fn is_symlink(&self) -> bool {
        self.is_symlink
    }
    fn is_package(&self) -> bool {
        self.is_package
    }
    fn modified_secs(&self) -> i64 {
        self.modified_secs
    }
}

/// A scan candidate (a file, or a directory).
pub struct Candidate<'a> {
    /// Normalized path string used for matching.
    pub path: String,
    pub meta: &'a dyn FileInfo,
    pub target: Target,
    /// Bundle id of the owning app (container name), used by \`bundleid\`.
    pub bundle_id: Option<&'a str>,
}

impl<'a> Candidate<'a> {
    pub fn new(path: &str, meta: &'a dyn FileInfo) -> Candidate<'a> {
        Candidate {
            path: normalize_path(path, Path::new("/")),
            meta,
            target: Target::PathHash,
            bundle_id: None,
        }
    }
    pub fn with_last_component(mut self) -> Self {
        self.target = Target::LastComponent;
        self
    }
    pub fn with_bundle_id(mut self, id: Option<&'a str>) -> Self {
        self.bundle_id = id;
        self
    }
}

/// Result of evaluating a filter tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Include,
    /// The rule explicitly matched an exclusion — never include.
    Exclude,
    /// A size threshold (greater/smaller/filesize) was not met; the
    /// candidate simply does not qualify (not an exclusion).
    Interrupt,
}

impl Decision {
    fn combine(self, other: Decision, relation: Relation) -> Decision {
        match relation {
            Relation::And => {
                if self == Decision::Interrupt || other == Decision::Interrupt {
                    Decision::Interrupt
                } else if self == Decision::Exclude || other == Decision::Exclude {
                    Decision::Exclude
                } else {
                    Decision::Include
                }
            }
            Relation::Or => {
                if self == Decision::Include || other == Decision::Include {
                    Decision::Include
                } else if self == Decision::Exclude || other == Decision::Exclude {
                    Decision::Exclude
                } else {
                    Decision::Interrupt
                }
            }
        }
    }
}

/// Coverage counters for unsupported filter columns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatchStats {
    pub signed: usize,
    pub app: usize,
}

impl MatchStats {
    pub fn total(&self) -> usize {
        self.signed + self.app
    }
}

/// Per-source-candidate context for nested \`subfilepath\` / \`subfilecount\`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FilterContext {
    pub inside_dir: bool,
    pub parent_children: u64,
    pub inside_app: bool,
}

pub struct Matcher {
    pub now_secs: i64,
    pub stats: MatchStats,
}

impl Default for Matcher {
    fn default() -> Self {
        Matcher::new()
    }
}

impl Matcher {
    pub fn new() -> Self {
        Matcher {
            now_secs: 0,
            stats: MatchStats::default(),
        }
    }

    /// Evaluate a sequence of filter groups (any match => Include; any
    /// Exclude => Exclude). Empty sequence => Include.
    pub fn decide(&mut self, c: &Candidate<'_>, groups: &[FilterGroup]) -> Decision {
        if groups.is_empty() {
            return Decision::Include;
        }
        let ctx = FilterContext::default();
        let mut result = groups[0].relation.neutral();
        for group in groups {
            result = result.combine(self.eval_group(c, group, ctx), group.relation);
        }
        result
    }

    /// Evaluate a single group.
    pub fn eval_group(
        &mut self,
        c: &Candidate<'_>,
        group: &FilterGroup,
        ctx: FilterContext,
    ) -> Decision {
        let mut leaves: Vec<Decision> = Vec::new();
        for nested in &group.groups {
            leaves.push(self.eval_group(c, nested, ctx));
        }
        for filter in &group.filters {
            leaves.push(self.eval_filter(c, filter, ctx));
        }
        if leaves.is_empty() {
            return group.relation.neutral();
        }
        // Left fold: the first leaf seeds the accumulator, subsequent leaves
        // apply their own relation (falling back to the group relation).
        let mut acc: Option<Decision> = None;
        for (idx, leaf) in leaves.iter().enumerate() {
            let relation = if idx == 0 {
                group.relation
            } else {
                group
                    .filters
                    .get(idx.saturating_sub(group.groups.len()))
                    .map(|f| f.relation)
                    .unwrap_or(group.relation)
            };
            acc = Some(match acc {
                None => *leaf,
                Some(prev) => prev.combine(*leaf, relation),
            });
        }
        acc.unwrap_or_else(|| group.relation.neutral())
    }

    pub fn eval_filter(
        &mut self,
        c: &Candidate<'_>,
        filter: &Filter,
        ctx: FilterContext,
    ) -> Decision {
        let coarse = self.eval_filter_coarse(c, filter, ctx);
        match coarse {
            Coarse::Soft(false) => Decision::Interrupt,
            Coarse::Soft(true) => Decision::Include,
            Coarse::Exclude(b) => {
                if b {
                    Decision::Exclude
                } else {
                    filter.relation.neutral()
                }
            }
        }
    }

    /// Three-valued evaluation: a size condition that does not hold is
    /// *soft-false* (candidate doesn't qualify) rather than an explicit
    /// exclusion, so OR groups can pick another branch.
    fn eval_filter_coarse(
        &mut self,
        c: &Candidate<'_>,
        filter: &Filter,
        ctx: FilterContext,
    ) -> Coarse {
        let value = filter.value.trim();
        let neutral = Coarse::Soft(filter.relation == Relation::And);
        let column = match &filter.column {
            Some(col) => col.clone(),
            None => return neutral,
        };
        match column {
            Column::Filename | Column::Contains | Column::Match | Column::BeginWith
            | Column::EndWith => Coarse::Soft(self.name_matches(c, &column, filter, value)),
            Column::Filepath | Column::SubFilePath => Coarse::Soft(self.path_matches(c, value)),
            Column::BundleId => Coarse::Soft(self.bundle_matches(c, value)),
            Column::FileSize | Column::Greater | Column::Smaller => {
                let target = parse_size(value);
                let size = c.meta.size();
                match target {
                    Some(t) => {
                        let ok = match column {
                            Column::FileSize => size > 0 && size >= t,
                            Column::Greater => size > t,
                            Column::Smaller => size < t,
                            _ => false,
                        };
                        Coarse::Soft(ok)
                    }
                    // Bare "greater"/"smaller" without a number compares to
                    // nothing -> neutral.
                    None => neutral,
                }
            }
            Column::SubFileCount => Coarse::Soft(c.meta.children() >= parse_count(value)),
            Column::Exclude => {
                Coarse::Exclude(
                    self.name_matches(c, &Column::Contains, filter, value)
                        || self.path_matches(c, value),
                )
            }
            Column::Time => {
                let days = value.parse::<i64>().unwrap_or(0);
                if days == 0 || self.now_secs == 0 {
                    neutral
                } else {
                    let age = self.now_secs.saturating_sub(c.meta.modified_secs());
                    Coarse::Soft(age >= days * 86_400)
                }
            }
            Column::Is => Coarse::Soft(is_kind(value, c)),
            Column::Sub => Coarse::Soft(ctx.inside_dir),
            Column::App => {
                self.stats.app += 1;
                neutral
            }
            Column::Signed => {
                self.stats.signed += 1;
                neutral
            }
        }
    }

    fn name_matches(&self, c: &Candidate<'_>, column: &Column, filter: &Filter, value: &str) -> bool {
        let name = c
            .path
            .rsplit('/')
            .next()
            .unwrap_or(c.path.as_str())
            .to_string();
        // allow subfile patterns like "*.plist" / ".*"
        let _ = filter;
        match column {
            Column::Match => regex_match(value, &name),
            Column::BeginWith => name.starts_with(value) || wildcard_prefix(value, &name),
            Column::EndWith => name.ends_with(value) || wildcard_suffix(value, &name),
            Column::Contains | Column::Filename => {
                name.contains(value)
                    || name.to_ascii_lowercase().contains(&value.to_ascii_lowercase())
                    || is_glob(value) && glob_match(value, &name)
            }
            _ => false,
        }
    }

    fn path_matches(&self, c: &Candidate<'_>, value: &str) -> bool {
        if value.is_empty() {
            return false;
        }
        let target = match c.target {
            Target::PathHash => c.path.as_str(),
            Target::LastComponent => c.path.rsplit('/').next().unwrap_or(c.path.as_str()),
        };
        target.contains(value)
            || (is_glob(value) && glob_match(value, target))
    }

    fn bundle_matches(&self, c: &Candidate<'_>, value: &str) -> bool {
        if value.is_empty() || value == "*" {
            return value == "*";
        }
        if let Some(id) = c.bundle_id {
            if glob_match(value, id) || id == value {
                return true;
            }
        }
        is_glob(value) && glob_match(value, &c.path)
    }
}

/// Internal three-valued leaf result.
enum Coarse {
    /// Ordinary predicate: true => Include, false => Interrupt.
    Soft(bool),
    /// Explicit `exclude` keyword: true => hard Exclude, false => neutral.
    Exclude(bool),
}

impl Relation {
    /// AND is neutral-true, OR is neutral-false.
    fn neutral(self) -> Decision {
        match self {
            Relation::And => Decision::Include,
            Relation::Or => Decision::Interrupt,
        }
    }
}

/// Parse a human size string: "1 MB", "50mb", "1.5g", "1024", "10 KB".
/// Returns bytes. Decimal SI (1000) and binary (1024) are both honoured by
/// unit spelling.
pub fn parse_size(raw: &str) -> Option<u64> {
    if raw.is_empty() {
        return None;
    }
    let cleaned: String = raw.trim().to_ascii_lowercase();
    let digits: String = cleaned
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if digits.is_empty() {
        return None;
    }
    let number: f64 = digits.parse().ok()?;
    if !number.is_finite() || number < 0.0 {
        return None;
    }
    let unit: String = cleaned[digits.len()..].trim().to_string();
    let multiplier: f64 = match unit.as_str() {
        "" | "b" | "byte" | "bytes" => 1.0,
        "k" | "kb" | "kib" => 1024.0,
        "m" | "mb" | "mib" => 1024.0 * 1024.0,
        "g" | "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" | "tib" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((number * multiplier) as u64)
}

fn parse_count(raw: &str) -> u64 {
    raw.trim()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or(0)
}

fn is_kind(value: &str, c: &Candidate<'_>) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "file" => !c.meta.is_dir(),
        "dir" | "directory" | "folder" => c.meta.is_dir() && !c.meta.is_package(),
        "package" | "bundle" | "app" => c.meta.is_package(),
        "symlink" | "link" => c.meta.is_symlink(),
        _ => false,
    }
}

fn is_glob(value: &str) -> bool {
    value.contains('*') || value.contains('?') || value.contains('[')
}

fn wildcard_prefix(value: &str, name: &str) -> bool {
    let p = value.trim_end_matches('*');
    !p.is_empty() && name.starts_with(p)
}

fn wildcard_suffix(value: &str, name: &str) -> bool {
    let s = value.trim_start_matches('*');
    !s.is_empty() && name.ends_with(s)
}

/// Glob with \`*\` (any run, may be empty) and \`?\` (one char).
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = pi;
            mark = ti;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Tiny subset regex for the rule \`match\` column: supports literals, \`.\`,
/// \`.*\`, \`\\d+\`, anchors \`^\`/\`$\`, alternation, character classes and
/// case-insensitive \`(?i)\` prefix. This covers every pattern shape seen in
/// the original rule packs, without pulling a regex engine crate.
pub fn regex_match(pattern: &str, text: &str) -> bool {
    let mut pat = pattern.to_string();
    let mut text = text.to_string();
    if let Some(stripped) = pat.strip_prefix("(?i)") {
        pat = stripped.to_ascii_lowercase();
        text = text.to_ascii_lowercase();
    }
    let anchored_start = pat.starts_with('^');
    let anchored_end = pat.ends_with('$');
    let core = pat
        .trim_start_matches('^')
        .trim_end_matches('$')
        .to_string();
    let classes: Vec<String> = core
        .split('|')
        .map(|s| s.to_string())
        .collect();
    for class in classes {
        if anchored_start && anchored_end {
            if glob_like_match(&class, &text) {
                return true;
            }
        } else if anchored_start {
            if let Some(prefix) = prefix_probe(&class) {
                if text.starts_with(&prefix) || glob_like_prefix(&class, &text) {
                    return true;
                }
            }
        } else if anchored_end {
            let suffix = suffix_probe(&class);
            if !suffix.is_empty() && text.ends_with(&suffix) {
                return true;
            }
        } else if glob_like_contains(&class, &text) {
            return true;
        }
    }
    false
}

fn prefix_probe(class: &str) -> Option<String> {
    let mut out = String::new();
    for ch in class.chars() {
        match ch {
            '.' | '*' | '+' | '?' | '[' | ']' | '(' | ')' | '\\' => break,
            c => out.push(c),
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn suffix_probe(class: &str) -> String {
    let mut out: String = class
        .chars()
        .rev()
        .take_while(|c| !matches!(c, '.' | '*' | '+' | '?' | '[' | ']' | '(' | ')' | '\\'))
        .collect();
    out = out.chars().rev().collect();
    out
}

/// Convert \`.*\` wildcards around literals into a contains test.
fn glob_like_contains(class: &str, text: &str) -> bool {
    let lit = strip_regex_sugar(class);
    if lit.is_empty() {
        return false;
    }
    text.contains(&lit)
}

fn glob_like_prefix(class: &str, text: &str) -> bool {
    let lit = strip_regex_sugar(class);
    text.starts_with(&lit)
}

fn glob_like_match(class: &str, text: &str) -> bool {
    let glob = to_glob(class);
    glob_match(&glob, text)
}

/// Turn a light regex into a glob: \`.*\` -> \`*\`, \`.\` -> \`?\`.
fn to_glob(class: &str) -> String {
    let mut out = String::new();
    let mut chars = class.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '.' => {
                if chars.peek() == Some(&'*') {
                    chars.next();
                    out.push('*');
                } else {
                    out.push('?');
                }
            }
            '\\' => {
                if let Some(next) = chars.next() {
                    if next == 'd' {
                        out.push('*');
                    } else {
                        out.push(next);
                    }
                }
            }
            '*' | '+' | '?' | '[' | ']' | '(' | ')' => out.push('*'),
            c => out.push(c),
        }
    }
    out
}

/// Strip regex sugar from a pattern, keeping the literal text.
fn strip_regex_sugar(class: &str) -> String {
    let mut out = String::new();
    let mut chars = class.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '.' | '*' | '+' | '?' => {}
            '\\' => {
                if let Some(next) = chars.next() {
                    if next != 'd' {
                        out.push(next);
                    }
                }
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::model::{CleanType, FilterGroup, Relation, RuleCategory, SubCategory};

    fn meta() -> FileMeta {
        FileMeta::dir(5)
    }

    fn cand<'a>(path: &'a str, m: &'a FileMeta) -> Candidate<'a> {
        Candidate::new(path, m)
    }

    fn f(column: &str, relation: Relation, value: &str) -> Filter {
        Filter::new(column, relation, value)
    }

    fn group(filters: Vec<Filter>, relation: Relation) -> FilterGroup {
        FilterGroup {
            level: 1,
            relation,
            filters,
            groups: Vec::new(),
        }
    }

    #[test]
    fn size_strings_parse() {
        assert_eq!(parse_size("1 MB"), Some(1 << 20));
        assert_eq!(parse_size("50mb"), Some(50 << 20));
        assert_eq!(parse_size("1.5 g"), Some((1.5 * (1u64 << 30) as f64) as u64));
        assert_eq!(parse_size("1024"), Some(1024));
        assert_eq!(parse_size("10 KB"), Some(10 * 1024));
        assert_eq!(parse_size("nonsense"), None);
        assert_eq!(parse_size(""), None);
    }

    #[test]
    fn filename_matching_keywords() {
        let m = meta();
        let c = cand("/Users/t/Library/Caches/foo.log", &m);
        let mut matcher = Matcher::new();
        assert_eq!(
            matcher.eval_filter(&c, &f("filename", Relation::And, "log"), FilterContext::default()),
            Decision::Include
        );
        assert_eq!(
            matcher.eval_filter(&c, &f("begin with", Relation::And, "foo"), FilterContext::default()),
            Decision::Include
        );
        assert_eq!(
            matcher.eval_filter(&c, &f("end with", Relation::And, ".log"), FilterContext::default()),
            Decision::Include
        );
        assert_eq!(
            matcher.eval_filter(
                &c,
                &f("match", Relation::And, "*.log"),
                FilterContext::default()
            ),
            Decision::Include
        );
        assert_eq!(
            matcher.eval_filter(&c, &f("filename", Relation::And, ".txt"), FilterContext::default()),
            Decision::Interrupt
        );
    }

    #[test]
    fn filepath_and_bundle_id() {
        let m = meta();
        let c = cand("/Users/t/Library/Caches/com.apple.Safari/x", &m).with_bundle_id(Some("com.apple.Safari"));
        let mut matcher = Matcher::new();
        assert_eq!(
            matcher.eval_filter(&c, &f("filepath", Relation::And, "/Caches/"), FilterContext::default()),
            Decision::Include
        );
        assert_eq!(
            matcher.eval_filter(&c, &f("bundleid", Relation::And, "com.apple.*"), FilterContext::default()),
            Decision::Include
        );
        assert_eq!(
            matcher.eval_filter(&c, &f("bundleid", Relation::And, "com.google.*"), FilterContext::default()),
            Decision::Interrupt
        );
    }

    #[test]
    fn filesize_thresholds_are_interrupts_not_excludes() {
        let big = FileMeta::file(5 << 20);
        let small = FileMeta::file(100);
        let mut matcher = Matcher::new();
        let c = cand("/tmp/a.bin", &big);
        assert_eq!(
            matcher.eval_filter(&c, &f("filesize", Relation::And, "1 MB"), FilterContext::default()),
            Decision::Include
        );
        assert_eq!(
            matcher.eval_filter(&c, &f("greater", Relation::And, "10 MB"), FilterContext::default()),
            Decision::Interrupt
        );
        let cs = cand("/tmp/b.bin", &small);
        assert_eq!(
            matcher.eval_filter(&cs, &f("smaller", Relation::And, "1 MB"), FilterContext::default()),
            Decision::Include
        );
    }

    #[test]
    fn exclude_returns_exclude() {
        let m = meta();
        let c = cand("/Users/t/Library/Caches/History.db", &m);
        let mut matcher = Matcher::new();
        assert_eq!(
            matcher.eval_filter(&c, &f("exclude", Relation::And, "History.db"), FilterContext::default()),
            Decision::Exclude
        );
        let ok = cand("/Users/t/Library/Caches/Cache.db", &m);
        assert_ne!(
            matcher.eval_filter(&ok, &f("exclude", Relation::And, "History.db"), FilterContext::default()),
            Decision::Exclude
        );
    }

    #[test]
    fn and_or_semantics() {
        let big = FileMeta::file(5 << 20);
        let c = cand("/tmp/a.log", &big);
        let mut matcher = Matcher::new();
        let g = group(
            vec![
                f("filename", Relation::And, "log"),
                f("filesize", Relation::And, "1 MB"),
            ],
            Relation::And,
        );
        assert_eq!(matcher.decide(&c, std::slice::from_ref(&g)), Decision::Include);
        let g2 = group(
            vec![
                f("filename", Relation::And, "txt"),
                f("filesize", Relation::And, "1 MB"),
            ],
            Relation::And,
        );
        assert_eq!(matcher.decide(&c, &[g2]), Decision::Interrupt);
        // An OR group whose leaves carry the group's `or` relation.
        let g3 = group(
            vec![
                f("filename", Relation::Or, "txt"),
                f("filesize", Relation::Or, "1 MB"),
            ],
            Relation::Or,
        );
        assert_eq!(matcher.decide(&c, &[g3]), Decision::Include);
        // Explicitly-ANDed leaves inside an OR group keep AND semantics.
        let g4 = group(
            vec![
                f("filename", Relation::And, "txt"),
                f("filesize", Relation::And, "1 MB"),
            ],
            Relation::Or,
        );
        assert_eq!(matcher.decide(&c, &[g4]), Decision::Interrupt);
    }

    #[test]
    fn nested_groups_evaluate_first() {
        let big = FileMeta::file(5 << 20);
        let c = cand("/tmp/a.log", &big);
        let mut matcher = Matcher::new();
        let inner = group(
            vec![f("filename", Relation::Or, "txt"), f("filesize", Relation::Or, "1 MB")],
            Relation::Or,
        );
        let outer = FilterGroup {
            level: 2,
            relation: Relation::And,
            filters: vec![f("filepath", Relation::And, "/tmp/")],
            groups: vec![inner],
        };
        assert_eq!(matcher.decide(&c, &[outer]), Decision::Include);
    }

    #[test]
    fn unsupported_columns_are_counted_and_neutral() {
        let m = meta();
        let c = cand("/tmp/x", &m);
        let mut matcher = Matcher::new();
        let d = matcher.eval_filter(&c, &f("signed", Relation::And, "NO"), FilterContext::default());
        assert_eq!(d, Decision::Include, "AND neutral keeps the group open");
        matcher.eval_filter(&c, &f("app", Relation::Or, "WeChat"), FilterContext::default());
        assert_eq!(matcher.stats.signed, 1);
        assert_eq!(matcher.stats.app, 1);
    }

    #[test]
    fn sub_fields_and_globs() {
        let dir = FileMeta::dir(10);
        let mut matcher = Matcher::new();
        let c = cand("/tmp/p", &dir);
        assert_eq!(
            matcher.eval_filter(&c, &f("subfilecount", Relation::And, "5"), FilterContext::default()),
            Decision::Include
        );
        assert_eq!(
            matcher.eval_filter(&c, &f("subfilecount", Relation::And, "50"), FilterContext::default()),
            Decision::Interrupt
        );
        assert!(glob_match("*.plist", "Info.plist"));
        assert!(!glob_match("*.plist", "Info.plist.bak"));
        assert!(glob_match("/a/*/c", "/a/b/c"));
        assert!(glob_match("*", ""));
    }

    #[test]
    fn path_specs_skip_space_rules() {
        let cat = RuleCategory {
            title: "t".into(),
            tips: None,
            clean_type: CleanType::Sys,
            items: vec![],
        };
        let item = SubCategory {
            title: "i".into(),
            item_type: None,
            clean_type: CleanType::Sys,
            recommend: true,
            fast_mode: false,
            show_result: true,
            default_state: true,
            is_cautious: false,
            tips: None,
            bundle_id: None,
            app_store_bundle_id: None,
            paths: vec![],
        };
        let ctx = MacroContext::new("/Users/tester").with_app("WeChat");
        let spec = PathSpec::from_pattern(&cat, &item, "~/Library/Caches/SPACE", &ctx).unwrap();
        assert!(spec.path.to_string_lossy().contains("WeChat"));
    }
}
