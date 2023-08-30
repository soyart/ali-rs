pub mod disks;
pub mod dm;
pub mod fs;

use self::fs::prepend_base;
use crate::defaults;
use crate::errors::AliError;
use crate::manifest::Manifest;
use crate::run::apply::Action;
use crate::utils::shell;

// Use manifest to install a new system
pub fn apply_manifest(
    manifest: &Manifest,
    location_env: Option<String>,
) -> Result<Vec<Action>, AliError> {
    let mut actions = Vec::new();

    // Format and partition disks
    if let Some(ref m_disks) = manifest.disks {
        match disks::apply_disks(m_disks) {
            Err(err) => {
                return Err(AliError::InstallError {
                    error: Box::new(err),
                    action_failed: Action::ApplyDisks,
                    actions_performed: actions,
                })
            }
            Ok(actions_disks) => {
                actions.extend(actions_disks);
            }
        };
    }

    // Format and create device mappers
    if let Some(ref m_dms) = manifest.device_mappers {
        match dm::apply_dms(m_dms) {
            Err(err) => {
                return Err(AliError::InstallError {
                    error: Box::new(err),
                    action_failed: Action::ApplyDms,
                    actions_performed: actions,
                })
            }
            Ok(actions_dms) => {
                actions.extend(actions_dms);
            }
        }
    }

    // Create rootfs
    match fs::apply_filesystem(&manifest.rootfs) {
        Err(err) => {
            return Err(AliError::InstallError {
                error: Box::new(err),
                action_failed: Action::CreateRootFs,
                actions_performed: actions,
            });
        }
        Ok(action_create_rootfs) => actions.push(action_create_rootfs),
    };

    // Create other filesystems
    if let Some(filesystems) = &manifest.filesystems {
        match fs::apply_filesystems(&filesystems) {
            Err(err) => {
                return Err(AliError::InstallError {
                    error: Box::new(err),
                    action_failed: Action::ApplyFilesystems,
                    actions_performed: actions,
                });
            }
            Ok(actions_create_filesystems) => {
                actions.extend(actions_create_filesystems);
            }
        }
    }

    let install_location = location_env.unwrap_or(defaults::DEFAULT_CHROOT_LOC.to_string());

    // mkdir rootfs chroot mount
    match shell::exec("mkdir", &["-p", &install_location]) {
        Err(err) => {
            return Err(AliError::InstallError {
                error: Box::new(err),
                action_failed: Action::MkdirRootFs,
                actions_performed: actions,
            });
        }
        Ok(()) => actions.push(Action::MkdirRootFs),
    }

    // Mount rootfs
    match fs::mount_filesystem(&manifest.rootfs, &install_location) {
        Err(err) => {
            return Err(AliError::InstallError {
                error: Box::new(err),
                action_failed: Action::MountRootFs,
                actions_performed: actions,
            });
        }
        Ok(action_mount_rootfs) => actions.push(action_mount_rootfs),
    }

    // Mount other filesystems to /{DEFAULT_CHROOT_LOC}
    if let Some(filesystems) = &manifest.filesystems {
        // Collect filesystems mountpoints and actions.
        // The mountpoints will be prepended with default base
        let mountpoints = filesystems
            .iter()
            .filter(|fs| fs.mnt.is_some())
            .map(|fs| fs.mnt.clone().unwrap())
            .map(|mountpoint| {
                (
                    prepend_base(&Some(&install_location), &mountpoint),
                    Action::Mkdir(mountpoint),
                )
            })
            .collect::<Vec<(String, Action)>>();

        // mkdir -p /{DEFAULT_CHROOT_LOC}/{mkdir_path}
        for mkdir_path in mountpoints {
            if let Err(err) = shell::exec("mkdir", &[&mkdir_path.0]) {
                return Err(AliError::InstallError {
                    error: Box::new(err),
                    action_failed: mkdir_path.1,
                    actions_performed: actions,
                });
            }

            actions.push(mkdir_path.1);
        }

        // Mount other filesystems under /{DEFAULT_CHROOT_LOC}
        match fs::mount_filesystems(&filesystems, &install_location) {
            Err(err) => {
                return Err(AliError::InstallError {
                    error: Box::new(err),
                    action_failed: Action::MountFilesystems,
                    actions_performed: actions,
                });
            }
            Ok(actions_mount_filesystems) => actions.extend(actions_mount_filesystems),
        }
    }

    // TODO: pacstrap, install, and post-install

    Ok(actions)
}
