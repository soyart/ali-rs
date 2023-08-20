mod cli;
mod defaults;
mod disks;
mod entity;
mod errors;
mod linux;
mod manifest;
mod run;
mod utils;

use clap::Parser;

fn main() -> Result<(), errors::NayiError> {
    let args = cli::Args::parse();

    match run::run(args) {
        Err(err) => eprintln!("nayi-rs failed: {err}"),
        Ok(report) => {
            println!("{}", report.to_json_string());
        }
    };

    Ok(())
}
