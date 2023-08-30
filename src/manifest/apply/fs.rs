use crate::errors::AliError;
use crate::linux;
use crate::manifest::ManifestFs;
use crate::run::apply::Action;

pub fn apply_filesystem(filesystem: &ManifestFs) -> Result<Action, AliError> {
    linux::mkfs::create_fs(filesystem)?;

    Ok(Action::CreateFs {
        device: filesystem.device.clone(),
        fs_type: filesystem.fs_type.clone(),
        fs_opts: filesystem.fs_opts.clone(),
        mountpoint: filesystem.mnt.clone(),
    })
}

pub fn mount_filesystem(filesystem: &ManifestFs) -> Result<Action, AliError> {
    linux::mount::mount_fs(filesystem)?;

    Ok(Action::MountFs {
        src: filesystem.device.clone(),
        dst: filesystem.mnt.clone().unwrap(),
        opts: filesystem.mnt_opts.clone(),
    })
}

pub fn apply_filesystems(filesystems: &[ManifestFs]) -> Result<Vec<Action>, AliError> {
    let mut actions = Vec::new();
    for fs in filesystems {
        let action_create_fs = Action::CreateFs {
            device: fs.device.clone(),
            fs_type: fs.fs_type.clone(),
            fs_opts: fs.fs_opts.clone(),
            mountpoint: fs.mnt.clone(),
        };

        match apply_filesystem(fs) {
            Err(err) => {
                return Err(AliError::InstallError {
                    error: Box::new(err),
                    action_failed: action_create_fs,
                    actions_performed: actions,
                });
            }
            Ok(action) => actions.push(action),
        }
    }

    Ok(actions)
}

pub fn mount_filesystems(filesystems: &[ManifestFs]) -> Result<Vec<Action>, AliError> {
    let mut actions = Vec::new();
    for fs in filesystems {
        if fs.mnt.is_none() {
            continue;
        }

        let action_mount_fs = Action::MountFs {
            src: fs.device.clone(),
            dst: fs
                .mnt
                .clone()
                .expect("mount fs that's not supposed to be mounted"),
            opts: fs.mnt_opts.clone(),
        };

        match mount_filesystem(fs) {
            Err(err) => {
                return Err(AliError::InstallError {
                    error: Box::new(err),
                    action_failed: action_mount_fs,
                    actions_performed: actions,
                });
            }
            Ok(action) => {
                actions.push(action);
            }
        }
    }

    Ok(actions)
}
