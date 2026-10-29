//! CleanWizard scanning engine.
//!
//! Layering (bottom to top):
//! * \`support\` — dependency-free JSON, SHA-256, thread pool, fs helpers.
//! * \`rules\`   — Lemon-compatible garbage XML model, matcher, macro expansion.
//! * \`scanner\` — concurrent size walking, category scan, big-file, duplicate,
//!   privacy scanning.
//! * \`cleaner\` — trash planning/execution behind an injected executor trait.
//! * \`ffi\`     — C ABI (\`cw_engine_init\`, \`cw_scan\`, \`cw_free_string\`, ...).
//!
//! The engine never performs network I/O.

pub mod cleaner;
pub mod rules;
pub mod scanner;
pub mod support;

pub mod ffi;

/// Engine semantic version, surfaced in the C ABI.
pub const ENGINE_VERSION: &str = "0.1.0";
