mod ali;
mod cli;
mod constants;
mod errors;
mod hooks;
mod linux;
mod run;
mod types;
mod utils;

use clap::Parser;

fn main() -> Result<(), errors::AliError> {
    let args = cli::Cli::parse();

    if let Err(err) = run::run(args) {
        eprintln!("{}", err.to_json_string());
    }

    Ok(())
}
