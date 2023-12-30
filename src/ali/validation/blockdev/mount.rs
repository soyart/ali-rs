use std::collections::HashSet;

use crate::ali::ManifestMountpoint;
use crate::errors::AliError;

const MSG: &str = "mountpoint validation failed";

pub(super) fn validate_dups(
    mountpoints: &[ManifestMountpoint],
) -> Result<(), AliError> {
    let mut dups = HashSet::new();

    for mnt in mountpoints {
        if mnt.dest.as_str() == "/" {
            return Err(AliError::BadManifest(format!(
                "{MSG}: bad mountpoint / for non-rootfs {}",
                mnt.device,
            )));
        }

        if !dups.insert(mnt.dest.as_str()) {
            return Err(AliError::BadManifest(format!(
                "{MSG}: duplicate mountpoints {}",
                mnt.dest,
            )));
        }
    }

    Ok(())
}

pub(super) fn validate(
    mountpoints: &[ManifestMountpoint],
    fs_devs: &mut HashSet<String>,
) -> Result<(), AliError> {
    for (i, mnt) in mountpoints.iter().enumerate() {
        if fs_devs.contains(&mnt.device) {
            continue;
        }

        return Err(AliError::BadManifest(format!(
            "{MSG}: mountpoint {} for device #{} ({}) is not fs-ready",
            mnt.dest,
            i + 1,
            mnt.device,
        )));
    }

    Ok(())
}
