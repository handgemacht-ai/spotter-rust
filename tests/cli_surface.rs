use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn every_subcommand_has_non_empty_help() {
    let commands: &[&[&str]] = &[
        &["--help"],
        &["transcripts", "--help"],
        &["transcripts", "sync", "--help"],
        &["transcripts", "search", "--help"],
        &["transcripts", "inspect", "--help"],
        &["transcripts", "compare", "--help"],
        &["transcripts", "aggregate", "--help"],
        &["transcripts", "audit", "--help"],
        &["transcripts", "errors", "--help"],
        &["transcripts", "health", "--help"],
        &["transcripts", "sequences", "--help"],
        &["projects", "--help"],
        &["projects", "list", "--help"],
        &["projects", "add", "--help"],
        &["projects", "remove", "--help"],
        &["projects", "alias", "--help"],
        &["init", "--help"],
    ];

    for args in commands {
        Command::cargo_bin("spotter")
            .expect("binary")
            .args(*args)
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());
    }
}

#[test]
fn transcript_help_index_runs_without_arguments() {
    Command::cargo_bin("spotter")
        .expect("binary")
        .args(["transcripts"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Spotter Transcript Analytics CLI"));
}

#[test]
fn transcript_help_index_lists_supported_transcript_commands() {
    let output = Command::cargo_bin("spotter")
        .expect("binary")
        .args(["transcripts"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).expect("utf8 stdout");

    for command in [
        "spotter transcripts sync",
        "spotter transcripts search",
        "spotter transcripts inspect",
        "spotter transcripts compare",
        "spotter transcripts aggregate",
        "spotter transcripts audit",
        "spotter transcripts errors",
        "spotter transcripts health",
        "spotter transcripts sequences",
    ] {
        assert!(
            stdout.contains(command),
            "missing help-index command: {command}"
        );
    }

    assert!(!stdout.contains("slice.register"));
}
