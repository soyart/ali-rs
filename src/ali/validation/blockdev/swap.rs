use std::collections::HashSet;

use crate::errors::AliError;

pub(super) fn validate(
    swaps: &[String],
    fs_ready_devs: &mut HashSet<String>,
) -> Result<(), AliError> {
    const MSG: &str = "swap validation failed";

    for (i, swap) in swaps.iter().enumerate() {
        if !fs_ready_devs.contains(swap) {
            return Err(AliError::BadManifest(format!(
                "{MSG}: device {swap} for swap #{} is not fs-ready",
                i + 1,
            )));
        }

        fs_ready_devs.remove(swap);
    }

    Ok(())
}
