use crate::ali::ManifestFs;
use crate::entity::action::ActionMountpoints;
use crate::errors::AliError;
use crate::linux;

use super::map_err::map_err_mountpoints;

pub fn apply_filesystem(
    filesystem: &ManifestFs,
) -> Result<ActionMountpoints, AliError> {
    linux::mkfs::create_fs(filesystem)?;

    Ok(ActionMountpoints::CreateFs {
        device: filesystem.device.clone(),
        fs_type: filesystem.fs_type.clone(),
        fs_opts: filesystem.fs_opts.clone(),
        mountpoint: filesystem.mnt.clone(),
    })
}

// mount_filesystem lets callers override mountpoint with `mountpoint`.
pub fn mount_filesystem(
    filesystem: &ManifestFs,
    mountpoint: &str,
) -> Result<ActionMountpoints, AliError> {
    let mountpoint = mountpoint.to_string();
    let fs = ManifestFs {
        mnt: Some(mountpoint.clone()),
        ..filesystem.clone()
    };

    linux::mount::mount_fs(&fs)?;

    Ok(ActionMountpoints::MountFs {
        src: fs.device,
        dst: mountpoint,
        opts: fs.mnt_opts,
    })
}

pub fn apply_filesystems(
    filesystems: &[ManifestFs],
) -> Result<Vec<ActionMountpoints>, AliError> {
    let mut actions = Vec::new();

    for fs in filesystems {
        let action_create_fs = ActionMountpoints::CreateFs {
            device: fs.device.clone(),
            fs_type: fs.fs_type.clone(),
            fs_opts: fs.fs_opts.clone(),
            mountpoint: fs.mnt.clone(),
        };

        match apply_filesystem(fs) {
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
    filesystems: &[ManifestFs],
    base: &str,
) -> Result<Vec<ActionMountpoints>, AliError> {
    let mut actions = Vec::new();
    for fs in filesystems {
        if fs.mnt.is_none() {
            continue;
        }

        let mountpoint = prepend_base(&Some(base), &fs.mnt.clone().unwrap());
        let action_mount_fs = ActionMountpoints::MountFs {
            src: fs.device.clone(),
            dst: mountpoint.to_string(),
            opts: fs.mnt_opts.clone(),
        };

        match mount_filesystem(fs, &mountpoint) {
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

pub fn prepend_base(base: &Option<&str>, mountpoint: &str) -> String {
    if base.is_none() {
        return mountpoint.to_string();
    }

    // e.g. base /data on manifest /foo => /data/foo
    format!("{}{mountpoint}", (*base).unwrap())
}
