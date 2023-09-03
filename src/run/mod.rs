pub mod apply;
pub mod validate;

use crate::cli;
use crate::errors::AliError;

pub fn run(cli_args: cli::Cli) -> Result<(), AliError> {
    match cli_args.commands {
        cli::Commands::Apply(apply_args) => match apply::run(&cli_args.manifest, apply_args) {
            Err(err) => Err(err),
            Ok(report) => Ok(println!("{}", report.to_json_string())),
        },
        cli::Commands::Validate => validate::run(&cli_args.manifest),
    }
}
