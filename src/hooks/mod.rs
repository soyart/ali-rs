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

struct ParseError {
    error: AliError,
    help_msg: String,
}

/// Hook represents a parsed, ready to use hook.
///
/// Other than [`run_hook`](Self::run_hook), which
/// actually executes the hook, this trait also defines
/// many methods for validating user calls to hooks.
trait Hook {
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

    /// Executes hook once parsed
    fn run_hook(
        &self,
        caller: &Caller,
        root_location: &str,
    ) -> Result<ActionHook, AliError>;
}

pub fn apply_hook(
    cmd: &str,
    caller: Caller,
    root_location: &str,
) -> Result<ActionHook, AliError> {
    let h = parse_validate_caller(cmd, &caller, root_location)?;
    h.run_hook(&caller, root_location)
}

/// Validates if hook_cmd is valid for its caller and mountpoint
pub fn validate_hook(
    cmd: &str,
    caller: &Caller,
    root_location: &str,
) -> Result<(), AliError> {
    _ = parse_validate_caller(cmd, caller, root_location)?;

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

fn wrap_bad_hook_cmd(err: AliError, help_msg: &str) -> ParseError {
    ParseError {
        error: err,
        help_msg: help_msg.to_string(),
    }
}

/// (Default) Prints help to output
fn print_help(hook_key: &str, usage: &str) {
    println!("{}", format!("{}: {}", hook_key, usage).green());
}

fn parse_hook(k: &str, cmd: &str) -> Result<Box<dyn Hook>, AliError> {
    let result = match k {
        KEY_WRAPPER_MNT | KEY_WRAPPER_NO_MNT => {
            wrappers::parse(k, cmd) //
        }

        KEY_QUICKNET | KEY_QUICKNET_PRINT => {
            quicknet::parse(k, cmd) //
        }

        KEY_MKINITCPIO | KEY_MKINITCPIO_PRINT => {
            mkinitcpio::parse(k, cmd) //
        }

        KEY_REPLACE_TOKEN | KEY_REPLACE_TOKEN_PRINT => {
            replace_token::parse(k, cmd)
        }

        KEY_UNCOMMENT
        | KEY_UNCOMMENT_PRINT
        | KEY_UNCOMMENT_ALL
        | KEY_UNCOMMENT_ALL_PRINT => {
            uncomment::parse(k, cmd) //
        }

        key => {
            Err(ParseError {
                error: AliError::BadHookCmd(format!("unknown hook key {key}")),
                help_msg: "Use `--help` to see help".to_string(),
            })
        }
    };

    match result {
        Err(ParseError { error, help_msg }) => {
            print_help(k, &help_msg);
            Err(error)
        }

        Ok(hook) => Ok(hook),
    }
}

fn parse_validate_caller(
    cmd: &str,
    caller: &Caller,
    root_location: &str,
) -> Result<Box<dyn Hook>, AliError> {
    let (key, _) = extract_key_and_parts(cmd)?;
    let hook = parse_hook(&key, cmd)?;

    if hook.should_chroot() {
        handle_no_mountpoint(hook.as_ref(), caller, root_location)?;
    }

    Ok(hook)
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

impl From<ParseError> for AliError {
    fn from(value: ParseError) -> Self {
        value.error
    }
}

impl std::ops::Deref for ParseError {
    type Target = AliError;

    fn deref(&self) -> &Self::Target {
        &self.error
    }
}

#[test]
fn test_extract_key_and_parts() {
    let should_pass = vec![
        (
            "hook_key hook_body",
            ("hook_key", vec!["hook_key", "hook_body"]),
        ),
        (
            "key val1 val2 val3",
            ("key", vec!["key", "val1", "val2", "val3"]),
        ),
        ("lone_key", ("lone_key", vec!["lone_key"])),
    ];

    for (s, (expected_key, expected_parts)) in should_pass {
        let (key, parts) = extract_key_and_parts(s).unwrap();
        assert_eq!(expected_key, key);
        assert_eq!(expected_parts, parts);
    }
}

#[test]
fn test_extract_key_and_parts_shlex() {
    let should_pass = vec![
        (
            "hook_key hook_body",
            ("hook_key", vec!["hook_key", "hook_body"]),
        ),
        (
            "key val1 val2 val3",
            ("key", vec!["key", "val1", "val2", "val3"]),
        ),
        ("lone_key", ("lone_key", vec!["lone_key"])),
        (
            "key v1=val1 'val2 val3'",
            ("key", vec!["key", "v1=val1", "val2 val3"]),
        ),
        (
            "key v1=val1 v2='val2 val3'",
            ("key", vec!["key", "v1=val1", "v2=val2 val3"]),
        ),
    ];

    for (s, (expected_key, expected_parts)) in should_pass {
        let (key, parts) = extract_key_and_parts_shlex(s).unwrap();
        assert_eq!(expected_key, key);
        assert_eq!(expected_parts, parts);
    }
}
