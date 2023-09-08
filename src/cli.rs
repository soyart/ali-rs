use clap::{Args, Parser, Subcommand};

use crate::entity::stage;
use crate::errors::AliError;

#[derive(Debug, Parser)]
#[clap(
    version,
    author = "github.com/soyart",
    about = "Rust-based ALI installer"
)]
pub struct Cli {
    #[command(subcommand)]
    pub commands: Commands,

    /// Path to manifest file
    #[arg(
        global = true,
        short = 'f',
        long = "file",
        default_value_t = String::from("./manifest.yaml"),
        value_parser = validate_filename,
    )]
    pub manifest: String,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Applies manifest to create a new system
    Apply(ArgsApply),

    /// Validates manifest
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
    #[arg(short = 'o', long = "overwrite", default_value_t = false)]
    pub overwrite: bool,

    /// ALI stages to skip
    #[arg(short = 's', long = "skip", num_args(0..))]
    pub skip_stages: Vec<stage::Stage>,

    /// Dry-run, ali-rs will not commit any changes to disks,
    /// and will just print steps to be performed
    #[arg(global = true, short = 'n', default_value_t = false)]
    pub dry_run: bool,
}

fn validate_filename(name: &str) -> Result<String, AliError> {
    if name.is_empty() {
        return Err(AliError::BadArgs(String::from("empty filename")));
    }

    Ok(name.to_string())
}
