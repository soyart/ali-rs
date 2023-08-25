use clap::Parser;

use crate::errors::AliError;

#[derive(Debug, Parser)]
#[clap(
    author = "github.com/soyart",
    version,
    about = "Rust-based ALI installer"
)]
pub struct Args {
    /// Manifest file
    #[arg(short = 'f', long = "file", value_parser = validate_filename)]
    pub manifest: String,

    /// Do not validate manifest entries
    #[arg(long = "no-validate")]
    pub no_validate: bool,

    /// Overwrite existing system block devices (not recommended).
    /// All disks to be used must be declared in manifests,
    /// and existing system devices will not be considered
    #[arg(short = 'o', long = "overwrite")]
    pub overwrite: bool,

    /// Dry-run, ali-rs will not commit any changes to disks,
    /// and will just print steps to be performed
    #[arg(short = 'n', default_value_t = false)]
    pub dry_run: bool,
}

fn validate_filename(name: &str) -> Result<String, AliError> {
    if name.is_empty() {
        return Err(AliError::BadArgs(String::from("empty filename")));
    }

    Ok(name.to_string())
}
