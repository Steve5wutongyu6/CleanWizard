//! Dependency-free JSON + filesystem scratch + environment helpers.
//!
//! The engine is deliberately free of third-party crates so that it builds
//! offline and inside an Xcode build without a cargo registry.

pub mod fsutil;
pub mod json;
pub mod sandbox;
pub mod sha256;
pub mod threadpool;
