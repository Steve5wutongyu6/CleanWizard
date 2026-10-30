//! Smoke test for the C ABI: parse the bundled rules, scan, print a summary.
//!
//! Run with: cargo run --example scan_smoke

use std::ffi::{c_char, CStr, CString};

use cleanwizard_engine::ffi::{cw_engine_free, cw_engine_version, cw_parse_rules, cw_scan};

unsafe fn take(ptr: *mut c_char) -> String {
    if ptr.is_null() {
        return String::from("{\"error\":\"null\"}");
    }
    let text = CStr::from_ptr(ptr).to_string_lossy().to_string();
    cw_engine_free(ptr);
    text
}

fn main() {
    let version = unsafe { take(cw_engine_version() as *mut c_char) };
    println!("engine {version}");

    let rules_path = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| {"../CleanWizard/Resources/Rules/seed/seed.lwx.xml".to_string()}));
    let c_path = CString::new(rules_path.to_string_lossy().as_bytes()).unwrap();

    let summary = unsafe { take(cw_parse_rules(c_path.as_ptr())) };
    println!("rules: {}", &summary[..summary.len().min(240)]);

    let scope = CString::new("{\"categories\":[\"sys\"]}").unwrap();
    let outcome = unsafe { take(cw_scan(c_path.as_ptr(), scope.as_ptr())) };
    println!("scan: {}", &outcome[..outcome.len().min(600)]);
}
