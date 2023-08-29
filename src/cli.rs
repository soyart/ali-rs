use clap::{Args, Parser, Subcommand};

use crate::errors::AliError;

#[derive(Debug, Parser)]
#[clap(
    author = "github.com/soyart",
    version,
    about = "Rust-based ALI installer"
)]
pub struct Cli {
    #[command(subcommand)]
    pub commands: Commands,

    /// Manifest file
    #[arg(
        global = true,
        short = 'f',
        long = "file",
        value_parser = validate_filename,
        default_value_t = String::from("./manifest.yaml")
    )]
    pub manifest: String,

    /// Dry-run, ali-rs will not commit any changes to disks,
    /// and will just print steps to be performed
    #[arg(global = true, short = 'n', default_value_t = false)]
    pub dry_run: bool,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Apply(ArgsApply),
    Validate,
}

#[derive(Debug, Args)]
pub struct ArgsApply {
    /// Do not validate manifest entries
    #[arg(long = "no-validate")]
    pub no_validate: bool,

    /// Overwrite existing system block devices (not recommended).
    /// All disks to be used must be declared in manifests,
    /// and existing system devices will not be considered
    #[arg(short = 'o', long = "overwrite")]
    pub overwrite: bool,
}

fn validate_filename(name: &str) -> Result<String, AliError> {
    if name.is_empty() {
        return Err(AliError::BadArgs(String::from("empty filename")));
    }

    Ok(name.to_string())
}
