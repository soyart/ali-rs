mod constants;
mod mkinitcpio;
mod quicknet;
mod replace_token;
mod uncomment;
mod wrappers;

pub use self::constants::hook_keys::*;

use colored::Colorize;
use serde::{
    Deserialize,
    Serialize,
};

use crate::errors::AliError;

/// All hook actions stores JSON string representation of the hook.
/// The reason being we want to hide hook implementation from outside code.
#[derive(Debug, Clone, Serialize, Deserialize)]

/// A report of hook actions, preferably in JSON or other serialized strings.
pub enum ActionHook {
    QuickNet(String),
    ReplaceToken(String),
    Uncomment(String),
    Mkinitcpio(String),
}

/// Entrypoint for hooks.
/// Some hooks may prefer to be called by certain callers.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Caller {
    ManifestChroot,
    ManifestPostInstall,
    Cli,
}

/// ModeHook represents whether this hook command is print-only
#[derive(Clone)]
enum ModeHook {
    /// May write changes to disk
    Normal,
    /// Print-only, i.e. idempotent
    Print,
}

/// A hook is an action parsed from a hook command.
///
/// A hook command is like a shell command - it is made of
/// 2 main parts: (1) the hook key, and (2) the command body
/// Hook key is always the first word of the hook command.
///
/// This module handles hook in this fashion:
///
/// 1. A hook key is matched with known hook key,
/// [creating an _empty hook_](init_blank_hook).
///
/// 2. If (1) was successful, we check if the hook key
/// is being called correctly (with `caller` and `root_location`).
///
/// 3. If (2) was successful, we pass the hook command to the
/// hook implementation to parse the rest of the hook command,
/// yielding a full, populated hook
trait Hook {
    /// (Default) Prints help to output
    fn help(&self) {
        println!(
            "{}",
            format!("{}: {}", self.hook_key(), self.usage()).green(),
        );
    }
    /// (Default) Prints yellow warning text to output
    fn eprintln_warn(&self, msg: &str) {
        eprintln!(
            "### {} ###",
            format!("{} WARN: {msg}", self.base_key()).yellow()
        );
    }

    /// (Default) Wraps error in hook with some string prefix
    fn hook_error(&self, msg: &str) -> AliError {
        AliError::HookError(format!("{}: {msg}", self.hook_key()))
    }

    /// (Default) Full key of the hook
    fn hook_key(&self) -> String {
        match self.mode() {
            ModeHook::Normal => self.base_key().to_string(),
            ModeHook::Print => format!("{}-print", self.base_key()),
        }
    }

    /// Base hook key (no `-print` suffix)
    fn base_key(&self) -> &'static str;

    /// Returns usage string for Self.help
    fn usage(&self) -> &'static str;

    /// Returns ModeHook parsed
    fn mode(&self) -> ModeHook;

    /// Returns whether this hook should be run inside chroot
    /// (warning only)
    fn should_chroot(&self) -> bool;

    /// Returns a set of callers the hook expects to be called from
    fn prefer_caller(&self, caller: &Caller) -> bool;

    /// Returns if we should abort if no mountpoint is found
    /// (i.e. root_location or mountpoint == /)
    fn abort_if_no_mount(&self) -> bool;

    /// Tries parsing `s` and returns error if any
    /// Implementation should use this chance to populate parsed data
    /// (hence `&mut self`) so that we only parse once.
    ///
    /// Nonetheless, implementation may parse s later with Self.try_parse,
    fn parse_cmd(&mut self, s: &str) -> Result<(), AliError>;

    /// Runs the inner hook once hook cmd is parsed
    fn run_hook(
        &self,
        caller: &Caller,
        root_location: &str,
    ) -> Result<ActionHook, AliError>;
}

/// Parses hook_cmd from manifest or CLI to hooks,
/// into some Hook, and validates it before finally
/// executing the hook.
pub fn apply_hook(
    cmd: &str,
    caller: Caller,
    root_location: &str,
) -> Result<ActionHook, AliError> {
    let hook_meta = parse_validate(cmd, &caller, root_location)?;
    hook_meta.run_hook(&caller, root_location)
}

/// Validates if hook_cmd is valid for its caller and mountpoint
pub fn validate_hook(
    cmd: &str,
    caller: &Caller,
    root_location: &str,
) -> Result<(), AliError> {
    _ = parse_validate(cmd, caller, root_location)?;

    Ok(())
}

pub fn is_hook(cmd: &str) -> bool {
    cmd.starts_with('@')
}

pub fn extract_key_and_parts(
    cmd: &str,
) -> Result<(String, Vec<String>), AliError> {
    let parts = cmd.split_whitespace().collect::<Vec<_>>();
    if parts.first().is_none() {
        return Err(AliError::AliRsBug("@mnt: got 0 part".to_string()));
    }

    Ok((
        parts.first().unwrap().to_string(),
        parts.into_iter().map(|s| s.to_string()).collect(),
    ))
}

pub fn extract_key_and_parts_shlex(
    cmd: &str,
) -> Result<(String, Vec<String>), AliError> {
    let (key, _) = extract_key_and_parts(cmd)?;

    let parts = shlex::split(cmd);
    if parts.is_none() {
        return Err(AliError::BadHookCmd("bad argument format".to_string()));
    }

    Ok((key, parts.unwrap()))
}

fn parse_validate(
    cmd: &str,
    caller: &Caller,
    root_location: &str,
) -> Result<Box<dyn Hook>, AliError> {
    let (key, _) = extract_key_and_parts(cmd)?;
    let mut h = init_blank_hook(&key)?;

    if let Err(err) = h.parse_cmd(cmd) {
        h.help();
        return Err(err);
    }

    if h.should_chroot() {
        handle_no_mountpoint(h.as_ref(), caller, root_location)?;
    }

    Ok(h)
}

fn init_blank_hook(k: &str) -> Result<Box<dyn Hook>, AliError> {
    match k {
        KEY_WRAPPER_MNT | KEY_WRAPPER_NO_MNT => {
            Ok(wrappers::init_from_key(k)) //
        }

        KEY_QUICKNET | KEY_QUICKNET_PRINT => {
            Ok(quicknet::init_from_key(k)) //
        }

        KEY_MKINITCPIO | KEY_MKINITCPIO_PRINT => {
            Ok(mkinitcpio::init_from_key(k)) //
        }

        KEY_REPLACE_TOKEN | KEY_REPLACE_TOKEN_PRINT => {
            Ok(replace_token::init_from_key(k))
        }

        KEY_UNCOMMENT
        | KEY_UNCOMMENT_PRINT
        | KEY_UNCOMMENT_ALL
        | KEY_UNCOMMENT_ALL_PRINT => Ok(uncomment::init_from_key(k)),

        key => Err(AliError::BadArgs(format!("unknown hook key: {key}"))),
    }
}

impl std::fmt::Display for Caller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ManifestChroot => {
                write!(f, "manifest key `chroot`") //
            }
            Self::ManifestPostInstall => {
                write!(f, "manifest key `postinstall`") //
            }
            Self::Cli => {
                write!(f, "subcommand `hooks`") //
            }
        }
    }
}

fn handle_no_mountpoint(
    hook: &dyn Hook,
    caller: &Caller,
    mountpoint: &str,
) -> Result<(), AliError> {
    if mountpoint == "/" {
        hook.eprintln_warn("got / as mountpoint");
        match caller {
            Caller::Cli => {
                hook.eprintln_warn(
                    "hint: use --mountpoint flag to specify non-/ mountpoint",
                )
            }
            Caller::ManifestPostInstall | Caller::ManifestChroot => {
                return Err(AliError::AliRsBug(format!(
                    "Got / as mountpoint for hook {}",
                    hook.hook_key(),
                )))
            }
        }

        if hook.abort_if_no_mount() {
            return Err(AliError::BadHookCmd(format!(
                "hook {} is to be run with a mountpoint",
                hook.hook_key()
            )));
        }
    }

    if !hook.prefer_caller(caller) {
        hook.eprintln_warn("non-preferred caller {caller}");
        hook.eprintln_warn("preferred callers: {preferred_callers:?}");
    }

    Ok(())
}
