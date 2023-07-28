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

    let mkfs_rootfs = &format!("mkfs.{}", manifest.rootfs.fs_type);
    if !in_path(mkfs_rootfs) {
        return Err(AyiError::CmdFailed(
            None,
            format!("no such program to create rootfs: {mkfs_rootfs}"),
        ));
    }

    for archfs in manifest.filesystems.iter() {
        let mkfs_cmd = &format!("mkfs.{}", archfs.fs_type);
        if !in_path(mkfs_cmd) {
            let device = &archfs.device;
            return Err(AyiError::CmdFailed(
                None,
                format!("no such program to create filesystem for device {device}: {mkfs_cmd}"),
            ));
        }
    }

    Ok(())
}
