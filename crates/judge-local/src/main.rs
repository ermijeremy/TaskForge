use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use taskforge_execution::{ConfigError, ExecutionConfig};

#[derive(Parser)]
#[command(version, about = "TaskForge local judge configuration scaffold")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate configuration files and paths; does not run a sandbox.
    CheckConfig {
        /// JSON configuration; relative runtime paths use this file's directory.
        #[arg(short, long)]
        config: PathBuf,
    },
}

fn run(cli: Cli) -> Result<(), ConfigError> {
    match cli.command {
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
            eprintln!("Configuration error: {error}");
            ExitCode::FAILURE
        }
    }
}
