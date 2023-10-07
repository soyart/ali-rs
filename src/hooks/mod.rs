mod constants;
mod mkinitcpio;
mod quicknet;
mod replace_token;
mod uncomment;

use serde::{Deserialize, Serialize};

use crate::errors::AliError;

use self::constants::mkinitcpio;

/// All hook actions stores JSON string representation of the hook.
/// The reason being we want to hide hook implementation from outside code.
#[derive(Debug, Clone, Serialize, Deserialize)]

pub enum ActionHook {
    QuickNet(String),
    ReplaceToken(String),
    Uncomment(String),
    Mkinitcpio(String),
}

pub fn apply_hook(
    hook_cmd: &str,
    in_chroot: bool,
    mut root_location: &str,
) -> Result<ActionHook, AliError> {
    let hook_parts = hook_cmd.split_whitespace().collect::<Vec<_>>();
    let hook = hook_parts.first();

    if hook.is_none() {
        return Err(AliError::BadManifest("empty hook".to_string()));
    };

    if in_chroot {
        root_location = "/";
    }

    let hook = hook.unwrap();

    match *hook {
        "@quicknet" | "@quicknet-print" => quicknet::quicknet(hook_cmd, root_location),
        "@replace-token" | "@replace-token-print" => replace_token::replace_token(hook_cmd),
        "@uncomment" | "@uncomment-print" | "@uncomment-all" | "@uncomment-all-print" => {
            uncomment::uncomment(hook_cmd)
        }
        "@mkinitcpio" | "@mkinitcpio-print" => mkinitcpio::mkinitcpio(hook_cmd),
        _ => Err(AliError::BadArgs(format!("unknown hook cmd: {hook}"))),
    }
}
