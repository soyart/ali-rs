mod archchroot;
mod disks;
mod dm;
mod fs;
mod routine;

use std::collections::HashSet;

use crate::ali::Manifest;
use crate::errors::AliError;
use crate::run::apply::{ActionBootstrap, ActionMountpoints, ActionPostInstallUser, Stages};
use crate::utils::shell;

// Use manifest to install a new system
pub fn apply_manifest(
    manifest: &Manifest,
    install_location: &str,
) -> Result<Box<Stages>, AliError> {
    let mut stages = Box::<Stages>::default();

    // Format and partition disks
    if let Some(ref m_disks) = manifest.disks {
        match disks::apply_disks(m_disks) {
            Err(err) => {
                return Err(AliError::InstallError {
                    error: Box::new(err),
                    stages_performed: stages,
                });
            }
            Ok(actions_disks) => {
                stages.mountpoints.extend(actions_disks);
            }
        };
    }

    // Format and create device mappers
    if let Some(ref m_dms) = manifest.device_mappers {
        match dm::apply_dms(m_dms) {
            Err(err) => {
                return Err(AliError::InstallError {
                    error: Box::new(err),
                    stages_performed: stages,
                })
            }
            Ok(actions_dms) => {
                stages.mountpoints.extend(actions_dms);
            }
        }
    }

    // Create rootfs
    match fs::apply_filesystem(&manifest.rootfs) {
        Err(err) => {
            return Err(AliError::InstallError {
                error: Box::new(err),
                stages_performed: stages,
            });
        }
        Ok(action_create_rootfs) => {
            stages.mountpoints.push(action_create_rootfs);
        }
    };

    // Create other filesystems
    if let Some(filesystems) = &manifest.filesystems {
        match fs::apply_filesystems(filesystems) {
            Err(err) => {
                return Err(AliError::InstallError {
                    error: Box::new(err),
                    stages_performed: stages,
                });
            }
            Ok(actions_create_filesystems) => {
                stages.mountpoints.extend(actions_create_filesystems);
            }
        }
    }

    // mkdir rootfs chroot mount
    match shell::exec("mkdir", &["-p", install_location]) {
        Err(err) => {
            return Err(AliError::InstallError {
                error: Box::new(err),
                stages_performed: stages,
            });
        }
        Ok(()) => {
            stages.mountpoints.push(ActionMountpoints::MkdirRootFs);
        }
    }

    // Mount rootfs
    match fs::mount_filesystem(&manifest.rootfs, install_location) {
        Err(err) => {
            return Err(AliError::InstallError {
                error: Box::new(err),
                stages_performed: stages,
            });
        }
        Ok(action_mount_rootfs) => {
            stages.mountpoints.push(action_mount_rootfs);
        }
    }

    // Mount other filesystems to /{DEFAULT_CHROOT_LOC}
    if let Some(filesystems) = &manifest.filesystems {
        // Collect filesystems mountpoints and actions.
        // The mountpoints will be prepended with default base
        let mountpoints: Vec<(String, ActionMountpoints)> = filesystems
            .iter()
            .filter_map(|fs| {
                fs.mnt.clone().map(|mountpoint| {
                    (
                        fs::prepend_base(&Some(install_location), &mountpoint),
                        ActionMountpoints::MkdirFs(mountpoint),
                    )
                })
            })
            .collect();

        // mkdir -p /{DEFAULT_CHROOT_LOC}/{mkdir_path}
        for (dir, action_mkdir) in mountpoints {
            if let Err(err) = shell::exec("mkdir", &[&dir]) {
                return Err(AliError::InstallError {
                    error: Box::new(err),
                    stages_performed: stages,
                });
            }

            stages.mountpoints.push(action_mkdir);
        }

        // Mount other filesystems under /{DEFAULT_CHROOT_LOC}
        match fs::mount_filesystems(filesystems, install_location) {
            Err(err) => {
                return Err(AliError::InstallError {
                    error: Box::new(err),
                    stages_performed: stages,
                });
            }
            Ok(actions_mount_filesystems) => {
                stages.mountpoints.extend(actions_mount_filesystems);
            }
        }
    }

    // Collect packages, with base as bare-minimum
    let mut packages = HashSet::from(["base".to_string()]);
    if let Some(pacstraps) = manifest.pacstraps.clone() {
        packages.extend(pacstraps);
    }

    // Install packages (manifest.pacstraps) to install_location
    let action_pacstrap = ActionBootstrap::InstallPackages { packages };
    if let Err(err) = pacstrap_to_location(&manifest.pacstraps, install_location) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            stages_performed: stages,
        });
    }
    stages.bootstrap.push(action_pacstrap);

    // Apply ALI routine installation outside of arch-chroot
    match routine::apply_routine(manifest, install_location) {
        Err(err) => {
            return Err(AliError::InstallError {
                error: Box::new(err),
                stages_performed: stages,
            });
        }
        Ok(actions_routine) => {
            stages.routines.extend(actions_routine);
        }
    }

    // Apply ALI routine installation in arch-chroot
    match archchroot::chroot_ali(manifest, install_location) {
        Err(err) => {
            return Err(AliError::InstallError {
                error: Box::new(err),
                stages_performed: stages,
            });
        }
        Ok(actions_archchroot) => {
            stages.chroot_ali.extend(actions_archchroot);
        }
    }

    // Apply manifest.chroot
    if let Some(ref cmds) = manifest.chroot {
        match archchroot::chroot_user(cmds.iter(), install_location) {
            Err(err) => {
                return Err(AliError::InstallError {
                    error: Box::new(err),
                    stages_performed: stages,
                });
            }
            Ok(actions_user_cmds) => {
                stages.chroot_user.extend(actions_user_cmds);
            }
        }
    }

    // Apply manifest.postinstall with sh -c 'cmd'
    if let Some(ref cmds) = manifest.postinstall {
        for cmd in cmds {
            let action_postinstall_cmd = ActionPostInstallUser::UserPostInstallCmd(cmd.clone());
            if let Err(err) = shell::sh_c(cmd) {
                return Err(AliError::InstallError {
                    error: Box::new(err),
                    stages_performed: stages,
                });
            }

            stages.postinstall_user.push(action_postinstall_cmd);
        }
    }

    Ok(stages)
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

    shell::sh_c(&cmd_pacstrap)
}
