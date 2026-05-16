use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

#[test]
fn environment_path_overrides_are_honored() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("state").join("spotter.db");
    let config_path = temp.path().join("config").join("config.toml");

    Command::cargo_bin("spotter")
        .expect("binary")
        .env("SPOTTER_DB_PATH", &db_path)
        .env("SPOTTER_CONFIG_PATH", &config_path)
        .args(["projects", "add", "fixture", "tests/fixtures/transcripts"])
        .assert()
        .success();
    assert!(config_path.exists(), "config env override was not written");

    Command::cargo_bin("spotter")
        .expect("binary")
        .env("SPOTTER_DB_PATH", &db_path)
        .env("SPOTTER_CONFIG_PATH", &config_path)
        .args([
            "transcripts",
            "sync",
            "--file",
            "tests/fixtures/transcripts/short.jsonl",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Synced 1 session(s)."));
    assert!(db_path.exists(), "db env override was not written");

    Command::cargo_bin("spotter")
        .expect("binary")
        .env("SPOTTER_DB_PATH", &db_path)
        .env("SPOTTER_CONFIG_PATH", &config_path)
        .args(["projects", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "fixture | tests/fixtures/transcripts",
        ));
}

#[test]
fn explicit_path_flags_take_precedence_over_environment() {
    let temp = TempDir::new().expect("temp dir");
    let env_db = temp.path().join("env").join("spotter.db");
    let env_config = temp.path().join("env").join("config.toml");
    let flag_db = temp.path().join("flags").join("spotter.db");
    let flag_config = temp.path().join("flags").join("config.toml");

    Command::cargo_bin("spotter")
        .expect("binary")
        .env("SPOTTER_DB_PATH", &env_db)
        .env("SPOTTER_CONFIG_PATH", &env_config)
        .args([
            "--db",
            flag_db.to_str().expect("utf8 db path"),
            "--config",
            flag_config.to_str().expect("utf8 config path"),
            "projects",
            "add",
            "fixture",
            "tests/fixtures/transcripts",
        ])
        .assert()
        .success();

    assert!(flag_config.exists(), "config flag path was not written");
    assert!(
        !env_config.exists(),
        "env config path should not be written"
    );

    Command::cargo_bin("spotter")
        .expect("binary")
        .env("SPOTTER_DB_PATH", &env_db)
        .env("SPOTTER_CONFIG_PATH", &env_config)
        .args([
            "--db",
            flag_db.to_str().expect("utf8 db path"),
            "--config",
            flag_config.to_str().expect("utf8 config path"),
            "transcripts",
            "sync",
            "--file",
            "tests/fixtures/transcripts/short.jsonl",
        ])
        .assert()
        .success();

    assert!(flag_db.exists(), "db flag path was not written");
    assert!(!env_db.exists(), "env db path should not be written");
}

#[test]
fn xdg_default_paths_are_respected() {
    let temp = TempDir::new().expect("temp dir");
    let xdg_config = temp.path().join("xdg-config");
    let xdg_data = temp.path().join("xdg-data");
    let expected_config = xdg_config.join("spotter").join("config.toml");
    let expected_db = xdg_data.join("spotter").join("spotter.db");

    Command::cargo_bin("spotter")
        .expect("binary")
        .env_remove("SPOTTER_DB_PATH")
        .env_remove("SPOTTER_CONFIG_PATH")
        .env("XDG_CONFIG_HOME", &xdg_config)
        .env("XDG_DATA_HOME", &xdg_data)
        .args(["projects", "add", "fixture", "tests/fixtures/transcripts"])
        .assert()
        .success();
    assert!(expected_config.exists(), "XDG config path was not written");

    Command::cargo_bin("spotter")
        .expect("binary")
        .env_remove("SPOTTER_DB_PATH")
        .env_remove("SPOTTER_CONFIG_PATH")
        .env("XDG_CONFIG_HOME", &xdg_config)
        .env("XDG_DATA_HOME", &xdg_data)
        .args([
            "transcripts",
            "sync",
            "--file",
            "tests/fixtures/transcripts/short.jsonl",
        ])
        .assert()
        .success();
    assert!(expected_db.exists(), "XDG data path was not written");
}
