use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use taskforge_execution::{ConfigError, ExecutionConfig, ReadinessError, check_readiness};
use thiserror::Error;

#[derive(Parser)]
#[command(version, about = "TaskForge configuration and sandbox readiness")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Verify Linux/Isolate and run a built-in trusted sandbox smoke probe.
    CheckReady {
        #[arg(short, long)]
        config: PathBuf,
    },
    /// Validate configuration files and paths; does not run a sandbox.
    CheckConfig {
        /// JSON configuration; relative runtime paths use this file's directory.
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
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::CheckReady { config } => {
            let config = ExecutionConfig::load(config)?;
            let report = check_readiness(&config)?;
            println!("Sandbox readiness passed (trusted smoke probe; cleanup completed).");
            println!("{}", report.isolate_version);
            println!("{}", report.python_version);
            println!("Cgroup root: {}", report.cgroup_root.display());
            println!("Checked sandbox IDs: {:?}", report.checked_box_ids);
            println!("Smoke metadata:\n{}", report.smoke_metadata);
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
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
