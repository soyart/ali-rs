use serde::{Deserialize, Serialize};
use serde_json;

use super::constants::mkinitcpio::*;
use super::ActionHook;
use crate::errors::AliError;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Mkinitcpio {
    pub boot_hook: Option<BootHooks>,
    pub binaries: Option<Vec<String>>,
    pub hooks: Option<Vec<String>>,
    pub print_only: bool,
}

pub fn mkinitcpio(cmd: &str) -> Result<ActionHook, AliError> {
    let mut m = parse(cmd)?;

    if m.boot_hook.is_some() {
        let hooks = hardcode_boot_hooks(m.boot_hook.clone().unwrap());
        let hooks = split_whitespace_to_strings(&hooks);

        m.hooks = Some(hooks);
    }

    let (mut hooks_mkinitcpio, mut binaries_mkinitcpio) = (None, None);
    if let Some(hooks) = &m.hooks {
        hooks_mkinitcpio = Some(fmt_shell_array("HOOKS", hooks.clone()));
    }
    if let Some(binaries) = &m.binaries {
        binaries_mkinitcpio = Some(fmt_shell_array("BINARIES", binaries.clone()));
    }

    if m.print_only {
        if let Some(s) = hooks_mkinitcpio {
            println!("{s}");
        }
        if let Some(s) = binaries_mkinitcpio {
            println!("{s}");
        }

        let s = serde_json::to_string(&m).unwrap();

        return Ok(ActionHook::Mkinitcpio(s));
    }

    // @TODO: Write

    Err(AliError::NotImplemented(
        "@mkinitcpio: write files".to_string(),
    ))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum BootHooks {
    RootOnLvm,
    RootOnLuks,
    RootOnLvmOnLuks,
    RootOnLuksOnLvm,
}

fn hardcode_boot_hooks(t: BootHooks) -> String {
    match t {
        BootHooks::RootOnLvm => MKINITCPIO_HOOKS_LVM_ROOT.to_string(),
        BootHooks::RootOnLuks => MKINITCPIO_HOOKS_LUKS_ROOT.to_string(),
        BootHooks::RootOnLvmOnLuks => MKINITCPIO_HOOKS_LVM_ON_LUKS_ROOT.to_string(),
        BootHooks::RootOnLuksOnLvm => MKINITCPIO_HOOKS_LUKS_ON_LVM_ROOT.to_string(),
    }
}

fn decide_boot_hooks(v: &str) -> Result<BootHooks, AliError> {
    if ALIASES_ROOT_LVM.contains(&v) {
        return Ok(BootHooks::RootOnLvm);
    }

    if ALIASES_ROOT_LUKS.contains(&v) {
        return Ok(BootHooks::RootOnLuks);
    }

    if ALIASES_ROOT_LVM_ON_LUKS.contains(&v) {
        return Ok(BootHooks::RootOnLvmOnLuks);
    }

    if ALIASES_ROOT_LUKS_ON_LVM.contains(&v) {
        return Ok(BootHooks::RootOnLuksOnLvm);
    }

    Err(AliError::BadHookCmd(format!(
        "@mkinitcpio: no such boot_hook: {v}"
    )))
}

fn parse(s: &str) -> Result<Mkinitcpio, AliError> {
    let parts = shlex::split(s).unwrap();
    // println!("parts {:?}", &parts[1..]);
    let args = &parts[1..];
    let keys_vals = args
        .iter()
        .filter_map(|arg| arg.split_once("="))
        .collect::<Vec<_>>();

    let mut mkinitcpio = Mkinitcpio::default();
    let mut dups = std::collections::HashSet::new();

    let cmd = parts[0].as_str();
    match cmd {
        "@mkinitcpio-print" => {}
        "@mkinitcpio" => {
            mkinitcpio.print_only = false;
        }
        _ => {
            return Err(AliError::BadHookCmd(format!(
                "@mkinitcpio: unknown hook command {cmd}"
            )))
        }
    }

    for (k, ref v) in keys_vals {
        let duplicate_key = !dups.insert(k);
        if duplicate_key {
            return Err(AliError::BadHookCmd(format!(
                "@mkinitcpio: duplicate key {k}"
            )));
        }

        match k {
            "boot_hook" => {
                let boot_hook = decide_boot_hooks(v)?;
                mkinitcpio.boot_hook = Some(boot_hook);

                continue;
            }
            "binaries" => {
                let binaries = split_whitespace_to_strings(v);
                mkinitcpio.binaries = Some(binaries);

                continue;
            }
            "hooks" => {
                let hooks = split_whitespace_to_strings(v);
                mkinitcpio.hooks = Some(hooks);
            }
            _ => continue,
        }
    }

    if let (Some(_), Some(_)) = (&mkinitcpio.boot_hook, &mkinitcpio.hooks) {
        return Err(AliError::BadHookCmd(
            "@mkinitcpio: boot_hook and hooks are mutually exclusive, but found both".to_string(),
        ));
    }

    Ok(mkinitcpio)
}

fn split_whitespace_to_strings(s: &str) -> Vec<String> {
    s.split_whitespace()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
}

fn fmt_shell_array(arr_name: &str, arr_elems: Vec<String>) -> String {
    let s = arr_elems.join(" ");

    format!("{arr_name}=({s})")
}

impl std::default::Default for Mkinitcpio {
    fn default() -> Self {
        return Self {
            boot_hook: None,
            binaries: None,
            hooks: None,
            print_only: true,
        };
    }
}

const ALIASES_ROOT_LVM: [&str; 7] = [
    "root-on-lvm",
    "root_on_lvm",
    "root-lvm",
    "root_lvm",
    "lvm-root",
    "lvm_root",
    "lvm",
];

const ALIASES_ROOT_LUKS: [&str; 7] = [
    "root-on-luks",
    "root_on_luks",
    "root-luks",
    "root_luks",
    "luks-root",
    "luks_root",
    "luks",
];

const ALIASES_ROOT_LVM_ON_LUKS: [&str; 8] = [
    "root-on-lvm-on-luks",
    "root_on_lvm_on_luks",
    "lvm-on-luks-root",
    "lvm_on_luks_root",
    "root-lvm-on-luks",
    "root_lvm_on_luks",
    "lvm-on-luks",
    "lvm_on_luks",
];

const ALIASES_ROOT_LUKS_ON_LVM: [&str; 8] = [
    "root-on-luks-on-lvm",
    "root_on_luks_on_lvm",
    "luks-on-lvm-root",
    "luks_on_lvm_root",
    "root-luks-on-lvm",
    "root_luks_on_lvm",
    "luks-on-lvm",
    "luks_on_lvm",
];
