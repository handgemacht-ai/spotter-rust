#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::too_many_lines)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use regex::Regex;
use tempfile::TempDir;

const TOOL_HEAVY: &str = "tests/fixtures/transcripts/tool_heavy.jsonl";
const SHORT: &str = "tests/fixtures/transcripts/short.jsonl";
const TOOL_HEAVY_SESSION: &str = "d6e0bada-1959-4eec-a9d2-0bfade768d8f";

#[derive(Clone, Copy)]
enum Setup {
    None,
    SeedToolHeavy,
    SeedShort,
    ProjectFixture,
}

struct GoldenCase {
    path: &'static str,
    setup: Setup,
    args: &'static [&'static str],
    stdin: Option<&'static str>,
}

struct CaseEnv<'a> {
    temp_root: &'a Path,
    case_dir: PathBuf,
    db: PathBuf,
    config: PathBuf,
    empty_root: PathBuf,
}

#[test]
fn cli_golden_outputs_match() {
    let temp = TempDir::new().expect("temp dir");

    for case in cases() {
        let env = CaseEnv::new(temp.path(), case.path);
        apply_setup(&env, case.setup);

        let output = run_spotter(&env, case.args, case.stdin);
        let actual = render_output(&env, &output);
        assert_or_regen(case.path, &actual);
    }
}

fn cases() -> &'static [GoldenCase] {
    &[
        GoldenCase {
            path: "transcripts/happy.txt",
            setup: Setup::None,
            args: &["transcripts"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts/empty.txt",
            setup: Setup::None,
            args: &["transcripts", "audit"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts/error.txt",
            setup: Setup::None,
            args: &["transcripts", "missing"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-sync/happy.txt",
            setup: Setup::None,
            args: &["transcripts", "sync", "--file", TOOL_HEAVY],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-sync/empty.txt",
            setup: Setup::None,
            args: &["transcripts", "sync"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-sync/error.txt",
            setup: Setup::None,
            args: &[
                "transcripts",
                "sync",
                "--file",
                "tests/fixtures/transcripts/missing.jsonl",
            ],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-search/happy.txt",
            setup: Setup::SeedToolHeavy,
            args: &["transcripts", "search", "--tool", "Bash", "--limit", "1"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-search/empty.txt",
            setup: Setup::SeedToolHeavy,
            args: &["transcripts", "search", "--tool", "NoSuchTool"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-search/error.txt",
            setup: Setup::SeedToolHeavy,
            args: &["transcripts", "search", "--limit", "not-a-number"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-inspect/happy.txt",
            setup: Setup::SeedToolHeavy,
            args: &[
                "transcripts",
                "inspect",
                "--session",
                TOOL_HEAVY_SESSION,
                "--context",
                "0",
            ],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-inspect/empty.txt",
            setup: Setup::SeedToolHeavy,
            args: &["transcripts", "inspect", "--session", "missing-session"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-inspect/error.txt",
            setup: Setup::SeedToolHeavy,
            args: &["transcripts", "inspect"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-compare/happy.txt",
            setup: Setup::SeedToolHeavy,
            args: &[
                "transcripts",
                "compare",
                "--left-session",
                TOOL_HEAVY_SESSION,
                "--right-session",
                TOOL_HEAVY_SESSION,
            ],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-compare/empty.txt",
            setup: Setup::SeedToolHeavy,
            args: &[
                "transcripts",
                "compare",
                "--left-session",
                "missing-left",
                "--right-session",
                "missing-right",
            ],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-compare/error.txt",
            setup: Setup::SeedToolHeavy,
            args: &[
                "transcripts",
                "compare",
                "--left-session",
                TOOL_HEAVY_SESSION,
            ],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-aggregate/happy.txt",
            setup: Setup::SeedToolHeavy,
            args: &["transcripts", "aggregate"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-aggregate/empty.txt",
            setup: Setup::None,
            args: &["transcripts", "aggregate"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-aggregate/error.txt",
            setup: Setup::SeedToolHeavy,
            args: &["transcripts", "aggregate", "--unknown"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-audit/happy.txt",
            setup: Setup::SeedToolHeavy,
            args: &["transcripts", "audit", "--session", TOOL_HEAVY_SESSION],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-audit/empty.txt",
            setup: Setup::None,
            args: &["transcripts", "audit"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-audit/error.txt",
            setup: Setup::None,
            args: &[
                "transcripts",
                "audit",
                "--file",
                "tests/fixtures/transcripts/missing.jsonl",
            ],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-errors/happy.txt",
            setup: Setup::SeedShort,
            args: &["transcripts", "errors", "--top", "3", "--classify"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-errors/empty.txt",
            setup: Setup::None,
            args: &["transcripts", "errors"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-errors/error.txt",
            setup: Setup::SeedShort,
            args: &["transcripts", "errors", "--top", "not-a-number"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-health/happy.txt",
            setup: Setup::SeedToolHeavy,
            args: &["transcripts", "health", "--session", TOOL_HEAVY_SESSION],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-health/empty.txt",
            setup: Setup::None,
            args: &["transcripts", "health", "--project", "missing"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-health/error.txt",
            setup: Setup::None,
            args: &["transcripts", "health", "--limit", "not-a-number"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-sequences/happy.txt",
            setup: Setup::SeedToolHeavy,
            args: &[
                "transcripts",
                "sequences",
                "--min-occurrences",
                "1",
                "--min-length",
                "2",
                "--max-length",
                "3",
            ],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-sequences/empty.txt",
            setup: Setup::None,
            args: &["transcripts", "sequences", "--min-occurrences", "1"],
            stdin: None,
        },
        GoldenCase {
            path: "transcripts-sequences/error.txt",
            setup: Setup::None,
            args: &["transcripts", "sequences", "--min-length", "not-a-number"],
            stdin: None,
        },
        GoldenCase {
            path: "projects/happy.txt",
            setup: Setup::ProjectFixture,
            args: &["projects", "list"],
            stdin: None,
        },
        GoldenCase {
            path: "projects/empty.txt",
            setup: Setup::None,
            args: &["projects", "list"],
            stdin: None,
        },
        GoldenCase {
            path: "projects/error.txt",
            setup: Setup::None,
            args: &["projects", "add", "fixture"],
            stdin: None,
        },
        GoldenCase {
            path: "projects-add/happy.txt",
            setup: Setup::None,
            args: &["projects", "add", "fixture", "tests/fixtures/transcripts"],
            stdin: None,
        },
        GoldenCase {
            path: "projects-add/error.txt",
            setup: Setup::None,
            args: &["projects", "add", "fixture"],
            stdin: None,
        },
        GoldenCase {
            path: "projects-remove/happy.txt",
            setup: Setup::ProjectFixture,
            args: &["projects", "remove", "fixture"],
            stdin: None,
        },
        GoldenCase {
            path: "projects-remove/empty.txt",
            setup: Setup::None,
            args: &["projects", "remove", "fixture"],
            stdin: None,
        },
        GoldenCase {
            path: "projects-remove/error.txt",
            setup: Setup::None,
            args: &["projects", "remove"],
            stdin: None,
        },
        GoldenCase {
            path: "projects-alias/happy.txt",
            setup: Setup::ProjectFixture,
            args: &["projects", "alias", "fixture", "renamed"],
            stdin: None,
        },
        GoldenCase {
            path: "projects-alias/empty.txt",
            setup: Setup::None,
            args: &["projects", "alias", "fixture", "renamed"],
            stdin: None,
        },
        GoldenCase {
            path: "projects-alias/error.txt",
            setup: Setup::None,
            args: &["projects", "alias", "fixture"],
            stdin: None,
        },
        GoldenCase {
            path: "init/happy.txt",
            setup: Setup::None,
            args: &[
                "init",
                "--claude-projects",
                "tests/fixtures/transcripts",
                "--yes",
            ],
            stdin: None,
        },
        GoldenCase {
            path: "init/empty.txt",
            setup: Setup::None,
            args: &["init", "--claude-projects", "{empty_root}", "--yes"],
            stdin: None,
        },
        GoldenCase {
            path: "init/error.txt",
            setup: Setup::None,
            args: &["init", "--claude-projects", "tests/fixtures/transcripts"],
            stdin: Some("99\n"),
        },
    ]
}

impl<'a> CaseEnv<'a> {
    fn new(temp_root: &'a Path, case_path: &str) -> Self {
        let slug = case_path.replace(['/', '.'], "_");
        let case_dir = temp_root.join(slug);
        fs::create_dir_all(&case_dir).expect("case temp dir");
        let empty_root = case_dir.join("empty-claude-projects");
        fs::create_dir_all(&empty_root).expect("empty claude root");

        Self {
            temp_root,
            db: case_dir.join("spotter.db"),
            config: case_dir.join("config.toml"),
            empty_root,
            case_dir,
        }
    }
}

fn apply_setup(env: &CaseEnv<'_>, setup: Setup) {
    match setup {
        Setup::None => {}
        Setup::SeedToolHeavy => {
            let output = run_spotter(env, &["transcripts", "sync", "--file", TOOL_HEAVY], None);
            assert_success("seed tool-heavy fixture", output);
        }
        Setup::SeedShort => {
            let output = run_spotter(env, &["transcripts", "sync", "--file", SHORT], None);
            assert_success("seed short fixture", output);
        }
        Setup::ProjectFixture => {
            let output = run_spotter(
                env,
                &["projects", "add", "fixture", "tests/fixtures/transcripts"],
                None,
            );
            assert_success("seed project config", output);
        }
    }
}

fn run_spotter(env: &CaseEnv<'_>, args: &[&str], stdin: Option<&str>) -> Output {
    let mut command = Command::cargo_bin("spotter").expect("binary");
    command.arg("--db").arg(&env.db);
    command.arg("--config").arg(&env.config);
    for arg in args {
        if *arg == "{empty_root}" {
            command.arg(&env.empty_root);
        } else {
            command.arg(arg);
        }
    }
    if let Some(stdin) = stdin {
        command.write_stdin(stdin);
    }
    command.output().expect("command output")
}

fn assert_success(context: &str, output: Output) {
    assert!(
        output.status.success(),
        "{context} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn render_output(env: &CaseEnv<'_>, output: &Output) -> String {
    format!(
        "exit: {}\nstdout:\n{}stderr:\n{}",
        output.status.code().unwrap_or(255),
        normalize(env, &output.stdout),
        normalize(env, &output.stderr)
    )
}

fn normalize(env: &CaseEnv<'_>, bytes: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    text = text.replace(&env.case_dir.display().to_string(), "<CASE>");
    text = text.replace(&env.temp_root.display().to_string(), "<TEMP>");
    if let Some(home) = std::env::var_os("HOME").and_then(|value| value.into_string().ok()) {
        text = text.replace(&home, "<HOME>");
    }

    let replacements = [
        (
            r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})",
            "<TIMESTAMP>",
        ),
        (
            r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
            "<UUID>",
        ),
        (r"srvtoolu_[A-Za-z0-9]+", "<SERVER_TOOL_USE_ID>"),
        (r"toolu_[A-Za-z0-9]+", "<TOOL_USE_ID>"),
        (r"req_[A-Za-z0-9]+", "<REQUEST_ID>"),
        (r"/[A-Za-z0-9._~@%+=:,/-]+", "<ABS_PATH>"),
    ];

    for (pattern, replacement) in replacements {
        text = Regex::new(pattern)
            .expect("redaction pattern")
            .replace_all(&text, replacement)
            .into_owned();
    }

    text
}

fn assert_or_regen(relative_path: &str, actual: &str) {
    let path = Path::new("tests/golden").join(relative_path);
    if std::env::var_os("SPOTTER_REGEN_GOLDEN").is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("golden parent");
        }
        fs::write(path, actual).expect("write golden");
        return;
    }

    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    assert_eq!(expected, actual, "golden mismatch: {}", path.display());
}
