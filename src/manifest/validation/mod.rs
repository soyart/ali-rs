mod blk; // Block device validation

use crate::defaults;
use crate::errors::AliError;
use crate::manifest::Manifest;
use crate::utils::fs::file_exists;
use crate::utils::shell::in_path;

pub fn validate(manifest: &Manifest, overwrite: bool) -> Result<(), AliError> {
    blk::validate(manifest, overwrite)?;

    // Check mkfs for rootfs
    let mkfs_rootfs = &format!("mkfs.{}", manifest.rootfs.fs_type);
    if !in_path(mkfs_rootfs) {
        return Err(AliError::BadManifest(format!(
            "no such program to create rootfs: {mkfs_rootfs}"
        )));
    }

    // Check mkfs for other FS
    if let Some(filesystems) = &manifest.filesystems {
        for fs in filesystems {
            let mkfs_cmd = &format!("mkfs.{}", fs.fs_type);
            if !in_path(mkfs_cmd) {
                let device = &fs.device;

                return Err(AliError::BadManifest(format!(
                    "no such program to create filesystem for device {device}: {mkfs_cmd}"
                )));
            }
        }
    }

    let zone_info = format!(
        "/usr/share/zoneinfo/{}",
        manifest
            .timezone
            .clone()
            .unwrap_or(defaults::DEFAULT_TIMEZONE.into())
    );

    if !file_exists(&zone_info) {
        return Err(AliError::BadManifest(format!(
            "no zone info file {zone_info}"
        )));
    }

    Ok(())
}
