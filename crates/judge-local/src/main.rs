use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use judge_local::{
    input::load_request,
    reporting::{ReportError, report_outcome},
};
use taskforge_execution::problem::PROBLEM_IDS;
use taskforge_execution::{
    ConfigError, EvaluationOutcome, ExecutionConfig, InfrastructureError, InfrastructureReason,
    IsolateRunner, ReadinessError, Runner, check_readiness,
};
use thiserror::Error;

#[derive(Parser)]
#[command(version, about = "TaskForge local JSON judging and sandbox readiness")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Evaluate a Python source against a fixed problem; emit one JSON result/error.
    Judge {
        #[arg(short, long, value_parser = clap::builder::PossibleValuesParser::new(PROBLEM_IDS))]
        problem: String,
        #[arg(short, long)]
        source: PathBuf,
        #[arg(short, long)]
        config: PathBuf,
        /// Trusted definitions directory, relative to the caller's working directory.
        #[arg(long, default_value = "fixtures/problems")]
        problems_dir: PathBuf,
    },
    /// Verify native Linux/Isolate with a built-in trusted probe.
    CheckReady {
        #[arg(short, long)]
        config: PathBuf,
        /// Emit a machine-readable readiness report/error instead of operator text.
        #[arg(long)]
        json: bool,
    },
    /// Validate configuration files and paths without running a sandbox.
    CheckConfig {
        #[arg(short, long)]
        config: PathBuf,
    },
}
#[derive(Debug, Error)]
enum CliError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),
    #[error("Sandbox readiness error: {0}")]
    Readiness(#[from] ReadinessError),
    #[error("{0}")]
    Report(#[from] ReportError),
    #[error("Cannot write CLI output: {0}")]
    Io(#[from] io::Error),
    #[error("Cannot serialize CLI output: {0}")]
    Json(#[from] serde_json::Error),
}
fn judge(
    problem: &str,
    source: &std::path::Path,
    config: &std::path::Path,
    problems_dir: &std::path::Path,
) -> EvaluationOutcome {
    let request = load_request(problem, source, config, problems_dir)?;
    let runner = IsolateRunner::new(request.config)?;
    let result = runner.evaluate(&request.source, &request.problem, request.problem.limits());
    match runner.shutdown() {
        Ok(()) => result,
        Err(cleanup) => {
            let context = match &result {
                Err(original) => format!(
                    "runner shutdown failed after {:?}: {}",
                    original.reason_code(),
                    cleanup.diagnostic()
                ),
                Ok(_) => format!("runner shutdown failed: {}", cleanup.diagnostic()),
            };
            Err(InfrastructureError::new(
                InfrastructureReason::CleanupFailed,
                &context,
            ))
        }
    }
}
fn ready_json(path: PathBuf) -> Result<u8, CliError> {
    let outcome = ExecutionConfig::load(path)
        .map_err(|_| {
            InfrastructureError::new(
                InfrastructureReason::InvalidConfiguration,
                "configuration is invalid; run check-config for details",
            )
        })
        .and_then(|config| {
            check_readiness(&config).map_err(|error| {
                let reason = if matches!(error, ReadinessError::Cleanup { .. }) {
                    InfrastructureReason::CleanupFailed
                } else {
                    InfrastructureReason::SandboxUnavailable
                };
                InfrastructureError::new(
                    reason,
                    "native readiness failed; run check-ready without --json for details",
                )
            })
        });
    let mut stdout = io::stdout().lock();
    let code = match outcome {
        Ok(report) => {
            serde_json::to_writer(&mut stdout, &report)?;
            0
        }
        Err(error) => {
            serde_json::to_writer(&mut stdout, &error)?;
            1
        }
    };
    stdout.write_all(b"\n")?;
    Ok(code)
}
fn run(cli: Cli) -> Result<u8, CliError> {
    match cli.command {
        Command::Judge {
            problem,
            source,
            config,
            problems_dir,
        } => {
            let report = report_outcome(judge(&problem, &source, &config, &problems_dir))?;
            let mut stdout = io::stdout().lock();
            stdout.write_all(report.json_bytes())?;
            stdout.write_all(b"\n")?;
            Ok(report.exit_code())
        }
        Command::CheckReady { config, json: true } => ready_json(config),
        Command::CheckReady {
            config,
            json: false,
        } => {
            let report = check_readiness(&ExecutionConfig::load(config)?)?;
            println!("Sandbox readiness passed (trusted smoke probe; cleanup completed).");
            println!("{}", report.isolate_version);
            println!("{}", report.python_version);
            println!("Cgroup root: {}", report.cgroup_root.display());
            println!("Checked sandbox IDs: {:?}", report.checked_box_ids);
            println!("Smoke metadata:\n{}", report.smoke_metadata);
            Ok(0)
        }
        Command::CheckConfig { config } => {
            let config = ExecutionConfig::load(config)?;
            let limits = config.limits();
            println!(
                "Configuration is valid (static checks only; sandbox readiness is not checked)."
            );
            println!("Python: {}", config.python_path().display());
            println!("Isolate: {}", config.isolate_path().display());
            println!("CPU per test: {} ms", limits.cpu_time().as_millis());
            println!("Wall time per test: {} ms", limits.wall_time().as_millis());
            println!("Memory per test: {} bytes", limits.memory_bytes());
            println!("Combined output per test: {} bytes", limits.output_bytes());
            println!("Submitted processes: {}", limits.processes());
            println!(
                "Whole evaluation deadline: {} ms",
                config.evaluation_timeout().as_millis()
            );
            Ok(0)
        }
    }
}
fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
