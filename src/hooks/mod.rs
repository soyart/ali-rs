mod constants;
mod mkinitcpio;
mod quicknet;
mod replace_token;
mod uncomment;

use colored::Colorize;
use serde::{Deserialize, Serialize};

use crate::errors::AliError;

const QUICKNET: &str = "@quicknet";
const QUICKNET_PRINT: &str = "@quicknet-print";
const MKINITCPIO: &str = "@mkinitcpio";
const MKINITCPIO_PRINT: &str = "@mkinitcpio-print";
const UNCOMMENT: &str = "@uncomment";
const UNCOMMENT_PRINT: &str = "@uncomment-print";
const UNCOMMENT_ALL: &str = "@uncomment-all";
const UNCOMMENT_ALL_PRINT: &str = "@uncomment-all-print";
const REPLACE_TOKEN: &str = "@replace-token";
const REPLACE_TOKEN_PRINT: &str = "@replace-token-print";

/// All hook actions stores JSON string representation of the hook.
/// The reason being we want to hide hook implementation from outside code.
#[derive(Debug, Clone, Serialize, Deserialize)]

pub enum ActionHook {
    QuickNet(String),
    ReplaceToken(String),
    Uncomment(String),
    Mkinitcpio(String),
}

pub enum Caller {
    ManifestChroot,
    ManifestPostInstall,
    Cli,
}

pub fn apply_hook(
    hook_cmd: &str,
    caller: Caller,
    root_location: &str,
) -> Result<ActionHook, AliError> {
    let hook_parts = hook_cmd.split_whitespace().collect::<Vec<_>>();

    if hook_parts.is_empty() {
        return Err(AliError::BadManifest("empty hook".to_string()));
    }

    let hook = hook_parts.first().unwrap();

    match *hook {
        QUICKNET | QUICKNET_PRINT => {
            quicknet::quicknet(hook_cmd, caller, root_location)
        }

        REPLACE_TOKEN | REPLACE_TOKEN_PRINT => {
            replace_token::replace_token(hook_cmd, caller, root_location)
        }

        MKINITCPIO | MKINITCPIO_PRINT => {
            mkinitcpio::mkinitcpio(hook_cmd, caller, root_location)
        }

        UNCOMMENT | UNCOMMENT_PRINT | UNCOMMENT_ALL | UNCOMMENT_ALL_PRINT => {
            uncomment::uncomment(hook_cmd, caller, root_location)
        }

        _ => Err(AliError::BadArgs(format!("unknown hook cmd: {hook}"))),
    }
}

fn warn_if_no_mountpoint(
    hook_name: &str,
    caller: Caller,
    mountpoint: &str,
) -> Result<(), AliError> {
    if mountpoint == "/" {
        println!(
            "{}",
            format!("## {hook_name} warning: got `/` as mountpoint").yellow()
        );

        match caller {
            Caller::Cli => {
                println!(
                    "{}",
                    format!("## {hook_name}: this hook should be run with flag --mountpoint").yellow(),
                );
            }
            Caller::ManifestPostInstall | Caller::ManifestChroot => {
                return Err(AliError::AliRsBug(format!(
                    "Got / as mountpoint for hook {hook_name}",
                )))
            }
        }
    }

    Ok(())
}
