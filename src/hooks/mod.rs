mod constants;
mod quicknet;

use serde::{Deserialize, Serialize};

use crate::errors::AliError;

#[derive(Debug, Clone, Serialize, Deserialize)]

pub enum ActionHook {
    QuickNet(String),
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
    match hook {
        &"#quicknet" => quicknet::quicknet(hook_cmd, root_location),
        _ => Err(AliError::NotImplemented(format!("hook {hook}"))),
    }
}
