mod apply;
mod validate;

use crate::cli::{Cli, Commands};
use crate::errors::AliError;

pub fn run(cli_args: Cli) -> Result<(), AliError> {
    match cli_args.commands {
        Commands::Apply(apply_args) => match apply::run(&cli_args.manifest, apply_args) {
            Err(err) => Err(err),
            Ok(report) => {
                println!("{}", report.to_json_string());
                Ok(())
            }
        },
        Commands::Validate => validate::run(&cli_args.manifest),
    }
}
