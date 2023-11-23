pub mod apply;
pub mod hooks;
pub mod validate;

use std::env;

use colored::Colorize;

use crate::constants::defaults;
use crate::errors::AliError;
use crate::{
    cli,
    constants,
    linux,
};

pub fn run(cli_args: cli::Cli) -> Result<(), AliError> {
    let new_root_location = install_location();

    match cli_args.commands {
        // Default is to validate
        None | Some(cli::Commands::Validate) => {
            validate::run(&cli_args.manifest, &new_root_location)
        }
        // Apply manifest in full
        Some(cli::Commands::Apply(args_apply)) => {
            if !linux::user::is_root() {
                println!("{}", "WARN: running as non-root user".yellow())
            }

            match apply::run(&cli_args.manifest, &new_root_location, args_apply)
            {
                Err(err) => Err(err),
                Ok(report) => Ok(println!("{}", report.to_json_string())),
            }
        }
        Some(cli::Commands::Hooks(args_hooks)) => {
            hooks::run(&cli_args.manifest, args_hooks)
        }
    }
}

fn install_location() -> String {
    env::var(constants::ENV_ALI_LOC)
        .unwrap_or(defaults::INSTALL_LOCATION.to_string())
}
