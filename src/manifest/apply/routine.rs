use std::collections::HashSet;

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
