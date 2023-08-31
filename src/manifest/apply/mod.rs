mod archchroot;
mod disks;
mod dm;
mod fs;
mod routine;

use std::collections::HashSet;

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
            Ok(actions_disks) => actions.extend(actions_disks),
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
            Ok(actions_dms) => actions.extend(actions_dms),
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

    let install_location = location_env.unwrap_or(defaults::DEFAULT_INSTALL_LOCATION.to_string());

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
                    fs::prepend_base(&Some(&install_location), &mountpoint),
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

    // Collect packages, with base as bare-minimum
    let mut packages = HashSet::from(["base".to_string()]);
    if let Some(pacstraps) = manifest.pacstraps.clone() {
        packages.extend(pacstraps);
    }

    // Install packages (manifest.pacstraps) to install_location
    let action_pacstrap = Action::InstallPackages { packages };
    if let Err(err) = pacstrap_to_location(&manifest.pacstraps, &install_location) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            action_failed: action_pacstrap,
            actions_performed: actions,
        });
    }
    actions.push(action_pacstrap);

    let action_ali_archchroot = Action::AliArchChroot;
    match archchroot::ali(&manifest, &install_location) {
        Err(err) => {
            return Err(AliError::InstallError {
                error: Box::new(err),
                action_failed: action_ali_archchroot,
                actions_performed: actions,
            });
        }
        Ok(actions_archchroot) => {
            actions.extend(actions_archchroot);
            actions.push(action_ali_archchroot);
        }
    }

    let action_ali_routine = Action::AliRoutine;
    match routine::apply_routine(manifest, &install_location) {
        Err(err) => {
            return Err(AliError::InstallError {
                error: Box::new(err),
                action_failed: action_ali_routine,
                actions_performed: actions,
            });
        }
        Ok(actions_routine) => {
            actions.extend(actions_routine);
            actions.push(action_ali_routine);
        }
    }

    Ok(actions)
}

fn pacstrap_to_location(
    pacstraps: &Option<HashSet<String>>,
    location: &str,
) -> Result<(), AliError> {
    // Collect packages, with base as bare-minimum
    let mut packages = HashSet::from(["base".to_string()]);
    if let Some(pacstraps) = pacstraps.clone() {
        packages.extend(pacstraps);
    }

    let cmd_pacstrap = {
        let mut cmd_parts = vec![
            "pacstrap".to_string(),
            "-K".to_string(),
            location.to_string(),
        ];
        cmd_parts.extend(packages);
        cmd_parts.join(" ")
    };

    shell::exec("sh", &["-c", &format!("'{cmd_pacstrap}'")])
}
