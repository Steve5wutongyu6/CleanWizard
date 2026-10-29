//! Scanning: rule roots -> sized items.
//!
//! `scan` is the only entry point the app needs; big-file, duplicate and
//! privacy scans build on the same walk helpers.

pub mod duplicates;
pub mod bigfile;
pub mod privacy;
pub mod scan;

pub use scan::{
    scan, tree_size, ItemResult, ScanOptions, ScanOutcome, Scope,
};
