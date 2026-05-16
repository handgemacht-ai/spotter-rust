use std::path::{Path, PathBuf};

use criterion::{criterion_group, criterion_main, Criterion};
use tempfile::NamedTempFile;

use spotter::{config::Config, db, jsonl};

fn must<T, E: std::fmt::Display>(result: Result<T, E>, context: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{context}: {error}"),
    }
}

fn parse_session_file(c: &mut Criterion) {
    let fixture = Path::new("tests/fixtures/transcripts/tool_heavy.jsonl");
    c.bench_function("jsonl_parse_session_file", |b| {
        b.iter(|| must(jsonl::parse_session_file(fixture), "parse fixture"));
    });
}

fn ingest_and_derive_runs(c: &mut Criterion) {
    let fixture = Path::new("tests/fixtures/transcripts/tool_heavy.jsonl");
    let parsed = must(jsonl::parse_session_file(fixture), "parse fixture");
    let config = Config::default();
    let alias = config.alias_for_cwd(parsed.cwd.as_deref());
    let project_path = parsed
        .cwd
        .as_ref()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);

    c.bench_function("analytics_derive_runs_via_ingest", |b| {
        b.iter(|| {
            let db_file = must(NamedTempFile::new(), "temp db");
            let mut conn = must(db::open(db_file.path()), "open db");
            must(
                db::ingest_session(&mut conn, &parsed, fixture, &alias, &project_path, None),
                "ingest fixture",
            );
        });
    });
}

criterion_group!(benches, parse_session_file, ingest_and_derive_runs);
criterion_main!(benches);
