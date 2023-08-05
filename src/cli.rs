use clap::Parser;

use crate::errors::NayiError;

#[derive(Debug, Parser)]
#[clap(author = "github.com/soyart", version, about = "Rust-based ayi parser")]
pub struct Args {
    #[arg(short = 'f', value_parser = validate_filename)]
    pub manifest: String,

    #[arg(short = 'n', default_value_t = false)]
    pub dry_run: bool,
}

fn validate_filename(name: &str) -> Result<String, NayiError> {
    if name.is_empty() {
        return Err(NayiError::BadArgs(String::from("empty filename")));
    }

    Ok(name.to_string())
}
