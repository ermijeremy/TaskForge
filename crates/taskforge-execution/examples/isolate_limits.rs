//! Limit-mapping demo only; never starts a sandbox or executes submitted source.
use std::process::ExitCode;
use taskforge_execution::{ExecutionLimits, sandbox::IsolateLimits};

fn main() -> ExitCode {
    match IsolateLimits::try_from(&ExecutionLimits::default()) {
        Ok(limits) => {
            for argument in limits.arguments() {
                println!("{argument}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Limit conversion: {error}");
            ExitCode::FAILURE
        }
    }
}
