use std::collections::HashSet;

use super::{
    archchroot,
    bootstrap,
    disks,
    dm,
    fs,
    routines,
};
use crate::ali::{
    Manifest,
    ManifestFs,
    ManifestMountpoint,
};
use crate::entity::action::{
    ActionBootstrap,
    ActionMountpoints,
    ActionPostInstallUser,
};
use crate::entity::stage::StageActions;
use crate::errors::AliError;
use crate::hooks;
use crate::utils::shell;

/// Prepare mountpoints for the new system on live system
pub fn mountpoints(
    manifest: &Manifest,
    root_location: &str,
    stages: &mut StageActions,
) -> Result<(), AliError> {
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
    let rootfs: ManifestFs = manifest.rootfs.clone().into();
    let action_create_rootfs = fs::apply_filesystem(&rootfs)?;
    stages.mountpoints.push(action_create_rootfs);

    // Create other filesystems
    if let Some(filesystems) = &manifest.filesystems {
        let actions_create_filesystems = fs::apply_filesystems(filesystems)?;
        stages.mountpoints.extend(actions_create_filesystems);
    }

    // mkdir rootfs chroot mount
    shell::exec("mkdir", &["-p", root_location])?;
    stages.mountpoints.push(ActionMountpoints::MkdirRootFs);

    // Mount rootfs
    let mnt_root: ManifestMountpoint = manifest.rootfs.clone().into();
    let action_mnt_rootfs = fs::mount_filesystem(&mnt_root, root_location)?;
    stages.mountpoints.push(action_mnt_rootfs);

    // Mount other filesystems to /{DEFAULT_CHROOT_LOC}
    if let Some(mounts) = &manifest.mountpoints {
        // Collect filesystems mountpoints and actions.
        // The mountpoints will be prepended with default base
        let mountpoints: Vec<(String, ActionMountpoints)> = mounts
            .iter()
            .map(|m| {
                (m.dest.clone(), ActionMountpoints::MkdirFs(m.dest.clone()))
            })
            .collect();

        // mkdir -p /{DEFAULT_CHROOT_LOC}/{mkdir_path}
        for (dir, action_mkdir) in mountpoints {
            shell::exec("mkdir", &[&dir])?;
            stages.mountpoints.push(action_mkdir);
        }

        // Mount other filesystems under /{DEFAULT_CHROOT_LOC}
        let actions_mnt = fs::mount_filesystems(mounts, root_location)?;
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
    // Apply ALI routine installation in arch-chroot
    let actions_archchroot =
        archchroot::chroot_ali(manifest, install_location)?;

    stages.chroot_ali.extend(actions_archchroot);

    Ok(())
}

pub fn chroot_user(
    manifest: &Manifest,
    install_location: &str,
    stages: &mut StageActions,
) -> Result<(), AliError> {
    if manifest.chroot.is_none() {
        return Ok(());
    }

    let commands = manifest.chroot.as_ref().unwrap();
    let actions_user_cmds =
        archchroot::chroot_user(commands.iter(), install_location)?;

    stages.chroot_user.extend(actions_user_cmds);

    Ok(())
}

pub fn postinstall_user(
    manifest: &Manifest,
    install_location: &str,
    stages: &mut StageActions,
) -> Result<(), AliError> {
    // Apply manifest.postinstall with sh -c 'cmd'
    if manifest.postinstall.is_none() {
        return Ok(());
    }

    let postinstall = manifest.postinstall.as_ref().unwrap();
    for cmd in postinstall {
        if hooks::is_hook(cmd) {
            let action_hook = hooks::apply_hook(
                cmd,
                hooks::Caller::ManifestPostInstall,
                install_location,
            )?;

            stages
                .postinstall_user
                .push(ActionPostInstallUser::Hook(action_hook));

            continue;
        }

        shell::sh_c(cmd)?;

        let action_postinstall_cmd =
            ActionPostInstallUser::UserPostInstallCmd(cmd.clone());

        stages.postinstall_user.push(action_postinstall_cmd);
    }

    Ok(())
}
