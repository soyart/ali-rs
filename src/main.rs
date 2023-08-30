mod cli;
mod defaults;
mod entity;
mod errors;
mod linux;
mod manifest;
mod run;
mod utils;

use clap::Parser;

fn main() -> Result<(), errors::AliError> {
    let args = cli::Cli::parse();
    let manifest = args.manifest.clone();

    match run::run(args) {
        Err(err) => eprintln!("ali-rs: failed to apply manifest {manifest}: {err}"),
        Ok(()) => {
            println!("ali-rs: manifest {} applied succesfully", manifest);
        }
    };

    Ok(())
}
