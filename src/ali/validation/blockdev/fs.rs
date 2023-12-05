use std::collections::HashSet;

use crate::ali::ManifestFs;
use crate::errors::AliError;

pub(super) fn collect_rootfs_fs_devs(
    rootfs: &String,
    fs_ready_devs: &mut HashSet<String>,
) -> Result<(), AliError> {
    const MSG: &str = "rootfs validation failed";

    if !fs_ready_devs.contains(rootfs) {
        return Err(AliError::BadManifest(format!(
            "{MSG}: no top-level fs-ready device for rootfs: {rootfs}",
        )));
    }

    // Remove used up fs-ready device (rootfs)
    fs_ready_devs.remove(rootfs);

    Ok(())
}

// Collects filesystems into fs_devs,
// and removing the base from fs_ready_devs as it goes through the list.
pub(super) fn collect_fs_devs<'a>(
    filesystems: &'a [ManifestFs],
    fs_ready_devs: &mut HashSet<String>,
    fs_devs: &mut HashSet<&'a String>,
) -> Result<(), AliError> {
    const MSG: &str = "fs validation failed";

    for (i, fs) in filesystems.iter().enumerate() {
        if !fs_ready_devs.contains(&fs.device) {
            return Err(AliError::BadManifest(format!(
                "{MSG}: device {} for fs #{} ({}) is not fs-ready",
                fs.device,
                i + 1,
                fs.fs_type,
            )));
        }

        // Remove used up fs-ready device
        fs_ready_devs.remove(&fs.device);

        // Collect this fs to fs devices to later validate mountpoints
        if fs_devs.insert(&fs.device) {
            continue;
        }

        return Err(AliError::AliRsBug(format!(
            "{MSG}: duplicate filesystem devices from manifest filesystems: {} ({})",
            fs.device, fs.fs_type,
        )));
    }

    Ok(())
}
