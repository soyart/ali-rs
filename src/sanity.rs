use crate::errors::AyiError;
use crate::manifest::Manifest;
use crate::utils::fs::file_exists;
use crate::utils::shell::in_path;

pub fn check(manifest: &Manifest) -> Result<(), AyiError> {
    // Check disks
    for disk in manifest.disks.iter() {
        if !file_exists(&disk.device) {
            return Err(AyiError::NoSuchDevice(disk.device.to_string()));
        }
    }

    let rootfs_fs = &manifest.rootfs.0.fs_type;
    if !in_path(rootfs_fs) {
        return Err(AyiError::CmdFailed(
            None,
            format!("no such program to create rootfs: {rootfs_fs}"),
        ));
    }

    for archfs in manifest.filesystems.iter() {
        if !in_path(&archfs.fs_type) {
            let device = &archfs.device;
            return Err(AyiError::CmdFailed(
                None,
                format!("no such program to create filesystem for device {device}: {rootfs_fs}"),
            ));
        }
    }

    Ok(())
}
