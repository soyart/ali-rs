// @TODO: non-interactive passphrase

use crate::errors::AliError;
use crate::utils::shell;

pub fn format(device: &str, key: Option<&str>) -> Result<(), AliError> {
    let mut format_cmd = format!("cryptsetup luksFormat {device}");

    if let Some(passphrase) = key {
        format_cmd = format!("echo '{passphrase}' | {format_cmd}");
    }

    shell::sh_c(&format_cmd)
}

pub fn open(
    device: &str,
    key: Option<&str>,
    name: &str,
) -> Result<(), AliError> {
    let mut open_cmd = format!("cryptsetup luksOpen {device} {name}");

    if let Some(passphrase) = key {
        open_cmd = format!("echo '{passphrase}' | {open_cmd}")
    }

    shell::sh_c(&open_cmd)
}
