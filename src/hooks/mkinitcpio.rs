use serde::{
    Deserialize,
    Serialize,
};

use super::constants::mkinitcpio::*;
use super::{
    wrap_bad_hook_cmd,
    ActionHook,
    Caller,
    Hook,
    ModeHook,
    ParseError,
    KEY_MKINITCPIO,
    KEY_MKINITCPIO_PRINT,
};
use crate::errors::AliError;

const USAGE: &str =
    "[boot_hook=<BOOT_HOOK_PRESET>] [hooks=<HOOKS>] [binaries=BINARIES]";

pub(super) fn parse(k: &str, cmd: &str) -> Result<Box<dyn Hook>, ParseError> {
    match k {
        KEY_MKINITCPIO | KEY_MKINITCPIO_PRINT => {
            match HookMkinitcpio::try_from(cmd) {
                Err(err) => Err(wrap_bad_hook_cmd(err, USAGE)),
                Ok(hook) => Ok(Box::new(hook)),
            }
        }

        key => panic!("unknown key {key}"),
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Mkinitcpio {
    boot_hook: Option<BootHooksRoot>,
    binaries: Option<Vec<String>>,
    hooks: Option<Vec<String>>,
}

struct HookMkinitcpio {
    conf: Mkinitcpio,
    mode_hook: ModeHook,
}

impl Hook for HookMkinitcpio {
    fn base_key(&self) -> &'static str {
        KEY_MKINITCPIO
    }

    fn usage(&self) -> &'static str {
        USAGE
    }

    fn mode(&self) -> ModeHook {
        self.mode_hook.clone()
    }

    fn should_chroot(&self) -> bool {
        true
    }

    fn prefer_caller(&self, caller: &Caller) -> bool {
        matches!(caller, &Caller::ManifestChroot | &Caller::Cli)
    }

    fn abort_if_no_mount(&self) -> bool {
        true
    }

    fn run_hook(
        &self,
        caller: &Caller,
        root_location: &str,
    ) -> Result<ActionHook, AliError> {
        apply_mkinitcpio(
            &self.hook_key(),
            &self.mode_hook,
            self.conf.clone(),
            caller,
            root_location,
        )
    }
}

impl TryFrom<&str> for HookMkinitcpio {
    type Error = AliError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let (hook_key, parts) = super::extract_key_and_parts_shlex(s)?;
        let mode_hook = match hook_key.as_str() {
            KEY_MKINITCPIO => ModeHook::Normal,
            KEY_MKINITCPIO_PRINT => ModeHook::Print,
            _ => {
                return Err(AliError::BadHookCmd(format!(
                    "unexpected key {hook_key}"
                )));
            }
        };

        if parts.len() < 2 {
            return Err(AliError::BadHookCmd(format!(
                "{hook_key}: need at least 1 argument"
            )));
        }

        let args = &parts[1..];
        let keys_vals = args
            .iter()
            .filter_map(|arg| arg.split_once('='))
            .collect::<Vec<_>>();

        let mut mkinitcpio = Mkinitcpio::default();
        let mut dups = std::collections::HashSet::new();

        for (k, v) in keys_vals {
            let duplicate_key = !dups.insert(k);
            if duplicate_key {
                return Err(AliError::AliRsBug(format!(
                    "{hook_key}: duplicate key {k}"
                )));
            }

            match k {
                "boot_hook" => {
                    let boot_hook = decide_boot_hooks(&hook_key, v)?;
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

        if mkinitcpio.boot_hook.is_some() && mkinitcpio.hooks.is_some() {
            return Err(AliError::BadHookCmd(format!(
                "{hook_key}: boot_hook and hooks are mutually exclusive, but found both"
            )));
        }

        Ok(HookMkinitcpio {
            conf: mkinitcpio,
            mode_hook,
        })
    }
}

fn apply_mkinitcpio(
    hook_key: &str,
    mode_hook: &ModeHook,
    mut m: Mkinitcpio,
    _caller: &Caller,
    root_location: &str,
) -> Result<ActionHook, AliError> {
    if m.boot_hook.is_some() {
        let hooks = preset(m.boot_hook.clone().unwrap());
        let hooks = split_whitespace_to_strings(&hooks);

        m.hooks = Some(hooks);
    }

    let (mut hooks_mkinitcpio, mut binaries_mkinitcpio) = (None, None);

    if let Some(binaries) = &m.binaries {
        binaries_mkinitcpio =
            Some(fmt_shell_array("BINARIES", binaries.clone()));
    }
    if let Some(hooks) = &m.hooks {
        hooks_mkinitcpio = Some(fmt_shell_array("HOOKS", hooks.clone()));
    }

    let s = serde_json::to_string(&m).unwrap();
    if matches!(mode_hook, ModeHook::Print) {
        if let Some(s) = binaries_mkinitcpio {
            println!("{s}");
        }
        if let Some(s) = hooks_mkinitcpio {
            println!("{s}");
        }

        return Ok(ActionHook::Mkinitcpio(s));
    }

    let _mkinitcpio_conf = format!("/{root_location}/mkinitcpio.conf");

    Err(AliError::NotImplemented(
        format!("{hook_key}: write files",),
    ))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum BootHooksRoot {
    Lvm,
    Luks,
    LvmOnLuks,
    LuksOnLvm,
}

#[rustfmt::skip]
fn preset(t: BootHooksRoot) -> String {
    match t {
        BootHooksRoot::Lvm => {
            MKINITCPIO_PRESET_LVM_ROOT.to_string()
        }
        BootHooksRoot::Luks => {
            MKINITCPIO_PRESET_LUKS_ROOT.to_string()
        }
        BootHooksRoot::LvmOnLuks => {
            MKINITCPIO_PRESET_LVM_ON_LUKS_ROOT.to_string()
        }
        BootHooksRoot::LuksOnLvm => {
            MKINITCPIO_PRESET_LUKS_ON_LVM_ROOT.to_string()
        }
    }
}

fn decide_boot_hooks(
    hook_key: &str,
    v: &str,
) -> Result<BootHooksRoot, AliError> {
    if ALIASES_ROOT_LVM.contains(&v) {
        return Ok(BootHooksRoot::Lvm);
    }

    if ALIASES_ROOT_LUKS.contains(&v) {
        return Ok(BootHooksRoot::Luks);
    }

    if ALIASES_ROOT_LVM_ON_LUKS.contains(&v) {
        return Ok(BootHooksRoot::LvmOnLuks);
    }

    if ALIASES_ROOT_LUKS_ON_LVM.contains(&v) {
        return Ok(BootHooksRoot::LuksOnLvm);
    }

    Err(AliError::BadHookCmd(format!(
        "{hook_key}: no such boot_hook preset: {v}"
    )))
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
