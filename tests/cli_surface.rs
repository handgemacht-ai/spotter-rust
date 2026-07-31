use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::NamedTempFile;

const FIXTURE: &str = "tests/fixtures/transcripts/tool_heavy.jsonl";

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
        &["transcripts", "profile", "--help"],
        &["transcripts", "sample", "--help"],
        &["transcripts", "similar", "--help"],
        &["embed", "--help"],
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

#[test]
fn unknown_format_value_is_rejected() {
    for args in [
        vec!["transcripts", "search", "--format", "yaml"],
        vec!["scan", "--file", FIXTURE, "aggregate", "--format", "yaml"],
    ] {
        spotter(&args)
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "invalid value 'yaml' for '--format <FORMAT>'",
            ))
            .stderr(predicate::str::contains("[possible values: table, json]"));
    }
}

#[test]
fn known_format_values_are_accepted() {
    for format in ["table", "json"] {
        spotter(&[
            "scan",
            "--file",
            FIXTURE,
            "aggregate",
            "--format",
            format,
            "--group-by",
            "tool_name,status",
        ])
        .assert()
        .success();
    }
}

#[test]
fn unknown_group_by_key_is_rejected() {
    for args in [
        vec!["transcripts", "aggregate", "--group-by", "bogus"],
        vec![
            "scan",
            "--file",
            FIXTURE,
            "aggregate",
            "--group-by",
            "tool_name,bogus",
        ],
        vec![
            "scan",
            "--file",
            FIXTURE,
            "compare",
            "--left-session",
            "a",
            "--right-session",
            "b",
            "--group-by",
            "bogus",
        ],
    ] {
        spotter(&args)
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "invalid value 'bogus' for '--group-by <GROUP_BY>'",
            ))
            .stderr(predicate::str::contains(
                "[possible values: tool_name, status, project, worktree, agent_id]",
            ));
    }
}

#[test]
fn known_group_by_keys_are_accepted() {
    for key in [
        "tool_name",
        "status",
        "project",
        "project_alias",
        "worktree",
        "worktree_name",
        "agent_id",
    ] {
        spotter(&[
            "scan",
            "--file",
            FIXTURE,
            "aggregate",
            "--group-by",
            key,
            "--format",
            "json",
        ])
        .assert()
        .success();
    }
}

#[test]
fn unknown_config_key_fails_the_run() {
    let config = NamedTempFile::new().expect("temp config");
    std::fs::write(config.path(), "unknown_root_key = 1\n").expect("write config");

    Command::cargo_bin("spotter")
        .expect("binary")
        .args([
            "--config",
            config.path().to_str().expect("utf8 config path"),
            "projects",
            "list",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown_root_key"));
}

/// Run the CLI against throwaway db/config paths so flag validation never
/// depends on the developer's real Spotter state.
fn spotter(args: &[&str]) -> Command {
    let db = NamedTempFile::new().expect("temp db");
    let config = NamedTempFile::new().expect("temp config");
    let mut command = Command::cargo_bin("spotter").expect("binary");
    command.args([
        "--db",
        db.path().to_str().expect("utf8 db path"),
        "--config",
        config.path().to_str().expect("utf8 config path"),
    ]);
    command.args(args);
    command
}

#[test]
fn dropped_slice_register_command_is_rejected() {
    Command::cargo_bin("spotter")
        .expect("binary")
        .args(["transcripts", "slice.register"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unrecognized subcommand 'slice.register'",
        ));
}
