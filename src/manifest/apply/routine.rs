use std::collections::HashSet;

use crate::defaults;
use crate::errors::AliError;
use crate::utils::shell;

pub fn pacstrap_to_location(
    pacstraps: &Option<HashSet<String>>,
    location: &str,
) -> Result<(), AliError> {
    // Collect packages, with base as bare-minimum
    let mut packages = HashSet::from(["base".to_string()]);
    if let Some(pacstraps) = pacstraps.clone() {
        packages.extend(pacstraps);
    }

    let cmd_pacstrap = cmd_pacstrap(packages.clone(), location.to_string());

    shell::exec("sh", &["-c", &format!("'{cmd_pacstrap}'")])
}

fn cmd_pacstrap(packages: HashSet<String>, location: String) -> String {
    let mut cmd_parts = vec!["pacstrap".to_string(), "-K".to_string(), location];
    cmd_parts.extend(packages);

    cmd_parts.join(" ")
}

pub fn genfstab_uuid(install_location: &str) -> Result<(), AliError> {
    let cmd = cmd_genfstab(install_location);
    shell::exec("sh", &["-c", &format!("'{cmd}'")])
}

fn cmd_genfstab(install_location: &str) -> String {
    format!("genfstab -U {install_location} >> {install_location}/etc/fstab")
}

pub fn hostname(hostname: &Option<String>, install_location: &str) -> Result<(), AliError> {
    let hostname = hostname
        .clone()
        .unwrap_or(defaults::DEFAULT_HOSTNAME.to_string());

    let etc_hostname = format!("{install_location}/etc/hostname");

    std::fs::write(&etc_hostname, hostname).map_err(|err| {
        AliError::FileError(err, format!("failed to write hostname to {etc_hostname}"))
    })
}
