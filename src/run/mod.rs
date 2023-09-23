pub mod apply;
pub mod hooks;
pub mod validate;

use crate::cli;
use crate::errors::AliError;

pub fn run(cli_args: cli::Cli) -> Result<(), AliError> {
    match cli_args.commands {
        // Default is to validate
        None | Some(cli::Commands::Validate) => validate::run(&cli_args.manifest),
        // Apply manifest in full
        Some(cli::Commands::Apply(args_apply)) => {
            match apply::run(&cli_args.manifest, args_apply) {
                Err(err) => Err(err),
                Ok(report) => Ok(println!("{}", report.to_json_string())),
            }
        }
        Some(cli::Commands::Hooks(args_hooks)) => hooks::run(args_hooks),
    }
}
