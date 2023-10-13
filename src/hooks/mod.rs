mod constants;
mod mkinitcpio;
mod quicknet;
mod replace_token;
mod uncomment;

pub use self::constants::hook_keys::*;

use std::collections::HashSet;

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

/// HookMetadata provides this module with per-hook information
/// that this module needs in order to validate hooks.
trait HookMetadata {
    /// [Default] Prints help to output
    fn help(&self) {
        println!(
            "{}",
            format!("{}: {}", self.hook_key(), self.usage()).green(),
        );
    }

    /// [Default] Prints yellow warning text to output
    fn eprintln_warn(&self, msg: &str) {
        eprintln!("{}", format!("{} WARN: {msg}", self.base_key()).yellow());
    }

    /// [Default] Wraps error in hook with some string prefix
    fn hook_error(&self, msg: &str) -> AliError {
        AliError::HookError(format!("{}: {msg}", self.hook_key()))
    }

    /// [Default] Full key of the hook
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
    fn preferred_callers(&self) -> HashSet<Caller>;

    /// Returns if we should abort if no mountpoint is found
    /// (i.e. root_location or mountpoint == /)
    fn abort_if_no_mount(&self) -> bool;

    /// Tries parsing `s` and returns error if any
    /// Implementation should use this change to populate parsed data
    /// (hence `&mut self`).
    ///
    /// Implementation may parse later with Self.advance
    fn try_parse(&mut self, s: &str) -> Result<(), AliError>;

    /// Returns the real implementation of the hook
    fn advance(&self) -> Box<dyn Hook>;
}

/// Hook represents the real hook action to be performed.
trait Hook {
    fn apply(
        &self,
        caller: &Caller,
        root_location: &str,
    ) -> Result<ActionHook, AliError>;
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

    let key = *hook_parts.first().unwrap();

    let mut hook_meta: Box<dyn HookMetadata> = match key {
        UNCOMMENT | UNCOMMENT_PRINT | UNCOMMENT_ALL | UNCOMMENT_ALL_PRINT => {
            uncomment::new(key)
        }

        QUICKNET | QUICKNET_PRINT => quicknet::new(key),

        REPLACE_TOKEN | REPLACE_TOKEN_PRINT => replace_token::new(key),

        MKINITCPIO | MKINITCPIO_PRINT => mkinitcpio::new(key),

        key => {
            return Err(AliError::BadArgs(format!("unknown hook key: {key}")))
        }
    };

    if let Err(err) = hook_meta.try_parse(hook_cmd) {
        hook_meta.help();
        return Err(err);
    }

    if hook_meta.should_chroot() {
        handle_no_mountpoint(&hook_meta, &caller, root_location)?;
    }

    hook_meta.advance().apply(&caller, root_location)
}

impl std::fmt::Display for Caller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ManifestChroot => write!(f, "manifest key `chroot`"),
            Self::ManifestPostInstall => {
                write!(f, "manifest key `postinstall`")
            }
            Self::Cli => {
                write!(f, "subcommand `hooks`")
            }
        }
    }
}

fn all_callers() -> HashSet<Caller> {
    HashSet::from([
        Caller::ManifestChroot,
        Caller::ManifestPostInstall,
        Caller::Cli,
    ])
}

fn handle_no_mountpoint(
    hook: &Box<dyn HookMetadata>,
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

    let preferred_callers = hook.preferred_callers();
    if !preferred_callers.contains(caller) {
        hook.eprintln_warn("non-preferred caller {caller}");
        hook.eprintln_warn("preferred callers: {preferred_callers:?}");
    }

    Ok(())
}
