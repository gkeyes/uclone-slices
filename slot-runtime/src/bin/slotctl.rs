#![doc = "Restricted `UClone` Slots Preview control-plane client binary."]

use std::process::ExitCode;

use clap::Parser;
use uclone_slot_runtime::cli::{Cli, run};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
