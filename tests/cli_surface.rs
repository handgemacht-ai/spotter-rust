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
