mod cli;
mod disks;
mod errors;
mod linux;
mod manifest;
mod run;
mod utils;

use clap::Parser;

use crate::errors::NayiError;

fn main() -> Result<(), errors::NayiError> {
    let args = cli::Args::parse();

    match run::run(args) {
        Err(err) => eprintln!("nayi-rs failed: {err}"),
        Ok(report) => {
            println!("Report: {report:?}");
        }
    };

    Ok(())
}
