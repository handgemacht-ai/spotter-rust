#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used))]
// The library is factored for the CLI and rustdoc already gates public item summaries.
#![allow(clippy::missing_errors_doc)]
// Analytics output intentionally uses f64 percentages over bounded CLI result sets.
#![allow(clippy::cast_precision_loss)]
// SQLite uses i64 counters; transcript sizes are bounded by the performance/RSS gates.
#![allow(clippy::cast_possible_wrap)]
// The public modules are internal seams for integration tests, not a hand-written SDK.
#![allow(clippy::must_use_candidate)]
// Some domain names repeat their module names for clarity in serialized CLI output.
#![allow(clippy::module_name_repetitions)]
// CLI handler ownership follows clap-derived values and keeps dispatch straightforward.
#![allow(clippy::needless_pass_by_value)]
// Closure bodies are kept expression-style when they only print output.
#![allow(clippy::semicolon_if_nothing_returned)]
// Short local names mirror CLI flag/domain wording.
#![allow(clippy::similar_names)]

//! Library support for the `spotter` transcript analytics CLI.
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//!
//! let conn = spotter::db::open(Path::new("/tmp/spotter-example.db"))?;
//! let sessions = spotter::db::list_sessions(&conn)?;
//! assert!(sessions.is_empty());
//! # Ok::<(), anyhow::Error>(())
//! ```

pub mod analytics;
pub mod cli;
pub mod config;
pub mod db;
pub mod jsonl;
pub mod paths;
