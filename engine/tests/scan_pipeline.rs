//! End-to-end engine checks against real files in a temporary tree.

use std::fs;
use std::path::{Path, PathBuf};

use cleanwizard_engine::rules::RuleSet;
use cleanwizard_engine::scanner::scan::{scan, Scope};

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "cw-it-{tag}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Scratch(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, bytes: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![b'x'; bytes]).unwrap();
}

fn seed_rules_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../CleanWizard/Resources/Rules/seed/seed.lwx.xml")
}

#[test]
fn seed_rules_parse_and_resolve_every_path() {
    let rules = RuleSet::load_file(&seed_rules_path()).expect("seed rules must parse");
    assert!(!rules.categories.is_empty(), "seed rules define categories");

    let (specs, rejected) = rules.path_specs();
    assert!(rejected.is_empty(), "seed paths must resolve: {rejected:?}");
    assert!(specs.len() >= 5, "seed rules expose several scan roots");
    assert!(
        specs.iter().all(|s| s.path.is_absolute()),
        "every root must be absolute"
    );
}

#[test]
fn scan_reports_sizes_and_paths() {
    let scratch = Scratch::new("scan");
    let root = scratch.path().join("Library/Caches");
    write(&root.join("demo/cache.bin"), 4096);
    write(&root.join("demo/notes.txt"), 128);

    let rules = RuleSet::parse_xml(&format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<garbage version="t"><category title="System Junk"><item title="Cache" cleanType="sys">
<path type="dir" value="{}"/>
</item></category></garbage>"#,
        root.display()
    ))
    .expect("inline rules parse");

    let outcome = scan(&rules, &Scope::default());
    assert_eq!(outcome.items.len(), 1, "one cleanable item expected");
    let item = &outcome.items[0];
    assert_eq!(item.size, 4096 + 128);
    assert_eq!(item.file_count, 2);
    // Without filters the whole subtree is one cleanable unit, so the sample
    // path is the directory itself.
    assert!(
        item.paths.iter().any(|p| p.ends_with("demo")),
        "{:?}",
        item.paths
    );
}

#[test]
fn filename_filter_narrows_the_result() {
    let scratch = Scratch::new("filter");
    let root = scratch.path().join("Downloads");
    write(&root.join("installer.dmg"), 2048);
    write(&root.join("keep.txt"), 512);

    let rules = RuleSet::parse_xml(&format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<garbage version="t"><category title="App Junk"><item title="Installers" cleanType="app">
<path type="dir" value="{}">
<filters level="1"><filter column="filename" relation="end with" value=".dmg"/></filters>
</path>
</item></category></garbage>"#,
        root.display()
    ))
    .expect("inline rules parse");

    let outcome = scan(&rules, &Scope::default());
    assert_eq!(outcome.items.len(), 1);
    assert_eq!(outcome.items[0].size, 2048, "only the .dmg counts");
    assert_eq!(outcome.items[0].file_count, 1);
}

#[test]
fn scope_restricts_clean_types() {
    let rules = RuleSet::load_file(&seed_rules_path()).unwrap();
    let scope = Scope {
        clean_types: Some(vec![cleanwizard_engine::rules::model::CleanType::Int]),
        ..Scope::default()
    };
    let outcome = scan(&rules, &scope);
    assert!(
        outcome.items.iter().all(|i| i.clean_type.as_str() == "int"),
        "only internet junk survives the scope"
    );
}

#[test]
fn missing_roots_are_reported_not_fatal() {
    let scratch = Scratch::new("missing");
    let root = scratch.path().join("does-not-exist");
    let rules = RuleSet::parse_xml(&format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<garbage version="t"><category title="Empty"><item title="Ghost" cleanType="sys">
<path type="dir" value="{}"/>
</item></category></garbage>"#,
        root.display()
    ))
    .unwrap();

    let outcome = scan(&rules, &Scope::default());
    assert!(outcome.items.is_empty());
    assert!(!outcome.skipped.is_empty(), "the missing root is reported");
}

#[test]
fn cleaner_refuses_dangerous_paths() {
    use cleanwizard_engine::cleaner::{CleanPlan, PlanOptions};

    let plan = CleanPlan::from_paths(
        &["/", "/System/Library", "relative/path", "/Users"],
        &PlanOptions::default(),
    );
    assert!(plan.paths.is_empty(), "{:?}", plan.paths);
    assert_eq!(plan.skipped.len(), 4);
}

#[test]
fn sibling_filter_groups_are_alternatives() {
    // Three separate <filters> groups on one path: any one may match.
    let scratch = Scratch::new("siblings");
    let root = scratch.path().join("Downloads");
    write(&root.join("setup.dmg"), 1000);
    write(&root.join("archive.zip"), 2000);
    write(&root.join("readme.txt"), 300);

    let rules = RuleSet::parse_xml(&format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<garbage version="t"><category title="App Junk"><item title="Installers" cleanType="app">
<path type="dir" value="{}">
<filters level="1"><filter column="filename" relation="end with" value=".dmg"/></filters>
<filters level="1"><filter column="filename" relation="end with" value=".zip"/></filters>
</path></item></category></garbage>"#,
        root.display()
    ))
    .unwrap();

    let outcome = scan(&rules, &Scope::default());
    assert_eq!(outcome.items.len(), 1);
    assert_eq!(outcome.items[0].size, 3000, "both groups contribute");
    assert_eq!(outcome.items[0].file_count, 2, "readme.txt is excluded");
}
