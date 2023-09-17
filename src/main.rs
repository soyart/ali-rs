mod ali;
mod cli;
mod constants;
mod entity;
mod errors;
mod hooks;
mod linux;
mod run;
mod utils;

use clap::Parser;

fn main() -> Result<(), errors::AliError> {
    let args = cli::Cli::parse();
    let manifest = args.manifest.clone();

    if let Err(err) = run::run(args) {
        eprintln!(
            "ali-rs: failed to apply manifest {manifest}: {}",
            err.to_json_string()
        );
    }

    Ok(())
}
