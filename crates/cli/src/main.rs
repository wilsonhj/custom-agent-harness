//! `eng`: argument parsing, a terminal check for decisions, and calls into the engine. The
//! decisions themselves live in `harness-core`; nothing here interprets task state beyond
//! choosing an exit code.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod login;
mod output;
mod runtimes;

use std::io::{BufRead as _, IsTerminal as _, Write as _};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use harness_core::artifact::MediaType;
use harness_core::decide::{Command, Event, Task};
use harness_core::ids::{CommitSha, IdempotencyKey, Seq, TaskId};
use harness_core::lifecycle::{DecisionKind, Subject};
use harness_core::policy::Pipeline;
use harness_core::stage::StageRunState;
use harness_core::types::{Actor, Envelope, RepoRef, WIRE_VERSION};
use harness_engine::{
    DoctorCheck, Engine, EngineConfig, EngineError, doctor, encode_packet, git, new_id, now,
    packet_digest, require_runtimes, resolve_config,
};
use output::{Report, emit};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

#[derive(Parser)]
#[command(
    name = "eng",
    version,
    about = "Spec packet in, verified and reviewed change out"
)]
struct Cli {
    /// One JSON object on stdout instead of text.
    #[arg(long, global = true)]
    json: bool,
    /// State directory (default `~/Library/Application Support/eng`).
    #[arg(long, global = true, value_name = "PATH")]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Check sandbox, process tracking, pinned runtime versions, shim and state directory.
    Doctor,
    /// Create tasks.
    Task {
        #[command(subcommand)]
        cmd: TaskCmd,
    },
    /// Record a human decision (needs a terminal and the typed word `yes`).
    Decide {
        task: String,
        #[arg(value_enum)]
        action: Action,
    },
    /// Run a task until it awaits a human, fails or is cancelled.
    Run { task: String },
    /// Show one task, or list all tasks.
    Status { task: Option<String> },
    /// Cancel a task.
    Cancel { task: String },
    /// Audit the event log.
    Audit {
        #[command(subcommand)]
        cmd: AuditCmd,
    },
    /// Sign in to a runtime with its official CLI (the harness never reads the login).
    Login {
        #[arg(value_enum)]
        runtime: login::LoginRuntime,
    },
}

#[derive(Subcommand)]
enum TaskCmd {
    /// Create a task from a spec packet; prints the task id and packet digest.
    New {
        #[arg(long)]
        repo: PathBuf,
        /// Packet directory inside the repository, for example `specs/my-feature`.
        #[arg(long)]
        packet: String,
        /// Default: `.eng/pipeline.toml` inside the repository.
        #[arg(long)]
        pipeline: Option<PathBuf>,
        /// Default: the repository's HEAD.
        #[arg(long)]
        base: Option<String>,
    },
}

#[derive(Subcommand)]
enum AuditCmd {
    /// Recompute the hash chain and print the head sequence number.
    Verify,
}

#[derive(Clone, Copy, ValueEnum)]
enum Action {
    ApproveSpec,
    Accept,
    Reject,
    SendBack,
}

impl Action {
    fn kind(self) -> DecisionKind {
        match self {
            Action::ApproveSpec => DecisionKind::ApproveSpec,
            Action::Accept => DecisionKind::Accept,
            Action::Reject => DecisionKind::Reject,
            Action::SendBack => DecisionKind::SendBack,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Action::ApproveSpec => "approve-spec",
            Action::Accept => "accept",
            Action::Reject => "reject",
            Action::SendBack => "send-back",
        }
    }
}

/// Debug builds only: with `ENG_FORCE_SANDBOX_UNAVAILABLE=1`, re-execute under a trivial
/// Seatbelt profile. Profiles do not nest, so the engine's own sandbox probe then fails for
/// real, with no change to the engine.
#[cfg(debug_assertions)]
fn force_sandbox_unavailable() {
    use std::os::unix::process::CommandExt as _;
    const INNER: &str = "ENG_FORCED_INNER";
    if std::env::var_os("ENG_FORCE_SANDBOX_UNAVAILABLE").is_some_and(|v| v == "1")
        && std::env::var_os(INNER).is_none()
        && let Ok(exe) = std::env::current_exe()
    {
        let err = std::process::Command::new("/usr/bin/sandbox-exec")
            .args(["-p", "(version 1)(allow default)"])
            .arg(exe)
            .args(std::env::args_os().skip(1))
            .env(INNER, "1")
            .exec();
        eprintln!("eng: ENG_FORCE_SANDBOX_UNAVAILABLE: {err}");
        std::process::exit(output::INTERNAL);
    }
}

#[cfg(not(debug_assertions))]
fn force_sandbox_unavailable() {}

fn main() -> ExitCode {
    let cli = Cli::parse();
    force_sandbox_unavailable();
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("eng: {e}");
            return exit(output::INTERNAL);
        }
    };
    exit(rt.block_on(dispatch(cli)))
}

fn exit(code: i32) -> ExitCode {
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

async fn dispatch(cli: Cli) -> i32 {
    let json = cli.json;
    let state_dir = match cli
        .state_dir
        .clone()
        .map_or_else(runtimes::default_state_dir, Ok)
    {
        Ok(d) => d,
        Err(e) => return emit(json, &Report::err(e), "", output::INTERNAL),
    };
    let cfg = match resolve_config(state_dir.clone()) {
        Ok(c) => c,
        Err(e) => return engine_error(json, None, &e),
    };
    match cli.cmd {
        Cmd::Doctor => cmd_doctor(json, &cfg).await,
        Cmd::Login { runtime } => cmd_login(json, runtime, &cfg),
        Cmd::Decide { task, action } => {
            // The terminal check comes first: nothing is opened or read without one.
            if !std::io::stdin().is_terminal() {
                let r = Report::err("decisions need a terminal on stdin").task(&task, None);
                return emit(json, &r, "", output::REJECTED);
            }
            with_engine(json, &cfg, |e| cmd_decide(e, json, task, action)).await
        }
        Cmd::Task {
            cmd:
                TaskCmd::New {
                    repo,
                    packet,
                    pipeline,
                    base,
                },
        } => {
            with_engine(json, &cfg, |e| {
                cmd_task_new(e, json, repo, packet, pipeline, base)
            })
            .await
        }
        Cmd::Run { task } => {
            if let Err(e) = require_runtimes(&cfg) {
                return emit(
                    json,
                    &Report::err(format!("runtime_not_found: {e}")).task(&task, None),
                    "",
                    output::FAILED,
                );
            }
            with_engine(json, &cfg, |e| cmd_run(e, json, task)).await
        }
        Cmd::Status { task } => with_engine(json, &cfg, |e| cmd_status(e, json, task)).await,
        Cmd::Cancel { task } => with_engine(json, &cfg, |e| cmd_cancel(e, json, task)).await,
        Cmd::Audit {
            cmd: AuditCmd::Verify,
        } => {
            with_engine(json, &cfg, |e| async move {
                match e.verify_chain() {
                    Ok(head) => emit(
                        json,
                        &Report::ok().with("head_seq", json!(head.0)),
                        &format!("hash chain intact; head seq {}", head.0),
                        output::OK,
                    ),
                    Err(e) => engine_error(json, None, &e),
                }
            })
            .await
        }
    }
}

/// Creates the state directory, opens the engine (which recovers anything in flight), runs `f`.
async fn with_engine<F, Fut>(json: bool, cfg: &EngineConfig, f: F) -> i32
where
    F: FnOnce(Engine) -> Fut,
    Fut: std::future::Future<Output = i32>,
{
    if let Err(e) = runtimes::ensure_state_dir(&cfg.state_dir) {
        return emit(
            json,
            &Report::err(format!("state dir {}: {e}", cfg.state_dir.display())),
            "",
            output::INTERNAL,
        );
    }
    match Engine::open(cfg.clone()).await {
        Ok(engine) => f(engine).await,
        Err(e) => engine_error(json, None, &e),
    }
}

fn engine_error(json: bool, task: Option<&str>, e: &EngineError) -> i32 {
    let (code, tag) = match e {
        EngineError::Chain(_) => (output::FAILED, "chain_broken"),
        EngineError::KillFailed { .. } | EngineError::WorkspaceLocked { .. } => {
            (output::FAILED, "workspace_locked")
        }
        _ => (output::INTERNAL, "internal"),
    };
    let mut r = Report::err(format!("{tag}: {e}"));
    if let Some(t) = task {
        r = r.task(t, None);
    }
    emit(json, &r, "", code)
}

fn principal() -> String {
    std::env::var("USER")
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "owner".into())
}

fn envelope(task: &TaskId, actor: Actor, body: Command) -> Envelope<Command> {
    Envelope {
        v: WIRE_VERSION,
        id: new_id(),
        task: task.clone(),
        at: now(),
        actor,
        body,
    }
}

fn human(task: &TaskId, body: Command) -> Envelope<Command> {
    envelope(
        task,
        Actor::Human {
            principal: principal(),
        },
        body,
    )
}

fn key() -> IdempotencyKey {
    IdempotencyKey::new(new_id())
}

/// The wire name of a unit-like value (`"review_clean"`, `"verified"`).
fn wire_name(v: Result<Value, serde_json::Error>) -> Option<String> {
    match v {
        Ok(Value::String(s)) => Some(s),
        _ => None,
    }
}

fn state_name(t: &Task) -> Option<String> {
    wire_name(serde_json::to_value(t.state))
}

// ---- doctor, login ----

async fn cmd_doctor(json: bool, cfg: &EngineConfig) -> i32 {
    let checks = doctor(cfg).await;
    let ok = checks.iter().all(|c| c.ok);
    // A failing check's detail is `<code>: <explanation>`.
    let first = checks
        .iter()
        .find(|c| !c.ok)
        .map(|c| c.detail.split(':').next().unwrap_or("failed").to_owned());
    let mut report = match first {
        None => Report::ok(),
        Some(code) => Report::err(code),
    };
    report = report.with(
        "checks",
        Value::Array(checks.iter().map(check_json).collect()),
    );
    let text = checks.iter().map(check_line).collect::<Vec<_>>().join("\n");
    let code = if ok { output::OK } else { output::FAILED };
    // Failures are already in the lines; keep stderr quiet in text mode.
    if json {
        emit(json, &report, "", code)
    } else {
        println!("{text}");
        code
    }
}

fn check_line(c: &DoctorCheck) -> String {
    format!(
        "{}  {:<20} {}",
        if c.ok { "ok  " } else { "FAIL" },
        c.name,
        c.detail
    )
}

fn check_json(c: &DoctorCheck) -> Value {
    json!({"name": c.name, "ok": c.ok, "detail": c.detail})
}

fn cmd_login(json: bool, runtime: login::LoginRuntime, cfg: &EngineConfig) -> i32 {
    if json {
        return emit(
            json,
            &Report::err("login is interactive; --json is not supported"),
            "",
            output::USAGE,
        );
    }
    match login::plan(runtime, cfg) {
        Ok(plan) => {
            let err = login::exec(&plan);
            emit(
                json,
                &Report::err(format!("could not start {}: {err}", plan.program.display())),
                "",
                output::INTERNAL,
            )
        }
        Err(e) => emit(
            json,
            &Report::err(format!("runtime_not_found: {e}")),
            "",
            output::FAILED,
        ),
    }
}

// ---- task new ----

async fn cmd_task_new(
    engine: Engine,
    json: bool,
    repo: PathBuf,
    packet: String,
    pipeline: Option<PathBuf>,
    base: Option<String>,
) -> i32 {
    let usage = |msg: String| emit(json, &Report::err(msg), "", output::USAGE);
    let repo = match std::fs::canonicalize(&repo) {
        Ok(p) => p,
        Err(e) => return usage(format!("--repo {}: {e}", repo.display())),
    };
    let rev = format!("{}^{{commit}}", base.as_deref().unwrap_or("HEAD"));
    let base = match git(&repo, &["rev-parse", "--verify", "-q", &rev]).await {
        Ok(out) => match CommitSha::parse(String::from_utf8_lossy(&out).trim()) {
            Ok(c) => c,
            Err(e) => return usage(format!("base: {e}")),
        },
        Err(e) => return usage(format!("base {rev}: {e}")),
    };
    let common = match git(
        &repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .await
    {
        Ok(out) => String::from_utf8_lossy(&out).trim().to_owned(),
        Err(e) => return usage(format!("--repo is not a Git repository: {e}")),
    };
    let pipeline_path = match pipeline {
        Some(p) if p.is_absolute() => p,
        Some(p) => std::env::current_dir().unwrap_or_default().join(p),
        None => repo.join(".eng/pipeline.toml"),
    };
    let pipeline_bytes = match std::fs::read(&pipeline_path) {
        Ok(b) => b,
        Err(e) => return usage(format!("pipeline {}: {e}", pipeline_path.display())),
    };
    let parsed = std::str::from_utf8(&pipeline_bytes)
        .map_err(|_| "not UTF-8".to_owned())
        .and_then(|t| Pipeline::parse_toml(t).map_err(|e| e.to_string()));
    if let Err(e) = parsed {
        return emit(
            json,
            &Report::err(format!("pipeline {}: {e}", pipeline_path.display())),
            "",
            output::REJECTED,
        );
    }
    let (digest, files) = match packet_digest(&repo, &base, &packet).await {
        Ok(x) => x,
        Err(e) => return usage(format!("packet: {e}")),
    };
    let stored = engine
        .put_input(&encode_packet(&files), MediaType::Text)
        .and_then(|p| {
            engine
                .put_input(&pipeline_bytes, MediaType::Text)
                .map(|l| (p, l))
        });
    let (packet_id, pipeline_id) = match stored {
        Ok(x) => x,
        Err(e) => return engine_error(json, None, &e),
    };
    debug_assert_eq!(packet_id, digest);
    let task = TaskId::new(new_id());
    let cmd = Command::CreateTask {
        repo: RepoRef {
            path: repo.to_string_lossy().into_owned(),
            git_common_dir: common,
        },
        base_commit: base.clone(),
        packet: packet_id,
        pipeline: pipeline_id,
        key: key(),
    };
    match engine.submit(human(&task, cmd)).await {
        Ok(r) if r.accepted => {
            let t = engine.task(&task).ok().flatten();
            let report = Report::ok()
                .task(task.as_str(), t.as_ref().and_then(state_name))
                .with("packet", json!(digest.as_str()))
                .with("base", json!(base.as_str()));
            emit(
                json,
                &report,
                &format!("task {task}\npacket {digest}"),
                output::OK,
            )
        }
        Ok(r) => rejected(json, task.as_str(), r.rejection),
        Err(e) => engine_error(json, Some(task.as_str()), &e),
    }
}

fn rejected(json: bool, task: &str, why: Option<String>) -> i32 {
    let msg = format!("rejected: {}", why.unwrap_or_default());
    emit(
        json,
        &Report::err(msg).task(task, None),
        "",
        output::REJECTED,
    )
}

// ---- decide ----

async fn cmd_decide(engine: Engine, json: bool, task: String, action: Action) -> i32 {
    let id = TaskId::new(task.clone());
    let t = match engine.task(&id) {
        Ok(Some(t)) => t,
        Ok(None) => return rejected(json, &task, Some("no such task".into())),
        Err(e) => return engine_error(json, Some(&task), &e),
    };
    // Approval binds the packet digest; the other decisions bind the last checkpoint (before
    // any checkpoint, the packet digest, as `decide` requires).
    let subject = match (action, t.checkpoints.last()) {
        (Action::ApproveSpec, _) | (_, None) => Subject::PacketDigest(t.packet.clone()),
        (_, Some(cp)) => Subject::Commit(cp.commit.clone()),
    };
    let (what, value) = match &subject {
        Subject::PacketDigest(d) => ("packet digest", d.to_string()),
        Subject::Commit(c) => ("commit", c.to_string()),
    };
    eprintln!("Task {task}: {} the {what}\n  {value}", action.name());
    eprint!("Type yes to record this decision: ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line);
    if read.is_err() || line.trim() != "yes" {
        return emit(
            json,
            &Report::err("not confirmed; nothing recorded").task(&task, state_name(&t)),
            "",
            output::REJECTED,
        );
    }
    let cmd = Command::RecordHumanDecision {
        kind: action.kind(),
        subject,
        key: key(),
    };
    match engine.submit(human(&id, cmd)).await {
        Ok(r) if r.accepted => {
            let state = engine.task(&id).ok().flatten().and_then(|t| state_name(&t));
            let text = format!(
                "recorded {} for {task}; state {}",
                action.name(),
                state.clone().unwrap_or_default()
            );
            emit(json, &Report::ok().task(&task, state), &text, output::OK)
        }
        Ok(r) => rejected(json, &task, r.rejection),
        Err(e) => engine_error(json, Some(&task), &e),
    }
}

// ---- run, cancel ----

async fn cmd_run(engine: Engine, json: bool, task: String) -> i32 {
    let id = TaskId::new(task.clone());
    let t = match engine.task(&id) {
        Ok(Some(t)) => t,
        Ok(None) => return rejected(json, &task, Some("no such task".into())),
        Err(e) => return engine_error(json, Some(&task), &e),
    };
    // Effects already in flight (a run that was interrupted and recovered) just continue.
    if !matches!(t.run, StageRunState::EffectsInFlight { .. }) {
        match engine.submit(human(&id, Command::Run { key: key() })).await {
            Ok(r) if r.accepted => {}
            Ok(r) => return rejected(json, &task, r.rejection),
            Err(e) => return engine_error(json, Some(&task), &e),
        }
    }
    let cancel = CancellationToken::new();
    let watcher = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            watcher.cancel();
            eprintln!("eng: cancelling; press Ctrl-C again to kill");
            if tokio::signal::ctrl_c().await.is_ok() {
                std::process::exit(130);
            }
        }
    });
    let mut t = match engine.run_until_idle(&id, cancel.clone()).await {
        Ok(t) => t,
        Err(e) => return engine_error(json, Some(&task), &e),
    };
    if cancel.is_cancelled() {
        match engine
            .submit(human(&id, Command::Cancel { key: key() }))
            .await
        {
            Ok(_) => {}
            Err(e) => return engine_error(json, Some(&task), &e),
        }
        t = match engine.task(&id) {
            Ok(Some(t)) => t,
            Ok(None) => return rejected(json, &task, Some("no such task".into())),
            Err(e) => return engine_error(json, Some(&task), &e),
        };
    }
    outcome(&engine, json, &t)
}

/// Report and exit code for a task a `run` left behind.
fn outcome(engine: &Engine, json: bool, t: &Task) -> i32 {
    let id = t.id.as_str();
    let base = Report::ok()
        .task(id, state_name(t))
        .with("round", json!(t.round));
    match &t.run {
        StageRunState::AwaitingHuman { gate } => {
            let gate = wire_name(serde_json::to_value(gate)).unwrap_or_default();
            let text = format!(
                "task {id}: awaiting a human ({gate}); {}",
                summary_line(engine, t)
            );
            emit(
                json,
                &base.with("gate", json!(gate)),
                &text,
                output::AWAITING_HUMAN,
            )
        }
        StageRunState::Succeeded => emit(json, &base, &format!("task {id}: done"), output::OK),
        StageRunState::Failed { code } => {
            let code = wire_name(serde_json::to_value(code)).unwrap_or_else(|| "failed".into());
            let mut r = Report::err(code.clone()).task(id, state_name(t));
            r = r.with("round", json!(t.round));
            emit(
                json,
                &r,
                &format!("task {id}: failed ({code})"),
                output::FAILED,
            )
        }
        StageRunState::Cancelled => {
            let r = Report::err("cancelled").task(id, state_name(t));
            emit(json, &r, &format!("task {id}: cancelled"), output::FAILED)
        }
        StageRunState::Pending | StageRunState::EffectsInFlight { .. } => {
            let r = Report::err("unexpected_idle_state").task(id, state_name(t));
            emit(json, &r, "", output::INTERNAL)
        }
    }
}

fn summary_line(engine: &Engine, t: &Task) -> String {
    let f = findings(engine, &t.id);
    format!(
        "round {}, {} checkpoint(s), {} finding(s) in the last review ({} blocking)",
        t.round,
        t.checkpoints.len(),
        f["last_review_total"],
        f["last_review_blocking"]
    )
}

async fn cmd_cancel(engine: Engine, json: bool, task: String) -> i32 {
    let id = TaskId::new(task.clone());
    match engine
        .submit(human(&id, Command::Cancel { key: key() }))
        .await
    {
        Ok(r) if r.accepted => {
            let state = engine.task(&id).ok().flatten().and_then(|t| state_name(&t));
            emit(
                json,
                &Report::ok().task(&task, state),
                &format!("task {task}: cancelled"),
                output::OK,
            )
        }
        Ok(r) => rejected(json, &task, r.rejection),
        Err(e) => engine_error(json, Some(&task), &e),
    }
}

// ---- status ----

fn all_events(engine: &Engine) -> Vec<(TaskId, Event)> {
    engine
        .events_after(Seq(0), u32::MAX)
        .map(|v| {
            v.into_iter()
                .map(|r| (r.event.task, r.event.body))
                .collect()
        })
        .unwrap_or_default()
}

fn findings(engine: &Engine, task: &TaskId) -> Value {
    let (mut reviews, mut total, mut last_total, mut last_blocking) = (0u32, 0usize, 0usize, 0u32);
    for (t, ev) in all_events(engine) {
        if let (
            true,
            Event::FindingsRecorded {
                findings, blocking, ..
            },
        ) = (&t == task, ev)
        {
            reviews += 1;
            total += findings.len();
            last_total = findings.len();
            last_blocking = blocking;
        }
    }
    json!({
        "reviews": reviews,
        "total": total,
        "last_review_total": last_total,
        "last_review_blocking": last_blocking,
    })
}

async fn cmd_status(engine: Engine, json: bool, task: Option<String>) -> i32 {
    let Some(task) = task else {
        let mut ids: Vec<TaskId> = Vec::new();
        for (t, ev) in all_events(&engine) {
            if matches!(ev, Event::TaskCreated { .. }) {
                ids.push(t);
            }
        }
        let rows: Vec<Value> = ids
            .iter()
            .filter_map(|id| engine.task(id).ok().flatten())
            .map(|t| json!({"task": t.id.as_str(), "state": state_name(&t), "run": string_of_run(&t)}))
            .collect();
        let text = if rows.is_empty() {
            "no tasks".to_owned()
        } else {
            rows.iter()
                .map(|r| {
                    format!(
                        "{}  {}  {}",
                        r["task"].as_str().unwrap_or(""),
                        r["state"].as_str().unwrap_or(""),
                        r["run"].as_str().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        return emit(
            json,
            &Report::ok().with("tasks", Value::Array(rows)),
            &text,
            output::OK,
        );
    };
    let id = TaskId::new(task.clone());
    let t = match engine.task(&id) {
        Ok(Some(t)) => t,
        Ok(None) => return rejected(json, &task, Some("no such task".into())),
        Err(e) => return engine_error(json, Some(&task), &e),
    };
    let checkpoints: Vec<Value> = t
        .checkpoints
        .iter()
        .map(|c| json!({"n": c.n, "commit": c.commit.as_str()}))
        .collect();
    let gate = match &t.run {
        StageRunState::AwaitingHuman { gate } => wire_name(serde_json::to_value(gate)),
        _ => None,
    };
    let failure = match &t.run {
        StageRunState::Failed { code } => wire_name(serde_json::to_value(code)),
        _ => None,
    };
    let mut report = Report::ok()
        .task(&task, state_name(&t))
        .with("run", json!(string_of_run(&t)))
        .with("round", json!(t.round))
        .with("gate", json!(gate))
        .with("failure", json!(failure))
        .with("findings", findings(&engine, &id))
        .with("checkpoints", Value::Array(checkpoints))
        .with(
            "last_checkpoint",
            json!(t.checkpoints.last().map(|c| c.commit.as_str())),
        )
        .with("packet", json!(t.packet.as_str()))
        .with("base", json!(t.base_commit.as_str()));
    report.error = None;
    let text = format!(
        "task {task}\n  state {}\n  run {}\n  round {}\n  gate {}\n  {}\n  last checkpoint {}",
        state_name(&t).unwrap_or_default(),
        string_of_run(&t).unwrap_or_default(),
        t.round,
        gate.unwrap_or_else(|| "-".into()),
        summary_line(&engine, &t),
        t.checkpoints
            .last()
            .map_or("-".to_owned(), |c| c.commit.to_string()),
    );
    emit(json, &report, &text, output::OK)
}

fn string_of_run(t: &Task) -> Option<String> {
    match serde_json::to_value(&t.run) {
        Ok(v) => v.get("type").and_then(Value::as_str).map(str::to_owned),
        Err(_) => None,
    }
}
