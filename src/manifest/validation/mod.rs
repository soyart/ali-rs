mod blk; // Block device validation

use crate::defaults;
use crate::entity::ValidationReport;
use crate::errors::AliError;
use crate::manifest::Manifest;
use crate::utils::fs::file_exists;
use crate::utils::shell;

// @TODO: return validation report
pub fn validate(manifest: &Manifest, overwrite: bool) -> Result<ValidationReport, AliError> {
    let block_devs = blk::validate(manifest, overwrite)?;

    // Check mkfs for rootfs
    let mkfs_rootfs = &format!("mkfs.{}", manifest.rootfs.fs_type);
    if !shell::in_path(mkfs_rootfs) {
        return Err(AliError::BadManifest(format!(
            "no such program to create rootfs: {mkfs_rootfs}"
        )));
    }

    // Check mkfs for other FS
    if let Some(filesystems) = &manifest.filesystems {
        for fs in filesystems {
            let mkfs_cmd = &format!("mkfs.{}", fs.fs_type);
            if !shell::in_path(mkfs_cmd) {
                let device = &fs.device;

                return Err(AliError::BadManifest(format!(
                    "no such program to create filesystem for device {device}: {mkfs_cmd}"
                )));
            }
        }
    }

    // Check timezone file in local installer
    let zone_info = format!(
        "/usr/share/zoneinfo/{}",
        manifest
            .timezone
            .clone()
            .unwrap_or(defaults::DEFAULT_TIMEZONE.into())
    );

    // Check all commands used by ALI before ch-root
    for cmd in defaults::REQUIRED_COMMANDS {
        if !shell::in_path(cmd) {
            return Err(AliError::Validation(format!("command {cmd} not in path")));
        }
    }

    if !file_exists(&zone_info) {
        return Err(AliError::BadManifest(format!(
            "no zone info file {zone_info}"
        )));
    }

    Ok(ValidationReport { block_devs })
}
