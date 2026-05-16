use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use serde_json::{json, Value};
use spotter::jsonl::{self, JsonlError};

#[test]
fn checked_in_jsonl_corpus_parses_cleanly() {
    let root = Path::new("tests/fixtures/transcripts");
    let mut parsed = 0;
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().and_then(OsStr::to_str) != Some("jsonl") {
            continue;
        }

        if path
            .components()
            .any(|component| component.as_os_str() == OsStr::new("subagents"))
        {
            jsonl::parse_subagent_file(path).unwrap_or_else(|error| {
                panic!(
                    "failed to parse subagent corpus file {}: {error}",
                    path.display()
                )
            });
        } else {
            jsonl::parse_session_file(path).unwrap_or_else(|error| {
                panic!("failed to parse corpus file {}: {error}", path.display())
            });
        }
        parsed += 1;
    }

    assert!(parsed >= 5, "expected checked-in JSONL corpus files");
}

#[test]
fn checked_in_fixture_mutations_reject_unknown_fields() {
    let base = fixture_message_with_usage();

    let mut top_level = base.clone();
    top_level
        .as_object_mut()
        .expect("top-level object")
        .insert("unknown_fixture_top".to_string(), json!(true));
    assert_unknown_level(&top_level, "top-level");

    let mut message = base.clone();
    message["message"]
        .as_object_mut()
        .expect("message object")
        .insert("unknown_fixture_message".to_string(), json!(true));
    assert_unknown_level(&message, "message");

    let mut usage = base;
    usage["message"]["usage"]
        .as_object_mut()
        .expect("usage object")
        .insert("unknown_fixture_usage".to_string(), json!(true));
    assert_unknown_level(&usage, "message.usage");
}

fn fixture_message_with_usage() -> Value {
    fs::read_to_string("tests/fixtures/transcripts/short.jsonl")
        .expect("read fixture")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .find_map(|line| {
            let value = serde_json::from_str::<Value>(line).expect("fixture json");
            let has_usage = value
                .get("message")
                .and_then(Value::as_object)
                .and_then(|message| message.get("usage"))
                .and_then(Value::as_object)
                .is_some();
            has_usage.then_some(value)
        })
        .expect("fixture message with usage")
}

fn assert_unknown_level(value: &Value, expected_level: &'static str) {
    let line = serde_json::to_string(&value).expect("serialize mutated fixture");
    let error = jsonl::parse_message_line(&line, 0, "main", 1)
        .expect_err("unknown fixture key should be rejected");
    assert!(
        matches!(error, JsonlError::UnknownField { level, .. } if level == expected_level),
        "unexpected error: {error}"
    );
}
