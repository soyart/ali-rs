#![feature(fs_try_exists)]
#![feature(exit_status_error)]

mod cli;
mod disks;
mod errors;
mod linux;
mod manifest;
mod utils;

use clap::Parser;

fn main() -> Result<(), errors::AyiError> {
    let args = cli::Args::parse();
    let manifest_yaml = std::fs::read_to_string(args.manifest).unwrap();
    let manifest = manifest::parse_manifest(&manifest_yaml).expect("failed to parse manifest yaml");

    println!("{:?}", manifest);

    Ok(())
}
