use crate::errors::AliError;
use crate::{
    cli,
    hooks,
};

pub fn run(cli_args: cli::ArgsHooks) -> Result<(), AliError> {
    let mountpoint = cli_args.mountpoint.unwrap_or(String::from("/"));

    for hook in cli_args.hooks {
        hooks::apply_hook(&hook, hooks::Caller::Cli, &mountpoint)?;
    }

    Ok(())
}
