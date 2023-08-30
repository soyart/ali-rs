pub mod disks;
pub mod dm;
pub mod fs;

use crate::errors::AliError;
use crate::manifest::Manifest;
use crate::run::apply::Action;

// Use manifest to install a new system
pub fn apply_manifest(manifest: &Manifest) -> Result<Vec<Action>, AliError> {
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

    // Mount rootfs
    match fs::mount_filesystem(&manifest.rootfs) {
        Err(err) => {
            return Err(AliError::InstallError {
                error: Box::new(err),
                action_failed: Action::MountRootFs,
                actions_performed: actions,
            });
        }
        Ok(action_mount_rootfs) => actions.push(action_mount_rootfs),
    }

    // Mount other filesystems
    if let Some(filesystems) = &manifest.filesystems {
        match fs::mount_filesystems(&filesystems) {
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
