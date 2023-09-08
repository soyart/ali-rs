use std::collections::HashSet;

use crate::ali::Manifest;
use crate::entity::stage::StageActions;
use crate::errors::AliError;
use crate::hooks;
use crate::utils::shell;

/// Prepare mountpoints for the new system on live system
pub fn mountpoints(
    manifest: &Manifest,
    mnt_location: &str,
    stages: &mut StageActions,
) -> Result<(), AliError> {
    use super::{disks, dm, fs};
    use crate::entity::action::ActionMountpoints;

    // Format and partition disks
    if let Some(ref m_disks) = manifest.disks {
        let actions_disks = disks::apply_disks(m_disks)?;
        stages.mountpoints.extend(actions_disks);
    }

    // Format and create device mappers
    if let Some(ref m_dms) = manifest.device_mappers {
        let actions_dms = dm::apply_dms(m_dms)?;
        stages.mountpoints.extend(actions_dms);
    }

    // Create rootfs
    let action_create_rootfs = fs::apply_filesystem(&manifest.rootfs)?;
    stages.mountpoints.push(action_create_rootfs);

    // Create other filesystems
    if let Some(filesystems) = &manifest.filesystems {
        let actions_create_filesystems = fs::apply_filesystems(filesystems)?;
        stages.mountpoints.extend(actions_create_filesystems);
    }

    // mkdir rootfs chroot mount
    shell::exec("mkdir", &["-p", mnt_location])?;
    stages.mountpoints.push(ActionMountpoints::MkdirRootFs);

    // Mount rootfs
    let action_mnt_rootfs = fs::mount_filesystem(&manifest.rootfs, mnt_location)?;
    stages.mountpoints.push(action_mnt_rootfs);

    // Mount other filesystems to /{DEFAULT_CHROOT_LOC}
    if let Some(filesystems) = &manifest.filesystems {
        // Collect filesystems mountpoints and actions.
        // The mountpoints will be prepended with default base
        let mountpoints: Vec<(String, ActionMountpoints)> = filesystems
            .iter()
            .filter_map(|fs| {
                fs.mnt.clone().map(|mountpoint| {
                    (
                        fs::prepend_base(&Some(mnt_location), &mountpoint),
                        ActionMountpoints::MkdirFs(mountpoint),
                    )
                })
            })
            .collect();

        // mkdir -p /{DEFAULT_CHROOT_LOC}/{mkdir_path}
        for (dir, action_mkdir) in mountpoints {
            shell::exec("mkdir", &[&dir])?;
            stages.mountpoints.push(action_mkdir);
        }

        // Mount other filesystems under /{DEFAULT_CHROOT_LOC}
        let actions_mnt = fs::mount_filesystems(filesystems, mnt_location)?;
        stages.mountpoints.extend(actions_mnt);
    }

    Ok(())
}

/// Install Arch Linux `base` and other packages defined in manifest.
pub fn bootstrap(
    manifest: &Manifest,
    install_location: &str,
    stages: &mut StageActions,
) -> Result<(), AliError> {
    use super::bootstrap;
    use crate::entity::action::ActionBootstrap;

    // Collect packages, with base as bare-minimum
    let mut packages = HashSet::from(["base".to_string()]);
    if let Some(pacstraps) = manifest.pacstraps.clone() {
        packages.extend(pacstraps);
    }

    // Install packages (manifest.pacstraps) to install_location
    let action_pacstrap = ActionBootstrap::InstallPackages { packages };
    bootstrap::pacstrap_to_location(&manifest.pacstraps, install_location)?;
    stages.bootstrap.push(action_pacstrap);

    Ok(())
}

pub fn routines(
    manifest: &Manifest,
    install_location: &str,
    stages: &mut StageActions,
) -> Result<(), AliError> {
    use super::routines;

    // Apply ALI routines installation outside of arch-chroot
    let actions_routine = routines::ali_routines(manifest, install_location)?;
    stages.routines.extend(actions_routine);

    Ok(())
}

pub fn chroot_ali(
    manifest: &Manifest,
    install_location: &str,
    stages: &mut StageActions,
) -> Result<(), AliError> {
    use super::archchroot;

    // Apply ALI routine installation in arch-chroot
    let actions_archchroot = archchroot::chroot_ali(manifest, install_location)?;
    stages.chroot_ali.extend(actions_archchroot);

    Ok(())
}

pub fn chroot_user(
    manifest: &Manifest,
    install_location: &str,
    stages: &mut StageActions,
) -> Result<(), AliError> {
    use super::archchroot;

    if let Some(ref cmds) = manifest.chroot {
        let actions_user_cmds = archchroot::chroot_user(cmds.iter(), install_location)?;
        stages.chroot_user.extend(actions_user_cmds);
    }

    Ok(())
}

pub fn postinstall_user(
    manifest: &Manifest,
    install_location: &str,
    stages: &mut StageActions,
) -> Result<(), AliError> {
    use crate::entity::action::ActionPostInstallUser;

    // Apply manifest.postinstall with sh -c 'cmd'
    if let Some(ref cmds) = manifest.postinstall {
        for cmd in cmds {
            if cmd.starts_with("#") {
                let action_hook = hooks::apply_hook(&cmd, false, install_location)?;
                stages
                    .postinstall_user
                    .push(ActionPostInstallUser::Hook(action_hook));

                continue;
            }

            shell::sh_c(cmd)?;

            let action_postinstall_cmd = ActionPostInstallUser::UserPostInstallCmd(cmd.clone());
            stages.postinstall_user.push(action_postinstall_cmd);
        }
    }

    Ok(())
}
