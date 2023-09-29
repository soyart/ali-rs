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

    let cmd_pacstrap = {
        let mut cmd_parts = vec![
            "pacstrap".to_string(),
            "-K".to_string(),
            location.to_string(),
        ];

        cmd_parts.extend(packages);
        cmd_parts.join(" ")
    };

    shell::sh_c(&cmd_pacstrap)
}
