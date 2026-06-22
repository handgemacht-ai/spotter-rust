//! Command-line interface implementation.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::{Context, Result};
use chrono::Utc;
use clap::{Args, Parser, Subcommand};
use serde::Serialize;
use signal_hook::{consts::SIGINT, flag};

use crate::{analytics, config::Config, db, jsonl, paths, scan};

/// Run the Spotter CLI.
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let db_path = paths::db_path(cli.db)?;
    let config_path = paths::config_path(cli.config)?;
    let config = Config::read_or_default(&config_path)?;
    let cancel = cancellation_flag()?;

    match cli.command {
        Command::Transcripts(command) => run_transcripts(command, db_path, config, &cancel),
        Command::Projects(command) => run_projects(command, config_path, config),
        Command::Init(args) => init(args, config_path),
        Command::Scan(command) => run_scan(command, &config, &cancel),
    }
}

fn cancellation_flag() -> Result<Arc<AtomicBool>> {
    let cancel = Arc::new(AtomicBool::new(false));
    flag::register(SIGINT, Arc::clone(&cancel)).context("failed to register SIGINT handler")?;
    Ok(cancel)
}

fn check_cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("operation cancelled by SIGINT");
    }
    Ok(())
}

#[derive(Debug, Parser)]
#[command(
    name = "spotter",
    version,
    about = "Local Claude Code transcript analytics"
)]
struct Cli {
    /// Override the SQLite database path.
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    /// Override the TOML config path.
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Command {
    /// Transcript analytics commands.
    Transcripts(TranscriptsCommand),
    /// Manage configured projects.
    Projects(ProjectsCommand),
    /// Create a starter config from Claude Code projects.
    Init(InitArgs),
    /// Query JSONL transcripts directly, without the SQLite DB.
    Scan(ScanCommand),
}

#[derive(Debug, Parser)]
struct TranscriptsCommand {
    #[command(subcommand)]
    command: Option<TranscriptSubcommand>,
}

#[derive(Debug, Subcommand)]
enum TranscriptSubcommand {
    /// Import/re-import transcripts and derive tool call runs.
    Sync(SyncArgs),
    /// Search tool call runs and transcript content.
    Search(SearchArgs),
    /// Inspect tool call runs for a specific session.
    Inspect(InspectArgs),
    /// Compare tool runs between session cohorts.
    Compare(CompareArgs),
    /// Aggregate tool usage across sessions.
    Aggregate(AggregateArgs),
    /// Audit transcript import completeness.
    Audit(AuditArgs),
    /// Analyze tool call errors.
    Errors(ErrorsArgs),
    /// Analyze transcript token health.
    Health(HealthArgs),
    /// Find tool call patterns and retries.
    Sequences(SequencesArgs),
}

#[derive(Debug, Args)]
struct SyncArgs {
    /// Sync a specific session by external id.
    #[arg(long)]
    session: Option<String>,

    /// Sync from a specific JSONL transcript file.
    #[arg(long)]
    file: Option<PathBuf>,

    /// Sync all sessions under a transcript root.
    #[arg(long)]
    transcript_root: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct SearchArgs {
    /// Filter by project alias.
    #[arg(long)]
    project: Option<String>,

    /// Filter by worktree name.
    #[arg(long)]
    worktree: Option<String>,

    /// Filter by session id.
    #[arg(long)]
    session: Option<String>,

    /// Filter by tool name.
    #[arg(long)]
    tool: Option<String>,

    /// Filter Bash commands containing text.
    #[arg(long)]
    command_contains: Option<String>,

    /// Filter errors containing text.
    #[arg(long)]
    error_contains: Option<String>,

    /// Match file paths touched by tool calls.
    #[arg(long)]
    file_path: Option<String>,

    /// Search transcript message content.
    #[arg(long)]
    content_contains: Option<String>,

    /// Minimum duration in milliseconds.
    #[arg(long)]
    min_duration: Option<i64>,

    /// Maximum duration in milliseconds.
    #[arg(long)]
    max_duration: Option<i64>,

    /// Filter by status.
    #[arg(long)]
    status: Option<String>,

    /// Max results.
    #[arg(long, default_value_t = 50)]
    limit: usize,

    /// Output format: table or json.
    #[arg(long, default_value = "table")]
    format: String,

    /// Aggregate rows per session.
    #[arg(long)]
    group_by_session: bool,
}

#[derive(Debug, Args)]
struct InspectArgs {
    /// Session ID (required).
    #[arg(long)]
    session: String,

    /// Filter to a specific tool use ID.
    #[arg(long)]
    tool_use_id: Option<String>,

    /// Number of surrounding runs to include.
    #[arg(long)]
    context: Option<usize>,

    /// Filter runs by status.
    #[arg(long)]
    status: Option<String>,

    /// Include surrounding message content for context.
    #[arg(long)]
    with_messages: bool,

    /// Output format: table or json.
    #[arg(long, default_value = "table")]
    format: String,
}

#[derive(Debug, Args)]
struct CompareArgs {
    /// Left cohort session ID (required, repeatable).
    #[arg(long, required = true)]
    left_session: Vec<String>,

    /// Right cohort session ID (required, repeatable).
    #[arg(long, required = true)]
    right_session: Vec<String>,

    /// Filter by tool name.
    #[arg(long)]
    tool: Option<String>,

    /// Filter commands containing text.
    #[arg(long)]
    command_contains: Option<String>,

    /// Group by field.
    #[arg(long, default_value = "tool_name")]
    group_by: String,

    /// Output format: table or json.
    #[arg(long, default_value = "table")]
    format: String,
}

#[derive(Debug, Args)]
struct AggregateArgs {
    /// Filter by project alias.
    #[arg(long)]
    project: Option<String>,

    /// Only sessions after this date.
    #[arg(long)]
    since: Option<String>,

    /// Filter by tool name.
    #[arg(long)]
    tool: Option<String>,

    /// Comma-separated fields: tool_name, status.
    #[arg(long, default_value = "tool_name")]
    group_by: String,

    /// Output format: table or json.
    #[arg(long, default_value = "table")]
    format: String,
}

#[derive(Debug, Args)]
struct AuditArgs {
    /// Audit a specific JSONL file.
    #[arg(long)]
    file: Option<PathBuf>,

    /// Audit by session ID.
    #[arg(long)]
    session: Option<String>,

    /// Audit recent sessions in a project.
    #[arg(long)]
    project: Option<String>,

    /// Max sessions to audit.
    #[arg(long, default_value_t = 20)]
    limit: usize,

    /// Output format: table or json.
    #[arg(long, default_value = "table")]
    format: String,
}

#[derive(Debug, Args)]
struct ErrorsArgs {
    /// Filter by project alias.
    #[arg(long)]
    project: Option<String>,

    /// Filter by session ID.
    #[arg(long)]
    session: Option<String>,

    /// Only errors after this date.
    #[arg(long)]
    since: Option<String>,

    /// Filter by tool name.
    #[arg(long)]
    tool: Option<String>,

    /// Max error patterns.
    #[arg(long, default_value_t = 20)]
    top: usize,

    /// Add category and preventability.
    #[arg(long)]
    classify: bool,

    /// Output format: table or json.
    #[arg(long, default_value = "table")]
    format: String,
}

#[derive(Debug, Args)]
struct HealthArgs {
    /// Analyze a single session.
    #[arg(long)]
    session: Option<String>,

    /// Filter by project alias.
    #[arg(long)]
    project: Option<String>,

    /// Only sessions after this date.
    #[arg(long)]
    since: Option<String>,

    /// Max session rows.
    #[arg(long, default_value_t = 50)]
    limit: usize,

    /// Output format: table or json.
    #[arg(long, default_value = "table")]
    format: String,
}

#[derive(Debug, Args)]
struct SequencesArgs {
    /// Filter by project alias.
    #[arg(long)]
    project: Option<String>,

    /// Only sessions after this date.
    #[arg(long)]
    since: Option<String>,

    /// Minimum sequence length.
    #[arg(long, default_value_t = 3)]
    min_length: usize,

    /// Maximum sequence length.
    #[arg(long, default_value_t = 5)]
    max_length: usize,

    /// Minimum occurrences to report.
    #[arg(long, default_value_t = 3)]
    min_occurrences: usize,

    /// Include recovery analysis.
    #[arg(long)]
    recovery: bool,

    /// Output format: table or json.
    #[arg(long, default_value = "table")]
    format: String,
}

#[derive(Debug, Parser)]
struct ProjectsCommand {
    #[command(subcommand)]
    command: ProjectSubcommand,
}

#[derive(Debug, Subcommand)]
enum ProjectSubcommand {
    /// List configured projects.
    List,
    /// Add a project alias.
    Add(ProjectAddArgs),
    /// Remove a project alias.
    Remove(ProjectRemoveArgs),
    /// Rename a project alias.
    Alias(ProjectAliasArgs),
}

#[derive(Debug, Args)]
struct ProjectAddArgs {
    /// Alias used in output and filters.
    alias: String,

    /// Project path.
    path: PathBuf,
}

#[derive(Debug, Args)]
struct ProjectRemoveArgs {
    /// Alias to remove.
    alias: String,
}

#[derive(Debug, Args)]
struct ProjectAliasArgs {
    /// Existing alias.
    old_alias: String,

    /// New alias.
    new_alias: String,
}

#[derive(Debug, Parser)]
struct ScanCommand {
    /// Scan a specific JSONL transcript file. Repeatable.
    #[arg(long, global = true)]
    file: Vec<PathBuf>,

    /// Scan every JSONL under a transcript root, including subagents. Repeatable.
    #[arg(long, global = true)]
    root: Vec<PathBuf>,

    /// Skip subagent transcripts when walking roots.
    #[arg(long, global = true)]
    no_subagents: bool,

    #[command(subcommand)]
    command: Option<ScanSubcommand>,
}

#[derive(Debug, Subcommand)]
enum ScanSubcommand {
    /// Search tool call runs and transcript content.
    Search(ScanSearchArgs),
    /// Inspect tool call runs for a specific session.
    Inspect(InspectArgs),
    /// Compare tool runs between session cohorts.
    Compare(CompareArgs),
    /// Aggregate tool usage across sessions.
    Aggregate(AggregateArgs),
    /// Audit transcript completeness.
    Audit(ScanAuditArgs),
    /// Analyze tool call errors.
    Errors(ErrorsArgs),
    /// Analyze transcript token health.
    Health(HealthArgs),
    /// Find tool call patterns and retries.
    Sequences(SequencesArgs),
    /// Score how often files are opened via the Read tool.
    ReadScores(ReadScoresArgs),
}

#[derive(Debug, Args)]
struct ScanSearchArgs {
    /// Filter by project alias.
    #[arg(long)]
    project: Option<String>,

    /// Filter by worktree name.
    #[arg(long)]
    worktree: Option<String>,

    /// Filter by session id (matches external session id, internal id, or parent).
    #[arg(long)]
    session: Option<String>,

    /// Filter by tool name.
    #[arg(long)]
    tool: Option<String>,

    /// Filter Bash commands containing text.
    #[arg(long)]
    command_contains: Option<String>,

    /// Filter errors containing text.
    #[arg(long)]
    error_contains: Option<String>,

    /// Match file paths touched or referenced by the tool call.
    #[arg(long)]
    file_path: Option<String>,

    /// Search transcript message content (substring, case-insensitive).
    #[arg(long)]
    content_contains: Option<String>,

    /// Minimum duration in milliseconds.
    #[arg(long)]
    min_duration: Option<i64>,

    /// Maximum duration in milliseconds.
    #[arg(long)]
    max_duration: Option<i64>,

    /// Filter by status (`completed`, `error`, `ongoing`, `orphan`).
    #[arg(long)]
    status: Option<String>,

    /// Only sessions/runs after this date or timestamp.
    #[arg(long)]
    since: Option<String>,

    /// Max results.
    #[arg(long, default_value_t = 50)]
    limit: usize,

    /// Output format: table or json.
    #[arg(long, default_value = "table")]
    format: String,

    /// Aggregate rows per session.
    #[arg(long)]
    group_by_session: bool,
}

#[derive(Debug, Args)]
struct ScanAuditArgs {
    /// Max files to audit when walking roots.
    #[arg(long, default_value_t = 20)]
    limit: usize,

    /// Output format: table or json.
    #[arg(long, default_value = "table")]
    format: String,
}

#[derive(Debug, Args)]
struct ReadScoresArgs {
    /// Recency half-life in days; a read this many days old counts half as much.
    #[arg(long, default_value_t = 30.0)]
    half_life_days: f64,

    /// Only include file paths under this prefix (after worktree normalization).
    #[arg(long)]
    under: Option<String>,

    /// Only include files with this extension, without the dot (e.g. `md`).
    #[arg(long)]
    ext: Option<String>,

    /// Max files to emit, most-read first (0 = all).
    #[arg(long, default_value_t = 0)]
    limit: usize,

    /// Output format: table or json.
    #[arg(long, default_value = "json")]
    format: String,
}

#[derive(Debug, Args)]
struct InitArgs {
    /// Claude projects directory to scan.
    #[arg(long)]
    claude_projects: Option<PathBuf>,

    /// Write discovered projects without prompting.
    #[arg(long)]
    yes: bool,
}

fn run_transcripts(
    command: TranscriptsCommand,
    db_path: PathBuf,
    config: Config,
    cancel: &AtomicBool,
) -> Result<()> {
    match command.command {
        Some(TranscriptSubcommand::Sync(args)) => sync(args, db_path, &config, cancel),
        Some(TranscriptSubcommand::Search(args)) => search(args, db_path),
        Some(TranscriptSubcommand::Inspect(args)) => inspect(args, db_path),
        Some(TranscriptSubcommand::Compare(args)) => compare(args, db_path),
        Some(TranscriptSubcommand::Aggregate(args)) => aggregate(args, db_path),
        Some(TranscriptSubcommand::Audit(args)) => audit(args, db_path),
        Some(TranscriptSubcommand::Errors(args)) => errors(args, db_path),
        Some(TranscriptSubcommand::Health(args)) => health(args, db_path),
        Some(TranscriptSubcommand::Sequences(args)) => sequences(args, db_path),
        None => {
            print_transcripts_index();
            Ok(())
        }
    }
}

fn inspect(args: InspectArgs, db_path: PathBuf) -> Result<()> {
    let conn = db::open(&db_path)?;
    let runs = analytics::inspect_runs(
        &conn,
        &args.session,
        args.tool_use_id.as_deref(),
        args.status.as_deref(),
        args.context,
    )?;
    if args.with_messages {
        let context = analytics::message_context(&conn, &runs)?;
        let payload = InspectWithMessages { runs, context };
        output(&payload, &args.format, || {
            print_inspect_with_messages(&payload)
        })
    } else {
        output(&runs, &args.format, || print_inspect_runs(&runs))
    }
}

#[derive(Debug, Serialize)]
struct InspectWithMessages {
    runs: Vec<db::ToolCallRun>,
    context: Vec<db::MessageHit>,
}

fn compare(args: CompareArgs, db_path: PathBuf) -> Result<()> {
    let conn = db::open(&db_path)?;
    let filters = analytics::RunFilters {
        tool: args.tool,
        command_contains: args.command_contains,
        ..analytics::RunFilters::default()
    };
    let result = analytics::compare(
        &conn,
        &args.left_session,
        &args.right_session,
        &filters,
        &args.group_by,
    )?;
    output(&result, &args.format, || {
        println!("Left cohort:");
        print_compare_groups(&result.left);
        println!("\nRight cohort:");
        print_compare_groups(&result.right);
    })
}

fn aggregate(args: AggregateArgs, db_path: PathBuf) -> Result<()> {
    let conn = db::open(&db_path)?;
    let filters = analytics::RunFilters {
        project: args.project,
        since: args.since,
        tool: args.tool,
        ..analytics::RunFilters::default()
    };
    let group_by = args
        .group_by
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let result = analytics::aggregate(&conn, &filters, &group_by)?;
    output(&result, &args.format, || print_aggregate(&result))
}

fn audit(args: AuditArgs, db_path: PathBuf) -> Result<()> {
    let conn = db::open(&db_path)?;
    let reports = if let Some(file) = args.file {
        vec![audit_file(&conn, &file)?]
    } else if let Some(session_id) = args.session {
        let session = db::find_session(&conn, &session_id)?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {session_id}"))?;
        vec![audit_session(&conn, &session)?]
    } else if let Some(project) = args.project {
        db::list_sessions(&conn)?
            .into_iter()
            .filter(|session| session.project_alias == project)
            .take(args.limit)
            .map(|session| audit_session(&conn, &session))
            .collect::<Result<Vec<_>>>()?
    } else {
        print_audit_usage();
        return Ok(());
    };

    output(&reports, &args.format, || print_audit_reports(&reports))
}

#[derive(Debug, Serialize)]
struct AuditReport {
    session_id: String,
    file: String,
    jsonl_lines: i64,
    imported_messages: i64,
    missing: i64,
    parse_errors: i64,
    jsonl_types: BTreeMap<String, i64>,
}

fn audit_file(conn: &rusqlite::Connection, file: &Path) -> Result<AuditReport> {
    let parsed = jsonl::parse_session_file(file)
        .with_context(|| format!("failed to parse transcript {}", file.display()))?;
    let session_id = parsed
        .session_id
        .clone()
        .or_else(|| {
            file.file_stem()
                .and_then(|stem| stem.to_str())
                .map(ToString::to_string)
        })
        .context("could not determine session id")?;
    let imported_messages =
        db::find_session(conn, &session_id)?.map_or(0, |session| session.message_count);
    let jsonl_lines = db::count_jsonl_lines(file)?;
    Ok(AuditReport {
        session_id,
        file: file.display().to_string(),
        jsonl_lines,
        imported_messages,
        missing: jsonl_lines - imported_messages,
        parse_errors: 0,
        jsonl_types: parsed
            .messages
            .into_iter()
            .fold(BTreeMap::new(), |mut acc, message| {
                *acc.entry(message.normalized_type).or_default() += 1;
                acc
            }),
    })
}

fn audit_session(conn: &rusqlite::Connection, session: &db::SessionRecord) -> Result<AuditReport> {
    let path = PathBuf::from(&session.transcript_path);
    if path.exists() {
        audit_file(conn, &path)
    } else {
        Ok(AuditReport {
            session_id: session.external_session_id.clone(),
            file: session.transcript_path.clone(),
            jsonl_lines: 0,
            imported_messages: session.message_count,
            missing: 0 - session.message_count,
            parse_errors: 0,
            jsonl_types: BTreeMap::new(),
        })
    }
}

fn errors(args: ErrorsArgs, db_path: PathBuf) -> Result<()> {
    let conn = db::open(&db_path)?;
    let filters = analytics::RunFilters {
        project: args.project,
        session: args.session,
        since: args.since,
        tool: args.tool,
        ..analytics::RunFilters::default()
    };
    let result = analytics::error_analysis(&conn, &filters, args.top, args.classify)?;
    output(&result, &args.format, || {
        print_errors(&result, args.classify)
    })
}

fn health(args: HealthArgs, db_path: PathBuf) -> Result<()> {
    let conn = db::open(&db_path)?;
    if let Some(session) = args.session {
        let report = analytics::health_session(&conn, &session)?;
        output(&report, &args.format, || {
            print_health_report(&session, &report)
        })
    } else {
        let report = analytics::health_project(
            &conn,
            args.project.as_deref(),
            args.since.as_deref(),
            args.limit,
        )?;
        output(&report, &args.format, || print_project_health(&report))
    }
}

fn sequences(args: SequencesArgs, db_path: PathBuf) -> Result<()> {
    let conn = db::open(&db_path)?;
    let filters = analytics::RunFilters {
        project: args.project,
        since: args.since,
        ..analytics::RunFilters::default()
    };
    let result = analytics::sequence_analysis(
        &conn,
        &filters,
        args.min_length,
        args.max_length,
        args.min_occurrences,
        args.recovery,
    )?;
    output(&result, &args.format, || print_sequences(&result))
}

fn run_projects(command: ProjectsCommand, config_path: PathBuf, mut config: Config) -> Result<()> {
    match command.command {
        ProjectSubcommand::List => {
            if config.projects.is_empty() {
                println!("No projects configured.");
            } else {
                println!("alias | path");
                println!("------------");
                for project in config.projects {
                    println!("{} | {}", project.alias, project.path.display());
                }
            }
        }
        ProjectSubcommand::Add(args) => {
            config.upsert_project(args.alias.clone(), args.path.clone());
            config.write(&config_path)?;
            println!("Added project {} -> {}", args.alias, args.path.display());
        }
        ProjectSubcommand::Remove(args) => {
            if config.remove_project(&args.alias) {
                config.write(&config_path)?;
                println!("Removed project {}", args.alias);
            } else {
                println!("Project not found: {}", args.alias);
            }
        }
        ProjectSubcommand::Alias(args) => {
            if config.rename_project(&args.old_alias, args.new_alias.clone()) {
                config.write(&config_path)?;
                println!("Renamed project {} -> {}", args.old_alias, args.new_alias);
            } else {
                println!("Could not rename project {}", args.old_alias);
            }
        }
    }
    Ok(())
}

fn init(args: InitArgs, config_path: PathBuf) -> Result<()> {
    let root = args
        .claude_projects
        .unwrap_or_else(default_claude_projects_dir);
    let projects = discover_project_cwds(&root);
    let config = if args.yes {
        config_from_projects(root, projects)
    } else {
        prompt_init_config(root, &projects)?
    };

    if config.projects.is_empty() {
        println!("No projects selected; config was not written.");
        return Ok(());
    }

    config.write(&config_path)?;
    println!(
        "Wrote {} project(s) to {}",
        config.projects.len(),
        config_path.display()
    );
    Ok(())
}

fn config_from_projects(root: PathBuf, projects: Vec<PathBuf>) -> Config {
    let mut config = Config::default();
    config.transcript_roots.push(root);
    for cwd in projects {
        config.upsert_project(default_alias(&cwd), cwd);
    }
    config
}

fn prompt_init_config(root: PathBuf, projects: &[PathBuf]) -> Result<Config> {
    let mut config = Config::default();
    config.transcript_roots.push(root);

    if projects.is_empty() {
        println!("No Claude Code projects found.");
        return Ok(config);
    }

    println!("Discovered {} project(s):", projects.len());
    for (index, project) in projects.iter().enumerate() {
        println!("  {}. {}", index + 1, project.display());
    }

    println!("Select projects to track: enter `all`, comma-separated numbers, or blank for all.");
    print!("Selection [all]: ");
    io::stdout().flush()?;

    let selection = read_line()?.trim().to_string();
    let selected_indices = parse_selection(&selection, projects.len())?;

    for index in selected_indices {
        let cwd = projects[index].clone();
        let default = default_alias(&cwd);
        print!("Alias for {} [{}]: ", cwd.display(), default);
        io::stdout().flush()?;
        let alias = read_line()?.trim().to_string();
        let alias = if alias.is_empty() { default } else { alias };
        config.upsert_project(alias, cwd);
    }

    Ok(config)
}

fn read_line() -> Result<String> {
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input)
}

fn parse_selection(input: &str, len: usize) -> Result<Vec<usize>> {
    if input.trim().is_empty() || input.trim().eq_ignore_ascii_case("all") {
        return Ok((0..len).collect());
    }

    let mut selected = Vec::new();
    for part in input
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let number = part
            .parse::<usize>()
            .with_context(|| format!("invalid project selection: {part}"))?;
        if number == 0 || number > len {
            anyhow::bail!("project selection out of range: {number}");
        }
        let index = number - 1;
        if !selected.contains(&index) {
            selected.push(index);
        }
    }
    Ok(selected)
}

fn default_alias(cwd: &Path) -> String {
    cwd.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_string()
}

fn sync(args: SyncArgs, db_path: PathBuf, config: &Config, cancel: &AtomicBool) -> Result<()> {
    let mut conn = db::open(&db_path)?;

    if let Some(file) = args.file {
        let synced = sync_one(&mut conn, &file, config, cancel)?;
        println!("Synced {synced} session(s).");
    } else if let Some(root) = args.transcript_root {
        let mut synced = 0;
        let mut failures = Vec::new();
        for file in db::transcript_files_under(&root) {
            check_cancelled(cancel)?;
            synced +=
                sync_one_collecting_failures(&mut conn, &file, config, cancel, &mut failures)?;
        }
        finish_sync(synced, failures)?;
    } else if let Some(session) = args.session {
        let file = find_configured_session(config, &session).with_context(|| {
            format!("session not found in configured transcript roots: {session}")
        })?;
        let synced = sync_one(&mut conn, &file, config, cancel)?;
        println!("Synced {synced} session(s).");
    } else if !config.transcript_roots.is_empty() {
        let mut synced = 0;
        let mut failures = Vec::new();
        for root in &config.transcript_roots {
            for file in db::transcript_files_under(root) {
                check_cancelled(cancel)?;
                synced +=
                    sync_one_collecting_failures(&mut conn, &file, config, cancel, &mut failures)?;
            }
        }
        finish_sync(synced, failures)?;
    } else {
        print_sync_usage();
    }

    Ok(())
}

#[derive(Debug)]
struct SyncFailure {
    file: PathBuf,
    error: anyhow::Error,
}

impl SyncFailure {
    fn new(file: PathBuf, error: anyhow::Error) -> Self {
        Self { file, error }
    }
}

fn sync_one(
    conn: &mut rusqlite::Connection,
    file: &Path,
    config: &Config,
    cancel: &AtomicBool,
) -> Result<usize> {
    let record = sync_file(conn, file, config, None, cancel)?;
    check_cancelled(cancel)?;
    println!(
        "Sync for session {}: ok, {} messages.",
        record.external_session_id, record.message_count
    );
    let subagents = sync_subagents(conn, &record, config, cancel)?;
    Ok(1 + subagents)
}

fn sync_one_collecting_failures(
    conn: &mut rusqlite::Connection,
    file: &Path,
    config: &Config,
    cancel: &AtomicBool,
    failures: &mut Vec<SyncFailure>,
) -> Result<usize> {
    let record = match sync_file(conn, file, config, None, cancel) {
        Ok(record) => record,
        Err(error) => {
            failures.push(SyncFailure::new(file.to_path_buf(), error));
            return Ok(0);
        }
    };
    check_cancelled(cancel)?;
    println!(
        "Sync for session {}: ok, {} messages.",
        record.external_session_id, record.message_count
    );
    let subagents = sync_subagents_collecting_failures(conn, &record, config, cancel, failures)?;
    Ok(1 + subagents)
}

fn finish_sync(synced: usize, failures: Vec<SyncFailure>) -> Result<()> {
    println!("Synced {synced} session(s).");
    if failures.is_empty() {
        return Ok(());
    }

    for failure in &failures {
        eprintln!(
            "Failed to sync {}: {:#}",
            failure.file.display(),
            failure.error
        );
    }
    anyhow::bail!(
        "synced {synced} session(s), failed {} transcript(s)",
        failures.len()
    )
}

fn sync_subagents_collecting_failures(
    conn: &mut rusqlite::Connection,
    parent: &db::SessionRecord,
    config: &Config,
    cancel: &AtomicBool,
    failures: &mut Vec<SyncFailure>,
) -> Result<usize> {
    let subagent_dir = Path::new(&parent.transcript_path)
        .with_file_name(&parent.external_session_id)
        .join("subagents");
    if !subagent_dir.exists() {
        return Ok(0);
    }

    let mut synced = 0;
    for entry in std::fs::read_dir(&subagent_dir)
        .with_context(|| format!("failed to read {}", subagent_dir.display()))?
    {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            check_cancelled(cancel)?;
            match sync_file(conn, &path, config, Some(&parent.id), cancel) {
                Ok(record) => {
                    println!(
                        "Sync for subagent {}: ok, {} messages.",
                        record.agent_id.as_deref().unwrap_or("unknown"),
                        record.message_count
                    );
                    synced += 1;
                }
                Err(error) => failures.push(SyncFailure::new(path, error)),
            }
        }
    }
    Ok(synced)
}

fn sync_file(
    conn: &mut rusqlite::Connection,
    file: &Path,
    config: &Config,
    parent_session_id: Option<&str>,
    cancel: &AtomicBool,
) -> Result<db::SessionRecord> {
    check_cancelled(cancel)?;
    let scanned = if parent_session_id.is_some() {
        jsonl::scan_subagent_file(file)
    } else {
        jsonl::scan_session_file(file)
    }
    .with_context(|| format!("failed to scan transcript {}", file.display()))?;
    let project_alias = config.alias_for_cwd(scanned.cwd.as_deref());
    let project_path = scanned
        .cwd
        .as_ref()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    db::ingest_session_file_with_cancel(
        conn,
        file,
        &project_alias,
        &project_path,
        parent_session_id,
        cancel,
    )
}

fn sync_subagents(
    conn: &mut rusqlite::Connection,
    parent: &db::SessionRecord,
    config: &Config,
    cancel: &AtomicBool,
) -> Result<usize> {
    let subagent_dir = Path::new(&parent.transcript_path)
        .with_file_name(&parent.external_session_id)
        .join("subagents");
    if !subagent_dir.exists() {
        return Ok(0);
    }

    let mut synced = 0;
    for entry in std::fs::read_dir(&subagent_dir)
        .with_context(|| format!("failed to read {}", subagent_dir.display()))?
    {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            check_cancelled(cancel)?;
            let record = sync_file(conn, &path, config, Some(&parent.id), cancel)?;
            println!(
                "Sync for subagent {}: ok, {} messages.",
                record.agent_id.as_deref().unwrap_or("unknown"),
                record.message_count
            );
            synced += 1;
        }
    }
    Ok(synced)
}

fn find_configured_session(config: &Config, session: &str) -> Option<PathBuf> {
    config
        .transcript_roots
        .iter()
        .flat_map(|root| db::transcript_files_under(root))
        .find(|path| path.file_stem().and_then(|stem| stem.to_str()) == Some(session))
}

fn search(args: SearchArgs, db_path: PathBuf) -> Result<()> {
    let conn = db::open(&db_path)?;
    if let Some(content) = &args.content_contains {
        let hits = analytics::search_content(&conn, content, args.limit)?;
        return output(&hits, &args.format, || {
            if hits.is_empty() {
                println!("No results found.");
            } else {
                println!("session_id | ordinal | role | snippet");
                println!("-------------------------------------");
                for hit in &hits {
                    println!(
                        "{} | {} | {} | {}",
                        hit.external_session_id,
                        hit.ordinal,
                        hit.role.as_deref().unwrap_or("n/a"),
                        hit.snippet.replace('\n', " ")
                    );
                }
            }
        });
    }

    let filters = analytics::RunFilters {
        project: args.project,
        worktree: args.worktree,
        session: args.session,
        tool: args.tool,
        command_contains: args.command_contains,
        error_contains: args.error_contains,
        file_path: args.file_path,
        min_duration: args.min_duration,
        max_duration: args.max_duration,
        status: args.status,
        since: None,
        limit: Some(args.limit),
    };
    let runs = analytics::search_runs(&conn, &filters)?;

    if args.group_by_session {
        let groups = group_runs_by_session(&runs);
        output(&groups, &args.format, || print_session_groups(&groups))
    } else {
        output(&runs, &args.format, || print_runs(&runs))
    }
}

fn run_scan(command: ScanCommand, config: &Config, cancel: &AtomicBool) -> Result<()> {
    let ScanCommand {
        file,
        root,
        no_subagents,
        command,
    } = command;
    let Some(command) = command else {
        print_scan_index();
        return Ok(());
    };

    let targets = scan::collect_targets(&file, &root, no_subagents, config);
    if targets.is_empty() {
        anyhow::bail!(
            "no transcripts to scan: provide --file/--root, configure transcript_roots, or ensure ~/.claude/projects or ~/.claude_agents/projects exists"
        );
    }

    match command {
        ScanSubcommand::Search(args) => scan_search(args, &targets, config, cancel),
        ScanSubcommand::Inspect(args) => scan_inspect(args, &targets, config, cancel),
        ScanSubcommand::Compare(args) => scan_compare(args, &targets, config, cancel),
        ScanSubcommand::Aggregate(args) => scan_aggregate(args, &targets, config, cancel),
        ScanSubcommand::Audit(args) => scan_audit(args, &targets),
        ScanSubcommand::Errors(args) => scan_errors(args, &targets, config, cancel),
        ScanSubcommand::Health(args) => scan_health(args, &targets, config, cancel),
        ScanSubcommand::Sequences(args) => scan_sequences(args, &targets, config, cancel),
        ScanSubcommand::ReadScores(args) => scan_read_scores(args, &targets, config, cancel),
    }
}

fn load_scan_store(
    targets: &[PathBuf],
    config: &Config,
    cancel: &AtomicBool,
) -> Result<scan::Store> {
    let store = scan::load(targets, config, cancel)?;
    for (path, error) in &store.errors {
        eprintln!("Skipped {}: {:#}", path.display(), error);
    }
    Ok(store)
}

fn scan_search(
    args: ScanSearchArgs,
    targets: &[PathBuf],
    config: &Config,
    cancel: &AtomicBool,
) -> Result<()> {
    let store = load_scan_store(targets, config, cancel)?;

    if let Some(content) = args.content_contains.as_deref() {
        let hits = analytics::search_content_in(&store.messages, content, args.limit);
        return output(&hits, &args.format, || {
            if hits.is_empty() {
                println!("No results found.");
            } else {
                println!("session_id | ordinal | role | snippet");
                println!("-------------------------------------");
                for hit in &hits {
                    println!(
                        "{} | {} | {} | {}",
                        hit.external_session_id,
                        hit.ordinal,
                        hit.role.as_deref().unwrap_or("n/a"),
                        hit.snippet.replace('\n', " ")
                    );
                }
            }
        });
    }

    let filters = analytics::RunFilters {
        project: args.project,
        worktree: args.worktree,
        session: args.session,
        tool: args.tool,
        command_contains: args.command_contains,
        error_contains: args.error_contains,
        file_path: args.file_path,
        min_duration: args.min_duration,
        max_duration: args.max_duration,
        status: args.status,
        since: args.since,
        limit: Some(args.limit),
    };
    let mut runs = analytics::search_runs_in(store.runs, &filters);
    runs.sort_by(|left, right| {
        left.started_at
            .as_deref()
            .unwrap_or("")
            .cmp(right.started_at.as_deref().unwrap_or(""))
            .then_with(|| left.start_ordinal.cmp(&right.start_ordinal))
            .then_with(|| left.tool_use_id.cmp(&right.tool_use_id))
    });

    if args.group_by_session {
        let groups = group_runs_by_session(&runs);
        output(&groups, &args.format, || print_session_groups(&groups))
    } else {
        output(&runs, &args.format, || print_runs(&runs))
    }
}

fn scan_inspect(
    args: InspectArgs,
    targets: &[PathBuf],
    config: &Config,
    cancel: &AtomicBool,
) -> Result<()> {
    let store = load_scan_store(targets, config, cancel)?;
    let session = store
        .find_session(&args.session)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Session not found: {}", args.session))?;
    let runs = analytics::inspect_runs_in(
        &session,
        store.runs.clone(),
        args.tool_use_id.as_deref(),
        args.status.as_deref(),
        args.context,
    );
    if args.with_messages {
        let context = analytics::message_context_in(&store.messages, &runs);
        let payload = InspectWithMessages { runs, context };
        output(&payload, &args.format, || {
            print_inspect_with_messages(&payload)
        })
    } else {
        output(&runs, &args.format, || print_inspect_runs(&runs))
    }
}

fn scan_compare(
    args: CompareArgs,
    targets: &[PathBuf],
    config: &Config,
    cancel: &AtomicBool,
) -> Result<()> {
    let store = load_scan_store(targets, config, cancel)?;
    let filters = analytics::RunFilters {
        tool: args.tool,
        command_contains: args.command_contains,
        ..analytics::RunFilters::default()
    };
    let result = analytics::compare_in(
        store.runs,
        &args.left_session,
        &args.right_session,
        &filters,
        &args.group_by,
    );
    output(&result, &args.format, || {
        println!("Left cohort:");
        print_compare_groups(&result.left);
        println!("\nRight cohort:");
        print_compare_groups(&result.right);
    })
}

fn scan_aggregate(
    args: AggregateArgs,
    targets: &[PathBuf],
    config: &Config,
    cancel: &AtomicBool,
) -> Result<()> {
    let store = load_scan_store(targets, config, cancel)?;
    let filters = analytics::RunFilters {
        project: args.project,
        since: args.since,
        tool: args.tool,
        ..analytics::RunFilters::default()
    };
    let group_by = args
        .group_by
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let result = analytics::aggregate_in(store.runs, &filters, &group_by);
    output(&result, &args.format, || print_aggregate(&result))
}

fn scan_audit(args: ScanAuditArgs, targets: &[PathBuf]) -> Result<()> {
    let mut reports = Vec::new();
    for path in targets.iter().take(args.limit) {
        reports.push(scan::audit_file(path)?);
    }
    output(&reports, &args.format, || print_scan_audit_reports(&reports))
}

fn scan_errors(
    args: ErrorsArgs,
    targets: &[PathBuf],
    config: &Config,
    cancel: &AtomicBool,
) -> Result<()> {
    let store = load_scan_store(targets, config, cancel)?;
    let filters = analytics::RunFilters {
        project: args.project,
        session: args.session,
        since: args.since,
        tool: args.tool,
        ..analytics::RunFilters::default()
    };
    let result = analytics::error_analysis_in(store.runs, &filters, args.top, args.classify);
    output(&result, &args.format, || {
        print_errors(&result, args.classify)
    })
}

fn scan_health(
    args: HealthArgs,
    targets: &[PathBuf],
    config: &Config,
    cancel: &AtomicBool,
) -> Result<()> {
    let store = load_scan_store(targets, config, cancel)?;
    if let Some(session_id) = args.session {
        let session = store
            .find_session(&session_id)
            .ok_or_else(|| anyhow::anyhow!("Session not found: {session_id}"))?;
        let usage = store
            .usage_by_session
            .iter()
            .find(|(record, _)| record.id == session.id)
            .map(|(_, messages)| messages.as_slice())
            .unwrap_or(&[]);
        let report = analytics::health_session_in(usage);
        output(&report, &args.format, || {
            print_health_report(&session_id, &report)
        })
    } else {
        let report = analytics::health_project_in(
            store.usage_by_session,
            args.project.as_deref(),
            args.since.as_deref(),
            args.limit,
        );
        output(&report, &args.format, || print_project_health(&report))
    }
}

fn scan_sequences(
    args: SequencesArgs,
    targets: &[PathBuf],
    config: &Config,
    cancel: &AtomicBool,
) -> Result<()> {
    let store = load_scan_store(targets, config, cancel)?;
    let filters = analytics::RunFilters {
        project: args.project,
        since: args.since,
        ..analytics::RunFilters::default()
    };
    let result = analytics::sequence_analysis_in(
        store.runs,
        &filters,
        args.min_length,
        args.max_length,
        args.min_occurrences,
        args.recovery,
    );
    output(&result, &args.format, || print_sequences(&result))
}

fn scan_read_scores(
    args: ReadScoresArgs,
    targets: &[PathBuf],
    config: &Config,
    cancel: &AtomicBool,
) -> Result<()> {
    let store = load_scan_store(targets, config, cancel)?;
    let options = analytics::ReadScoreOptions {
        half_life_days: args.half_life_days,
        under: args.under,
        ext: args.ext,
        now: Utc::now(),
        limit: (args.limit > 0).then_some(args.limit),
    };
    let result = analytics::read_scores_in(store.runs, &options);
    output(&result, &args.format, || print_read_scores(&result))
}

fn print_read_scores(result: &analytics::ReadScoreResult) {
    println!(
        "Read Scores ({} files, {} reads, {}-day half-life):\n",
        result.file_count, result.total_reads, result.half_life_days
    );
    if result.files.is_empty() {
        println!("No reads found.");
        return;
    }
    println!("  score      reads   path");
    println!("  ----------------------------------------------------------");
    for file in &result.files {
        println!("  {:<10} {:<7} {}", file.score, file.reads, file.path);
    }
}

fn print_scan_audit_reports(reports: &[scan::AuditFileReport]) {
    for report in reports {
        println!("Session: {}", report.session_id);
        println!("  File: {}", report.file);
        println!("  JSONL lines:       {}", report.jsonl_lines);
        println!("  Parsed messages:   {}", report.parsed_messages);
        println!();
        println!("  JSONL types:");
        for (record_type, count) in &report.jsonl_types {
            println!("    {record_type:<25} {count}");
        }
    }
}

fn print_scan_index() {
    println!(
        "Spotter Scan (DB-less) CLI\n\nCommands:\n\n  spotter scan search      Search tool call runs and transcript content\n  spotter scan inspect     Inspect tool call runs for a specific session\n  spotter scan compare     Compare tool runs between session cohorts\n  spotter scan aggregate   Aggregate tool usage across sessions\n  spotter scan audit       Audit transcript JSONL completeness\n  spotter scan errors      Analyze tool call errors\n  spotter scan health      Analyze transcript token health\n  spotter scan sequences   Find tool call patterns and retries\n  spotter scan read-scores Score how often files are opened via Read\n\nScan-level options (apply to every subcommand):\n  --file <path>      Scan a specific JSONL transcript (repeatable)\n  --root <path>      Scan every JSONL under a transcript root (repeatable)\n  --no-subagents     Skip subagent transcripts when walking roots"
    );
}

#[derive(Debug, Serialize)]
struct SessionGroup {
    session_id: String,
    project_alias: String,
    worktree_name: Option<String>,
    matches: usize,
    ordinal_range: String,
    tool_use_ids: Vec<String>,
}

fn group_runs_by_session(runs: &[db::ToolCallRun]) -> Vec<SessionGroup> {
    let mut map = std::collections::BTreeMap::<String, Vec<&db::ToolCallRun>>::new();
    for run in runs {
        map.entry(run.session_id.clone()).or_default().push(run);
    }
    map.into_values()
        .map(|mut group| {
            group.sort_by(|left, right| {
                left.start_ordinal
                    .unwrap_or(i64::MAX)
                    .cmp(&right.start_ordinal.unwrap_or(i64::MAX))
                    .then_with(|| left.tool_use_id.cmp(&right.tool_use_id))
            });
            let first = group[0];
            let ordinals = group
                .iter()
                .flat_map(|run| [run.start_ordinal, run.end_ordinal])
                .flatten()
                .collect::<Vec<_>>();
            let ordinal_range = match (ordinals.iter().min(), ordinals.iter().max()) {
                (Some(min), Some(max)) => format!("{min}-{max}"),
                _ => "n/a".to_string(),
            };
            SessionGroup {
                session_id: first.external_session_id.clone(),
                project_alias: first.project_alias.clone(),
                worktree_name: first.worktree_name.clone(),
                matches: group.len(),
                ordinal_range,
                tool_use_ids: group.iter().map(|run| run.tool_use_id.clone()).collect(),
            }
        })
        .collect()
}

fn output<T, F>(value: &T, format: &str, print_table: F) -> Result<()>
where
    T: Serialize,
    F: FnOnce(),
{
    if format == "json" {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        print_table();
    }
    Ok(())
}

fn print_runs(runs: &[db::ToolCallRun]) {
    if runs.is_empty() {
        println!("No results found.");
        return;
    }

    println!("tool_use_id | tool_name | command | status | duration_ms");
    println!("---------------------------------------------------------");
    for run in runs {
        println!(
            "{} | {} | {} | {} | {}",
            run.tool_use_id,
            run.tool_name,
            run.command.as_deref().unwrap_or(""),
            run.status,
            run.duration_ms
                .map_or_else(|| "n/a".to_string(), |duration| duration.to_string())
        );
    }
}

fn print_session_groups(groups: &[SessionGroup]) {
    if groups.is_empty() {
        println!("No results found.");
        return;
    }

    println!("session_id | project | worktree | matches | ordinal_range | tool_use_ids");
    println!("--------------------------------------------------------------------------");
    for group in groups {
        println!(
            "{} | {} | {} | {} | {} | {}",
            group.session_id,
            group.project_alias,
            group.worktree_name.as_deref().unwrap_or("n/a"),
            group.matches,
            group.ordinal_range,
            group.tool_use_ids.join(",")
        );
    }
}

fn print_inspect_runs(runs: &[db::ToolCallRun]) {
    if runs.is_empty() {
        println!("No tool call runs found for this session.");
        return;
    }

    for run in runs {
        println!(
            "[{}] {} {} - {} ({}ms)",
            run.status,
            run.tool_name,
            run.tool_use_id,
            run.command.as_deref().unwrap_or("(no command)"),
            run.duration_ms
                .map_or_else(|| "?".to_string(), |duration| duration.to_string())
        );
    }
}

fn print_inspect_with_messages(payload: &InspectWithMessages) {
    print_inspect_runs(&payload.runs);
    if !payload.context.is_empty() {
        println!("\nMessages:");
        for message in &payload.context {
            println!(
                "  #{} {} {}",
                message.ordinal,
                message.role.as_deref().unwrap_or("n/a"),
                message.snippet.replace('\n', " ")
            );
        }
    }
}

fn print_compare_groups(groups: &[analytics::CompareGroup]) {
    if groups.is_empty() {
        println!("  (no data)");
        return;
    }

    for group in groups {
        println!(
            "  {}: count={}, avg_duration={}ms",
            group.key,
            group.count,
            group
                .avg_duration_ms
                .map_or_else(|| "n/a".to_string(), |duration| duration.to_string())
        );
    }
}

fn print_aggregate(result: &analytics::AggregateResult) {
    println!(
        "Tool Usage Summary ({} sessions, {} tool calls):\n",
        result.session_count, result.total_runs
    );
    println!("  Group                          Calls   Errors  Err%    Avg ms     P95 ms");
    println!("  -------------------------------------------------------------------------");
    for group in &result.groups {
        let key = group.key.values().cloned().collect::<Vec<_>>().join(", ");
        println!(
            "  {:<30} {:<7} {:<7} {:<7} {:<10} {:<10}",
            key,
            group.count,
            group.errors,
            format!("{}%", group.error_pct),
            group
                .avg_duration_ms
                .map_or_else(|| "-".to_string(), |duration| duration.to_string()),
            group
                .p95_duration_ms
                .map_or_else(|| "-".to_string(), |duration| duration.to_string())
        );
    }
    if !result.top_errors.is_empty() {
        println!("\nTop Errors:");
        for error in &result.top_errors {
            println!(
                "  {} {} - {} occurrences",
                error.tool_name, error.fingerprint, error.count
            );
        }
    }
}

fn print_audit_reports(reports: &[AuditReport]) {
    for report in reports {
        let missing_text = if report.missing > 0 {
            let pct = if report.jsonl_lines > 0 {
                report.missing as f64 / report.jsonl_lines as f64 * 100.0
            } else {
                0.0
            };
            format!("{} ({pct:.1}%)", report.missing)
        } else {
            "0".to_string()
        };
        println!("Session: {}", report.session_id);
        println!("  File: {}", report.file);
        println!("  JSONL lines:       {}", report.jsonl_lines);
        println!("  Imported messages: {}", report.imported_messages);
        println!("  MISSING:           {missing_text}");
        println!("  Parse errors:      {}", report.parse_errors);
        println!();
        println!("  JSONL types:");
        for (record_type, count) in &report.jsonl_types {
            println!("    {record_type:<25} {count}");
        }
    }
}

fn print_errors(result: &analytics::ErrorAnalysisResult, classify: bool) {
    if result.total_errors == 0 {
        println!("No errors found.");
        return;
    }

    println!(
        "Error Analysis ({} errors, {} patterns, showing {}):\n",
        result.total_errors,
        result.pattern_count,
        result.patterns.len()
    );
    for pattern in &result.patterns {
        println!("  {} - {} occurrences", pattern.tool_name, pattern.count);
        println!("    Fingerprint: {}", pattern.fingerprint);
        if classify {
            println!(
                "    Category:    {} ({})",
                pattern.category.as_deref().unwrap_or("n/a"),
                pattern.preventability.as_deref().unwrap_or("n/a")
            );
            println!(
                "    Error rate:  {}% of {} total calls",
                pattern
                    .error_rate
                    .map_or_else(|| "0".to_string(), |rate| rate.to_string()),
                pattern.total_tool_calls.unwrap_or(0)
            );
        }
        println!(
            "    First seen:  {}",
            pattern.first_seen.as_deref().unwrap_or("unknown")
        );
        println!(
            "    Last seen:   {}",
            pattern.last_seen.as_deref().unwrap_or("unknown")
        );
        println!(
            "    Sample:      {}",
            pattern.sample_error.replace('\n', " ")
        );
        println!();
    }
}

fn print_health_report(session: &str, report: &analytics::HealthReport) {
    println!("Health Report: {session}\n");
    println!("  Messages with usage: {}", report.message_count);
    println!(
        "  Cache window:        {} ({}s)",
        report.cache_window.tier, report.cache_window.idle_window_seconds
    );
    println!("  Continued session:   {}", report.summary.is_continued);
    println!("  Startup context:     {}", report.summary.startup_context);
    println!("  Peak context:        {}", report.summary.peak_context);
    println!("  Total input:         {}", report.summary.total_input);
    println!("  Total output:        {}", report.summary.total_output);
    println!("  Total cache read:    {}", report.summary.total_cache_read);
    println!(
        "  Total cache create:  {}",
        report.summary.total_cache_creation
    );
    println!("  Peak cache read:     {}", report.summary.peak_cache_read);
    println!(
        "  Peak cache create:   {}",
        report.summary.peak_cache_creation
    );
    println!("  Total waste:         {}", report.summary.total_waste);
    println!("  Cache misses:        {}", report.cache_misses.len());
    println!("  Token jumps:         {}", report.jumps.len());
}

fn print_project_health(report: &analytics::ProjectHealth) {
    println!("Health Summary ({} sessions):\n", report.session_count);
    println!("  Total cache misses:  {}", report.total_cache_misses);
    println!("  Total token jumps:   {}", report.total_jumps);
    println!("  Total waste tokens:  {}", report.total_waste_tokens);
    println!("  Peak context:        {}", report.peak_context);
    println!("  Total cache read:    {}", report.total_cache_read_tokens);
    println!(
        "  Total cache create:  {}",
        report.total_cache_creation_tokens
    );
    println!("  Peak cache read:     {}", report.peak_cache_read_tokens);
    println!(
        "  Peak cache create:   {}",
        report.peak_cache_creation_tokens
    );
}

fn print_sequences(result: &analytics::SequenceResult) {
    println!("Sequence Analysis ({} sessions):\n", result.session_count);
    if result.frequent_sequences.is_empty() {
        println!("No frequent sequences found.");
    } else {
        println!("Frequent Sequences:");
        for row in &result.frequent_sequences {
            println!("  {} - {}", row.pattern.join(" -> "), row.count);
        }
    }

    if !result.retry_patterns.is_empty() {
        println!("\nRetry Patterns:");
        for row in &result.retry_patterns {
            println!("  {} - {}", row.pattern, row.count);
        }
    }

    if let Some(recovery) = &result.recovery_stats {
        println!("\nRecovery Analysis:");
        for row in recovery {
            println!(
                "  {} errors={} retry={} recover={}",
                row.category, row.total_errors, row.retry_rate, row.recovery_rate
            );
        }
    }
}

fn print_audit_usage() {
    println!(
        "Usage: spotter transcripts audit [options]\n\nOptions:\n  --file <path>     Audit a specific JSONL file\n  --session <id>    Audit by session ID\n  --project <id>    Audit recent sessions in a project\n  --limit <n>       Max sessions to audit\n  --format <fmt>    table or json"
    );
}

fn default_claude_projects_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
        .join(".claude")
        .join("projects")
}

fn discover_project_cwds(root: &Path) -> Vec<PathBuf> {
    if !root.exists() {
        return Vec::new();
    }

    let mut projects = std::collections::BTreeSet::new();
    for file in db::transcript_files_under(root) {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        for line in text.lines().filter(|line| !line.trim().is_empty()).take(20) {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if let Some(cwd) = value.get("cwd").and_then(serde_json::Value::as_str) {
                projects.insert(PathBuf::from(cwd));
                break;
            }
        }
    }
    projects.into_iter().collect()
}

fn print_transcripts_index() {
    println!(
        "Spotter Transcript Analytics CLI\n\nCommands:\n\n  spotter transcripts sync       Import/re-import transcripts and derive tool call runs\n  spotter transcripts search     Search tool call runs across sessions with filters\n  spotter transcripts inspect    Inspect tool call runs for a specific session\n  spotter transcripts compare    Compare tool runs between two session cohorts\n  spotter transcripts aggregate  Aggregate tool usage across sessions\n  spotter transcripts audit      Audit transcript import completeness\n  spotter transcripts errors     Analyze tool call errors\n  spotter transcripts health     Analyze transcript token health\n  spotter transcripts sequences  Find tool call patterns and retries"
    );
}

fn print_sync_usage() {
    println!(
        "Usage: spotter transcripts sync [options]\n\nOptions:\n  --session <id>            Sync a specific session by external ID\n  --file <path>             Sync from a specific JSONL transcript file\n  --transcript-root <path>  Sync all sessions under a transcript root"
    );
}
