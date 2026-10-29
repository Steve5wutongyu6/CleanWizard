//! Rule model: a faithful port of the original app's QM* model classes.
//!
//! | Lemon model            | here                          |
//! |------------------------|-------------------------------|
//! | `QMCategoryItem`        | [`RuleCategory`]              |
//! | `QMCategorySubItem`     | [`SubCategory`]               |
//! | `QMActionPathItem`      | [`ScanPath`]                  |
//! | `QMFilterItem`          | [`FilterGroup`] / [`Filter`]   |
//! | `QMCautionItem`         | [`Caution`]                   |

use crate::support::json::{FromValue, Json, ToValue};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// `cleanType` attribute: which cleanup module the item belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CleanType {
    /// `sys` — 系统垃圾 (system junk)
    Sys,
    /// `app` — 应用垃圾 (application junk)
    App,
    /// `int` — 上网垃圾 (internet / browser junk)
    Int,
}

impl CleanType {
    pub fn parse(s: Option<&str>) -> CleanType {
        match s.map(|x| x.trim().to_ascii_lowercase()).as_deref() {
            Some("app") => CleanType::App,
            Some("int") => CleanType::Int,
            _ => CleanType::Sys,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            CleanType::Sys => "sys",
            CleanType::App => "app",
            CleanType::Int => "int",
        }
    }
}

impl ToValue for CleanType {
    fn to_value(&self) -> Json {
        Json::Str(self.as_str().to_string())
    }
}

impl FromValue for CleanType {
    fn from_value(value: &Json) -> Option<Self> {
        Some(CleanType::parse(value.as_str()))
    }
}

/// Keyword found in a filter's `column` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Column {
    Filename,
    Filepath,
    BundleId,
    Contains,
    Match,
    BeginWith,
    EndWith,
    FileSize,
    Greater,
    Smaller,
    Signed,
    Exclude,
    SubFilePath,
    SubFileCount,
    Time,
    Is,
    Sub,
    App,
}

impl Column {
    /// Accepts the original keywords (lower-case, may contain spaces), plus
    /// the compact/camel spellings found in dumps (`beginwith`, `begin_with`,
    /// `subFileCount` ...).
    pub fn parse(raw: &str) -> Option<Column> {
        let key: String = raw
            .trim()
            .to_ascii_lowercase()
            .chars()
            .filter(|c| !matches!(c, '_' | '-' | ' '))
            .collect();
        Some(match key.as_str() {
            "filename" | "file" | "name" => Column::Filename,
            "filepath" | "path" => Column::Filepath,
            "bundleid" | "bundleidentifier" | "bundle" => Column::BundleId,
            "contains" => Column::Contains,
            "match" | "regex" | "matches" => Column::Match,
            "beginwith" | "startswith" | "prefix" => Column::BeginWith,
            "endwith" | "suffix" => Column::EndWith,
            "filesize" | "size" => Column::FileSize,
            "greater" | "gt" => Column::Greater,
            "smaller" | "lt" => Column::Smaller,
            "signed" | "signature" | "codesign" => Column::Signed,
            "exclude" => Column::Exclude,
            "subfilepath" => Column::SubFilePath,
            "subfilecount" => Column::SubFileCount,
            "time" | "mtime" => Column::Time,
            "is" => Column::Is,
            "sub" => Column::Sub,
            "app" => Column::App,
            _ => return None,
        })
    }

    pub fn as_key(self) -> &'static str {
        match self {
            Column::Filename => "filename",
            Column::Filepath => "filepath",
            Column::BundleId => "bundleid",
            Column::Contains => "contains",
            Column::Match => "match",
            Column::BeginWith => "begin with",
            Column::EndWith => "end with",
            Column::FileSize => "filesize",
            Column::Greater => "greater",
            Column::Smaller => "smaller",
            Column::Signed => "signed",
            Column::Exclude => "exclude",
            Column::SubFilePath => "subfilepath",
            Column::SubFileCount => "subfilecount",
            Column::Time => "time",
            Column::Is => "is",
            Column::Sub => "sub",
            Column::App => "app",
        }
    }
}

/// `relation` attribute of a `<filter>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    And,
    Or,
}

impl Relation {
    pub fn parse(raw: Option<&str>, default: Relation) -> Relation {
        match raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
            Some("or") | Some("||") => Relation::Or,
            Some("and") | Some("&&") => Relation::And,
            _ => default,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Relation::And => "and",
            Relation::Or => "or",
        }
    }
}

impl ToValue for Relation {
    fn to_value(&self) -> Json {
        Json::Str(self.as_str().to_string())
    }
}

// ---------------------------------------------------------------------------
// Filters
// ---------------------------------------------------------------------------

/// One `<filter column relation value>` leaf.
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    /// Raw `column` text, e.g. "begin with".
    pub column_raw: String,
    /// Parsed keyword (`None` when the original app shipped an unknown one).
    pub column: Option<Column>,
    pub relation: Relation,
    pub value: String,
}

impl Filter {
    pub fn new(column: &str, relation: Relation, value: &str) -> Filter {
        Filter {
            column_raw: column.to_string(),
            column: Column::parse(column),
            relation,
            value: value.to_string(),
        }
    }
}

impl ToValue for Filter {
    fn to_value(&self) -> Json {
        let mut o = Json::obj();
        o.insert("column", self.column_raw.clone())
            .insert("relation", self.relation.to_value())
            .insert("value", self.value.clone());
        o
    }
}

impl FromValue for Filter {
    fn from_value(value: &Json) -> Option<Self> {
        let column = value.get("column")?.as_str()?.to_string();
        let relation =
            Relation::parse(value.get("relation").and_then(|v| v.as_str()), Relation::And);
        let val = value
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Some(Filter {
            column_raw: column.clone(),
            column: Column::parse(&column),
            relation,
            value: val,
        })
    }
}

/// A `<filters level="..">` group. Children are `<filter>` leaves or nested
/// [`FilterGroup`]s. Same-level siblings combine with the group relation;
/// deeper levels are evaluated first.
#[derive(Debug, Clone, PartialEq)]
pub struct FilterGroup {
    pub level: u32,
    pub relation: Relation,
    pub filters: Vec<Filter>,
    pub groups: Vec<FilterGroup>,
}

impl FilterGroup {
    pub fn new(level: u32, relation: Relation) -> FilterGroup {
        FilterGroup {
            level,
            relation,
            filters: Vec::new(),
            groups: Vec::new(),
        }
    }

    pub fn with_filter(mut self, f: Filter) -> FilterGroup {
        self.filters.push(f);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.filters.is_empty() && self.groups.is_empty()
    }

    pub fn leaf_count(&self) -> usize {
        self.filters.len() + self.groups.iter().map(|g| g.leaf_count()).sum::<usize>()
    }
}

impl ToValue for FilterGroup {
    fn to_value(&self) -> Json {
        let mut o = Json::obj();
        o.insert("level", self.level)
            .insert("relation", self.relation.to_value())
            .insert("filters", &self.filters[..])
            .insert("groups", &self.groups[..]);
        o
    }
}

impl FromValue for FilterGroup {
    fn from_value(value: &Json) -> Option<Self> {
        let level = value
            .get("level")
            .and_then(|v| v.as_i64())
            .unwrap_or(1)
            .max(1) as u32;
        let relation = Relation::parse(value.get("relation").and_then(|v| v.as_str()), Relation::And);
        let filters = value
            .get("filters")
            .and_then(|v| v.as_arr())
            .map(|arr| arr.iter().filter_map(Filter::from_value).collect())
            .unwrap_or_default();
        let groups = value
            .get("groups")
            .and_then(|v| v.as_arr())
            .map(|arr| arr.iter().filter_map(FilterGroup::from_value).collect())
            .unwrap_or_default();
        Some(FilterGroup {
            level,
            relation,
            filters,
            groups,
        })
    }
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// A `<path type value value1 level filename>` scan root (QMActionPathItem).
#[derive(Debug, Clone, PartialEq)]
pub struct ScanPath {
    /// `type` attribute; `None` when absent (defaults to a directory scan).
    pub kind: Option<String>,
    pub value: String,
    /// Optional second value (`value1`) — alternate / companion root.
    pub value1: Option<String>,
    pub level: u32,
    pub filename: Option<String>,
    pub filters: Vec<FilterGroup>,
}

impl ToValue for ScanPath {
    fn to_value(&self) -> Json {
        let mut o = Json::obj();
        o.insert("type", Json::of(&self.kind))
            .insert("value", self.value.clone())
            .insert("value1", Json::of(&self.value1))
            .insert("level", self.level)
            .insert("filename", Json::of(&self.filename))
            .insert("filters", &self.filters[..]);
        o
    }
}

impl FromValue for ScanPath {
    fn from_value(value: &Json) -> Option<Self> {
        let path = value.get("value")?.as_str()?.to_string();
        Some(ScanPath {
            kind: value
                .get("type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            value: path,
            value1: value
                .get("value1")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            level: value
                .get("level")
                .and_then(|v| v.as_i64())
                .unwrap_or(1)
                .max(1) as u32,
            filename: value
                .get("filename")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            filters: value
                .get("filters")
                .and_then(|v| v.as_arr())
                .map(|arr| arr.iter().filter_map(FilterGroup::from_value).collect())
                .unwrap_or_default(),
        })
    }
}

// ---------------------------------------------------------------------------
// Sub category / item  (QMCategorySubItem / QMCategoryItem)
// ---------------------------------------------------------------------------

/// `<item>` — one cleanable sub-category.
#[derive(Debug, Clone, PartialEq)]
pub struct SubCategory {
    pub title: String,
    pub item_type: Option<String>,
    pub clean_type: CleanType,
    pub recommend: bool,
    pub fast_mode: bool,
    pub show_result: bool,
    pub default_state: bool,
    pub is_cautious: bool,
    pub tips: Option<String>,
    pub bundle_id: Option<String>,
    pub app_store_bundle_id: Option<String>,
    pub paths: Vec<ScanPath>,
}

impl SubCategory {
    /// Stable identifier used in JSON and by the UI ("<bundle>.<title-slug>").
    pub fn key(&self) -> String {
        let mut s = String::new();
        if let Some(b) = &self.bundle_id {
            s.push_str(b);
            s.push('.');
        }
        for ch in self.title.chars() {
            if ch.is_alphanumeric() {
                s.push(ch);
            } else if ch == ' ' || ch == '-' || ch == '/' {
                s.push('_');
            }
        }
        if s.is_empty() {
            s.push_str("item");
        }
        s
    }

    pub fn total_filters(&self) -> usize {
        self.paths
            .iter()
            .flat_map(|p| p.filters.iter())
            .map(|g| g.leaf_count())
            .sum()
    }
}

impl ToValue for SubCategory {
    fn to_value(&self) -> Json {
        let mut o = Json::obj();
        o.insert("key", self.key())
            .insert("title", self.title.clone())
            .insert("type", Json::of(&self.item_type))
            .insert("cleanType", self.clean_type.to_value())
            .insert("recommend", self.recommend)
            .insert("fastMode", self.fast_mode)
            .insert("showResult", self.show_result)
            .insert("defaultState", self.default_state)
            .insert("isCautious", self.is_cautious)
            .insert("tips", Json::of(&self.tips))
            .insert("bundleId", Json::of(&self.bundle_id))
            .insert("appStoreBundleId", Json::of(&self.app_store_bundle_id))
            .insert("paths", &self.paths[..]);
        o
    }
}

impl FromValue for SubCategory {
    fn from_value(value: &Json) -> Option<Self> {
        Some(SubCategory {
            title: value.get("title").and_then(|v| v.as_str())?.to_string(),
            item_type: value
                .get("type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            clean_type: value
                .get("cleanType")
                .and_then(CleanType::from_value)
                .unwrap_or(CleanType::Sys),
            recommend: value
                .get("recommend")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            fast_mode: value.get("fastMode").and_then(|v| v.as_bool()).unwrap_or(false),
            show_result: value
                .get("showResult")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            default_state: value
                .get("defaultState")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            is_cautious: value
                .get("isCautious")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            tips: value.get("tips").and_then(|v| v.as_str()).map(|s| s.to_string()),
            bundle_id: value
                .get("bundleId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            app_store_bundle_id: value
                .get("appStoreBundleId")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            paths: value
                .get("paths")
                .and_then(|v| v.as_arr())
                .map(|arr| arr.iter().filter_map(ScanPath::from_value).collect())
                .unwrap_or_default(),
        })
    }
}

/// `<category>` — a top-level list section.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleCategory {
    pub title: String,
    pub tips: Option<String>,
    pub clean_type: CleanType,
    pub items: Vec<SubCategory>,
}

impl RuleCategory {
    /// Stable key: derived from the first item so the section maps 1:1 onto
    /// a sub-category.
    pub fn key(&self) -> String {
        self.items
            .first()
            .map(|i| i.key())
            .unwrap_or_else(|| self.title.clone())
    }
}

impl ToValue for RuleCategory {
    fn to_value(&self) -> Json {
        let mut o = Json::obj();
        o.insert("key", self.key())
            .insert("title", self.title.clone())
            .insert("tips", Json::of(&self.tips))
            .insert("cleanType", self.clean_type.to_value())
            .insert("items", &self.items[..]);
        o
    }
}

impl FromValue for RuleCategory {
    fn from_value(value: &Json) -> Option<Self> {
        let items: Vec<SubCategory> = value
            .get("items")
            .and_then(|v| v.as_arr())
            .map(|arr| arr.iter().filter_map(SubCategory::from_value).collect())
            .unwrap_or_default();
        Some(RuleCategory {
            title: value
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            tips: value.get("tips").and_then(|v| v.as_str()).map(|s| s.to_string()),
            clean_type: items
                .first()
                .map(|i| i.clean_type)
                .or_else(|| value.get("cleanType").and_then(CleanType::from_value))
                .unwrap_or(CleanType::Sys),
            items,
        })
    }
}

// ---------------------------------------------------------------------------
// Cautions
// ---------------------------------------------------------------------------

/// `<caution>` — 慎删清单 entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Caution {
    pub caution_id: Option<String>,
    pub column: Option<Column>,
    pub column_raw: Option<String>,
    pub relation: Option<Relation>,
    pub bundle_id: Option<String>,
    pub app_name: Option<String>,
    pub value: String,
}

impl ToValue for Caution {
    fn to_value(&self) -> Json {
        let mut o = Json::obj();
        o.insert("cautionID", Json::of(&self.caution_id))
            .insert("column", Json::of(&self.column_raw))
            .insert("relation", Json::of(&self.relation))
            .insert("bundleID", Json::of(&self.bundle_id))
            .insert("appName", Json::of(&self.app_name))
            .insert("value", self.value.clone());
        o
    }
}

impl FromValue for Caution {
    fn from_value(value: &Json) -> Option<Self> {
        let column_raw = value
            .get("column")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let relation = value
            .get("relation")
            .and_then(|v| v.as_str())
            .map(|r| Relation::parse(Some(r), Relation::And));
        Some(Caution {
            caution_id: value
                .get("cautionID")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            column: column_raw.as_deref().and_then(Column::parse),
            column_raw,
            relation,
            bundle_id: value
                .get("bundleID")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            app_name: value
                .get("appName")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            value: value
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_column_keywords_from_string_table() {
        for raw in [
            "filename",
            "filepath",
            "bundleid",
            "contains",
            "match",
            "begin with",
            "end with",
            "filesize",
            "greater",
            "smaller",
            "signed",
            "exclude",
            "subfilepath",
            "subfilecount",
            "time",
            "is",
            "sub",
            "app",
        ] {
            assert!(Column::parse(raw).is_some(), "missing column keyword {}", raw);
        }
        assert_eq!(Column::parse("begin with"), Some(Column::BeginWith));
        assert_eq!(Column::parse("BeginWith"), Some(Column::BeginWith));
        assert_eq!(Column::parse("begin_with"), Some(Column::BeginWith));
        assert_eq!(Column::parse("subFileCount"), Some(Column::SubFileCount));
        assert_eq!(Column::parse("nonsense"), None);
        assert_eq!(Column::BeginWith.as_key(), "begin with");
    }

    #[test]
    fn clean_type_defaults_and_roundtrips() {
        assert_eq!(CleanType::parse(Some("sys")), CleanType::Sys);
        assert_eq!(CleanType::parse(Some("app")), CleanType::App);
        assert_eq!(CleanType::parse(Some("int")), CleanType::Int);
        assert_eq!(CleanType::parse(Some("INT")), CleanType::Int);
        assert_eq!(CleanType::parse(None), CleanType::Sys);
        assert_eq!(CleanType::parse(Some("???")), CleanType::Sys);
    }

    #[test]
    fn sub_category_keys_are_stable() {
        let sub = SubCategory {
            title: "Safari 缓存".into(),
            item_type: None,
            clean_type: CleanType::Int,
            recommend: true,
            fast_mode: false,
            show_result: true,
            default_state: true,
            is_cautious: false,
            tips: None,
            bundle_id: Some("com.apple.Safari".into()),
            app_store_bundle_id: None,
            paths: vec![],
        };
        assert_eq!(sub.key(), "com.apple.Safari.Safari_缓存");
    }

    #[test]
    fn item_json_roundtrip() {
        let original = SubCategory {
            title: "DerivedData".into(),
            item_type: Some("0".into()),
            clean_type: CleanType::App,
            recommend: true,
            fast_mode: true,
            show_result: true,
            default_state: false,
            is_cautious: true,
            tips: Some("t".into()),
            bundle_id: Some("com.apple.dt.Xcode".into()),
            app_store_bundle_id: Some("com.apple.dt.Xcode".into()),
            paths: vec![ScanPath {
                kind: Some("0".into()),
                value: "~/Library/Developer/Xcode/DerivedData/".into(),
                value1: None,
                level: 1,
                filename: Some("*.d".into()),
                filters: vec![FilterGroup::new(1, Relation::And)],
            }],
        };
        let json = original.to_value();
        let back = SubCategory::from_value(&json).unwrap();
        assert_eq!(back, original);
    }
}
