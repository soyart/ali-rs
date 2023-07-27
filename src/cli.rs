use clap::Parser;

#[derive(Debug, Parser)]
#[clap(author = "github.com/soyart", version, about = "Rust-based ayi parser")]
pub struct Args {
    #[arg(short = 'f', value_parser = validate_filename)]
    pub manifest: String,

    #[arg(short = 'n', default_value_t = false)]
    pub dry_run: bool,
}

fn validate_filename(name: &str) -> Result<String, String> {
    if name.is_empty() {
        return Err(String::from("empty filename"));
    }

    Ok(name.to_string())
}
