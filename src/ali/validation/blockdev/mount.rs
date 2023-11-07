use std::collections::HashSet;

use crate::ali::ManifestMountpoint;
use crate::errors::AliError;

pub(super) fn validate(
    mountpoints: &[ManifestMountpoint],
) -> Result<(), AliError> {
    let mut dups = HashSet::new();

    for mnt in mountpoints {
        if mnt.dest.as_str() == "/" {
            return Err(AliError::BadManifest(format!(
                "bad mountpoint / for non-rootfs {}",
                mnt.device,
            )));
        }

        if !dups.insert(mnt.dest.as_str()) {
            return Err(AliError::BadManifest(format!(
                "duplicate mountpoints {}",
                mnt.dest,
            )));
        }
    }

    Ok(())
}
