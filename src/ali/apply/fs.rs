use crate::ali::{
    ManifestFs,
    ManifestMountpoint,
};
use crate::types::action::ActionMountpoints;
use crate::errors::AliError;
use crate::linux;

use super::map_err::map_err_mountpoints;

pub fn create_filesystem(
    filesystem: &ManifestFs,
) -> Result<ActionMountpoints, AliError> {
    linux::mkfs::create_fs(filesystem)?;

    Ok(ActionMountpoints::CreateFs {
        device: filesystem.device.clone(),
        fs_type: filesystem.fs_type.clone(),
        fs_opts: filesystem.fs_opts.clone(),
    })
}

// mount_filesystem lets callers override mountpoint with `mountpoint`.
pub fn mount_filesystem(
    mnt: &ManifestMountpoint,
    base: &str,
) -> Result<ActionMountpoints, AliError> {
    linux::mount::mount(mnt, base)?;

    Ok(ActionMountpoints::MountFs {
        src: mnt.device.clone(),
        dst: mnt.dest.clone(),
        opts: mnt.mnt_opts.clone(),
    })
}

pub fn create_filesystems(
    filesystems: &[ManifestFs],
) -> Result<Vec<ActionMountpoints>, AliError> {
    let mut actions = Vec::new();

    for fs in filesystems {
        let action_create_fs = ActionMountpoints::CreateFs {
            device: fs.device.clone(),
            fs_type: fs.fs_type.clone(),
            fs_opts: fs.fs_opts.clone(),
        };

        match create_filesystem(fs) {
            Err(err) => {
                return Err(map_err_mountpoints(
                    err,
                    action_create_fs,
                    actions,
                ));
            }
            Ok(action) => actions.push(action),
        }
    }

    Ok(actions)
}

// mount_filesystem lets callers defined base dir
// for all filesystems to be mounted under.
pub fn mount_filesystems(
    mountpoints: &[ManifestMountpoint],
    base: &str,
) -> Result<Vec<ActionMountpoints>, AliError> {
    let mut actions = Vec::new();
    for mnt in mountpoints {
        let action_mount_fs = ActionMountpoints::MountFs {
            src: mnt.device.clone(),
            dst: mnt.dest.clone(),
            opts: mnt.mnt_opts.clone(),
        };

        match mount_filesystem(mnt, base) {
            Err(err) => {
                return Err(map_err_mountpoints(err, action_mount_fs, actions));
            }
            Ok(action) => {
                actions.push(action);
            }
        }
    }

    Ok(actions)
}
