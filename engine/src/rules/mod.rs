//! Rule engine: parse original-app-compatible garbage XML and evaluate it
//! against real files.

pub mod expand;
pub mod matcher;
pub mod model;
pub mod xml;

pub use matcher::{Decision, Matcher, PathSpec};
pub use model::{
    Caution, CleanType, Column, Filter, FilterGroup, Relation, RuleCategory, ScanPath, SubCategory,
};
pub use xml::Elem;

use crate::support::json::{Json, ToValue};
use std::path::{Path, PathBuf};

/// Top-level \`<garbage>\` document.
#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    pub version: String,
    pub appstore_version: Option<String>,
    pub categories: Vec<RuleCategory>,
    pub cautions: Vec<Caution>,
    pub global_filters: Vec<FilterGroup>,
    /// Where this rule set came from (seed file, Knowledge dir, ...).
    pub source: Option<String>,
}

/// Why a \`<path>\` or \`<item>\` could not be turned into a scan spec.
#[derive(Debug, Clone, PartialEq)]
pub struct RejectedPath {
    pub item: String,
    pub pattern: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct RuleSummary {
    pub categories: usize,
    pub items: usize,
    pub paths: usize,
    pub cautions: usize,
    pub global_filters: usize,
    pub rejected: Vec<RejectedPath>,
}

#[derive(Debug)]
pub struct RuleLoadError {
    pub path: PathBuf,
    pub reason: String,
}

impl std::fmt::Display for RuleLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.reason)
    }
}

impl std::error::Error for RuleLoadError {}

impl RuleSet {
    pub fn parse_xml(xml_text: &str) -> Result<RuleSet, String> {
        let root = xml::parse(xml_text).map_err(|e| e.to_string())?;
        Ok(RuleSet::from_elem(&root))
    }

    pub fn from_elem(root: &Elem) -> RuleSet {
        let version = root.attr("version").unwrap_or("2").to_string();
        let appstore_version = root.attr("garbage_appstore").map(|s| s.to_string());

        let mut categories = Vec::new();
        for cat in root.children_named("category") {
            let mut items = Vec::new();
            for item in cat.children_named("item") {
                items.push(parse_item(item));
            }
            if items.is_empty() {
                continue;
            }
            let title = cat
                .attr("title")
                .map(|s| s.to_string())
                .unwrap_or_else(|| items[0].title.clone());
            let clean_type = items[0].clean_type;
            categories.push(RuleCategory {
                title,
                tips: cat.attr("tips").map(|s| s.to_string()),
                clean_type,
                items,
            });
        }

        let mut cautions = Vec::new();
        for cautions_node in root.children_named("cautions") {
            for c in cautions_node.children_named("caution") {
                cautions.push(parse_caution(c));
            }
        }
        // Tolerate <caution> directly under the root.
        for c in root.children_named("caution") {
            cautions.push(parse_caution(c));
        }

        let global_filters = parse_filter_nodes(root.children_named("filters"));

        RuleSet {
            version,
            appstore_version,
            categories,
            cautions,
            global_filters,
            source: None,
        }
    }

    /// Load every \`*.plist\` / \`*.xml\` file in \`dir\` (non-recursive), sorted
    /// by file name. Files that fail to parse are reported but do not poison
    /// the rest of the directory.
    pub fn load_dir(dir: &Path) -> Result<Vec<RuleSet>, Vec<RuleLoadError>> {
        let mut errors = Vec::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                return Err(vec![RuleLoadError {
                    path: dir.to_path_buf(),
                    reason: format!("read_dir: {}", e),
                }])
            }
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| {
                            let e = e.to_ascii_lowercase();
                            e == "plist" || e == "xml"
                        })
                        .unwrap_or(false)
            })
            .collect();
        files.sort();
        let mut sets = Vec::new();
        for file in files {
            match RuleSet::load_file(&file) {
                Ok(set) => sets.push(set),
                Err(e) => errors.push(e),
            }
        }
        if sets.is_empty() && !errors.is_empty() {
            Err(errors)
        } else {
            Ok(sets)
        }
    }

    pub fn load_file(path: &Path) -> Result<RuleSet, RuleLoadError> {
        let bytes = std::fs::read(path).map_err(|e| RuleLoadError {
            path: path.to_path_buf(),
            reason: format!("read: {}", e),
        })?;
        let text = String::from_utf8(bytes).map_err(|e| RuleLoadError {
            path: path.to_path_buf(),
            reason: format!("not utf-8: {}", e),
        })?;
        let mut set = RuleSet::parse_xml(&text).map_err(|e| RuleLoadError {
            path: path.to_path_buf(),
            reason: e,
        })?;
        set.source = Some(path.to_string_lossy().into_owned());
        Ok(set)
    }

    /// Append (\`other\`) rules onto this set. Duplicate items (same key) and
    /// duplicate cautions are skipped so a seed library and a dumped pack
    /// can be layered.
    pub fn merge(&mut self, other: RuleSet) {
        for cat in other.categories {
            match self.categories.iter_mut().find(|c| c.title == cat.title) {
                Some(existing) => {
                    for item in cat.items {
                        if !existing.items.iter().any(|i| i.key() == item.key()) {
                            existing.items.push(item);
                        }
                    }
                }
                None => self.categories.push(cat),
            }
        }
        for c in other.cautions {
            if !self
                .cautions
                .iter()
                .any(|x| x.caution_id == c.caution_id && x.value == c.value)
            {
                self.cautions.push(c);
            }
        }
        self.global_filters.extend(other.global_filters);
    }

    pub fn items(&self) -> impl Iterator<Item = (&RuleCategory, &SubCategory)> {
        self.categories
            .iter()
            .flat_map(|c| c.items.iter().map(move |i| (c, i)))
    }

    pub fn find_item(&self, key: &str) -> Option<&SubCategory> {
        self.items().map(|(_, i)| i).find(|i| i.key() == key)
    }

    pub fn summary(&self) -> RuleSummary {
        let mut s = RuleSummary {
            categories: self.categories.len(),
            ..Default::default()
        };
        for item in self.items().map(|(_, i)| i) {
            s.items += 1;
            s.paths += item.paths.len();
        }
        s.cautions = self.cautions.len();
        s.global_filters = self.global_filters.len();
        s
    }

    /// Turn every rule path into an evaluable [\`PathSpec\`]. Paths whose
    /// macros cannot be resolved without an application name (\`SPACE\`) are
    /// reported as rejected and skipped.
    pub fn path_specs(&self) -> (Vec<PathSpec>, Vec<RejectedPath>) {
        let mut specs = Vec::new();
        let mut rejected = Vec::new();
        for (cat, item) in self.items() {
            for path in &item.paths {
                match PathSpec::from_scan_path(cat, item, path, &self.global_filters) {
                    Ok(spec) => specs.push(spec),
                    Err(reason) => rejected.push(RejectedPath {
                        item: item.key(),
                        pattern: path.value.clone(),
                        reason,
                    }),
                }
            }
        }
        (specs, rejected)
    }

    pub fn to_json(&self) -> Json {
        let mut o = Json::obj();
        o.insert("version", self.version.clone())
            .insert("appstoreVersion", Json::of(&self.appstore_version))
            .insert("source", Json::of(&self.source));
        let mut cats = Json::arr();
        for c in &self.categories {
            cats.push(c.to_value());
        }
        o.insert("categories", cats);
        let mut cautions = Json::arr();
        for c in &self.cautions {
            cautions.push(c.to_value());
        }
        o.insert("cautions", cautions);
        let mut gf = Json::arr();
        for g in &self.global_filters {
            gf.push(g.to_value());
        }
        o.insert("globalFilters", gf);
        o
    }

    pub fn to_json_string(&self) -> String {
        self.to_json().to_string_compact()
    }
}

fn parse_item(item: &Elem) -> SubCategory {
    let paths = item
        .children_named("path")
        .map(|p| ScanPath {
            kind: p.attr("type").map(|s| s.to_string()),
            value: p.attr("value").unwrap_or("").trim().to_string(),
            value1: p.attr("value1").map(|s| s.to_string()),
            level: p
                .attr("level")
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(1)
                .max(1),
            filename: p.attr("filename").map(|s| s.to_string()),
            filters: parse_filter_nodes(p.children_named("filters")),
        })
        .collect();

    SubCategory {
        title: item
            .attr("title")
            .map(|s| s.to_string())
            .unwrap_or_else(|| "item".to_string()),
        item_type: item.attr("type").map(|s| s.to_string()),
        clean_type: CleanType::parse(item.attr("cleanType")),
        recommend: parse_flag(item.attr("recommend"), true),
        fast_mode: parse_flag(item.attr("fastMode"), false),
        show_result: parse_flag(item.attr("showResult"), true),
        default_state: parse_flag(item.attr("defaultState"), true),
        is_cautious: parse_flag(item.attr("isCautious"), false),
        tips: item.attr("tips").map(|s| s.to_string()),
        bundle_id: item.attr("bundleId").map(|s| s.to_string()),
        app_store_bundle_id: item.attr("appStoreBundleId").map(|s| s.to_string()),
        paths,
    }
}

fn parse_caution(c: &Elem) -> Caution {
    let column_raw = c.attr("column").map(|s| s.to_string());
    Caution {
        caution_id: c.attr("cautionID").map(|s| s.to_string()),
        column: column_raw.as_deref().and_then(Column::parse),
        column_raw,
        relation: c
            .attr("relation")
            .map(|r| Relation::parse(Some(r), Relation::And)),
        bundle_id: c.attr("bundleID").map(|s| s.to_string()),
        app_name: c.attr("appName").map(|s| s.to_string()),
        value: c.attr("value").unwrap_or("").to_string(),
    }
}

/// Parse a \`<filters>\` sequence. Top-level groups default to \`and\`.
fn parse_filter_nodes<'a>(nodes: impl Iterator<Item = &'a Elem>) -> Vec<FilterGroup> {
    nodes
        .map(|f| parse_filter_group(f, Relation::And))
        .filter(|g| !g.is_empty())
        .collect()
}

fn parse_filter_group(node: &Elem, default_relation: Relation) -> FilterGroup {
    let level = node
        .attr("level")
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(1)
        .max(1);
    let relation = Relation::parse(node.attr("relation"), default_relation);
    let mut filters = Vec::new();
    let mut groups = Vec::new();
    for child in &node.children {
        if child.tag.eq_ignore_ascii_case("filter") {
            let column = child.attr("column").unwrap_or("").to_string();
            let rel = Relation::parse(child.attr("relation"), relation);
            filters.push(Filter {
                column_raw: column.clone(),
                column: Column::parse(&column),
                relation: rel,
                value: child.attr("value").unwrap_or("").to_string(),
            });
        } else if child.tag.eq_ignore_ascii_case("filters") {
            groups.push(parse_filter_group(child, relation));
        }
    }
    FilterGroup {
        level,
        relation,
        filters,
        groups,
    }
}

fn parse_flag(raw: Option<&str>, default: bool) -> bool {
    match raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("yes") | Some("true") | Some("1") | Some("on") => true,
        Some("no") | Some("false") | Some("0") | Some("off") => false,
        Some("") | None => default,
        _ => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"<garbage version="2" garbage_appstore="NO">
  <category title="系统垃圾" tips="sys">
    <item title="日志" type="0" cleanType="sys" recommend="YES" fastMode="NO" showResult="YES" defaultState="YES" isCautious="NO" bundleId="com.apple">
      <path type="0" value="~/Library/Logs/SPACE" level="1">
        <filters level="1">
          <filter column="filename" relation="contains" value="log"/>
          <filter column="filesize" relation="greater" value="1 MB"/>
        </filters>
      </path>
      <path type="1" value="/Library/Logs/" level="1" value1="/var/log/"/>
    </item>
  </category>
  <category title="上网垃圾">
    <item title="Safari 缓存" cleanType="int" bundleId="com.apple.Safari" defaultState="NO">
      <path type="0" value="~/Library/Caches/com.apple.Safari" level="1">
        <filters level="1" relation="or">
          <filter column="subfilecount" relation="greater" value="10"/>
        </filters>
      </path>
    </item>
  </category>
  <cautions>
    <caution cautionID="c1" column="filename" bundleID="com.apple.Safari" appName="Safari" value="History.db"/>
  </cautions>
  <filters>
    <filter column="filepath" relation="contains" value="/Protect/"/>
  </filters>
</garbage>"#;

    #[test]
    fn parses_full_rule_document() {
        let set = RuleSet::parse_xml(DOC).unwrap();
        assert_eq!(set.version, "2");
        assert_eq!(set.appstore_version.as_deref(), Some("NO"));
        let s = set.summary();
        assert_eq!(s.categories, 2);
        assert_eq!(s.items, 2);
        assert_eq!(s.paths, 3);
        assert_eq!(s.cautions, 1);
        assert_eq!(s.global_filters, 1);

        let logs = set.find_item("com.apple.日志").unwrap();
        assert!(logs.recommend);
        assert!(!logs.fast_mode);
        assert_eq!(logs.paths.len(), 2);
        assert_eq!(logs.paths[1].value1.as_deref(), Some("/var/log/"));
        assert_eq!(logs.total_filters(), 2);

        let safari = set.find_item("com.apple.Safari.Safari_缓存").unwrap();
        assert_eq!(safari.clean_type, CleanType::Int);
        assert!(!safari.default_state);
    }

    #[test]
    fn filters_keep_columns_and_relations() {
        let set = RuleSet::parse_xml(DOC).unwrap();
        let logs = set
            .items()
            .map(|(_, i)| i)
            .find(|i| i.title == "日志")
            .unwrap();
        let group = &logs.paths[0].filters[0];
        assert_eq!(group.level, 1);
        assert_eq!(group.relation, Relation::And);
        assert_eq!(group.filters[0].column, Some(Column::Filename));
        assert_eq!(group.filters[1].value, "1 MB");
        let safari = set
            .items()
            .map(|(_, i)| i)
            .find(|i| i.title == "Safari 缓存")
            .unwrap();
        assert_eq!(safari.paths[0].filters[0].relation, Relation::Or);
    }

    #[test]
    fn path_specs_report_unresolved_space_macro() {
        let set = RuleSet::parse_xml(DOC).unwrap();
        let (specs, rejected) = set.path_specs();
        assert_eq!(specs.len(), 2, "safari cache + /Library/Logs usable");
        assert!(rejected.iter().any(|r| r.pattern.contains("SPACE")));
    }

    #[test]
    fn merge_dedups_items() {
        let mut a = RuleSet::parse_xml(DOC).unwrap();
        let b = RuleSet::parse_xml(DOC).unwrap();
        let before = a.summary();
        a.merge(b);
        let after = a.summary();
        assert_eq!(before.items, after.items);
        assert_eq!(before.cautions, after.cautions);
    }

    #[test]
    fn flags_parse_defensively() {
        assert!(parse_flag(Some("YES"), false));
        assert!(!parse_flag(Some("no"), true));
        assert!(parse_flag(None, true));
        assert!(!parse_flag(Some("garbage"), false));
    }
}
