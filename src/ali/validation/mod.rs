mod blk;
mod hooks;

use crate::ali::Manifest;
use crate::constants::{
    self,
    defaults,
};
use crate::entity::report::ValidationReport;
use crate::errors::AliError;
use crate::utils::fs::file_exists;
use crate::utils::shell;

pub fn validate(
    manifest: &Manifest,
    overwrite: bool,
) -> Result<ValidationReport, AliError> {
    let block_devs = blk::validate(manifest, overwrite)?;

    // Check all commands used by ALI before ch-root
    for cmd in constants::REQUIRED_COMMANDS {
        if !shell::in_path(cmd) {
            return Err(AliError::Validation(format!(
                "command {cmd} not in path"
            )));
        }
    }

    // Check mkfs for rootfs
    let mkfs_rootfs = &format!("mkfs.{}", manifest.rootfs.fs_type);
    if !shell::in_path(mkfs_rootfs) {
        return Err(AliError::BadManifest(format!(
            "no such program to create rootfs: {mkfs_rootfs}"
        )));
    }

    // Check mkfs.{fs} for other FS
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

    hooks::validate(manifest)?;

    // Check timezone file in local installer
    let zone_info = format!(
        "/usr/share/zoneinfo/{}",
        manifest
            .timezone
            .clone()
            .unwrap_or(defaults::TIMEZONE.into())
    );

    if !file_exists(&zone_info) {
        return Err(AliError::BadManifest(format!(
            "no zone info file {zone_info}"
        )));
    }

    Ok(ValidationReport { block_devs })
}
