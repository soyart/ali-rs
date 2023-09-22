mod apply_stages;
mod archchroot;
mod bootstrap;
mod disks;
mod dm;
mod fs;
mod map_err;
mod routines;

use std::collections::HashSet;

use crate::ali::Manifest;
use crate::entity::stage::{Stage, StageActions};
use crate::errors::AliError;

/// Use `manifest` to install a new system to `install_location`
/// skipping any stages in `skip`, and maps `AliError::ApplyError`
/// to `AliError::InstallError` with StageActions embedded.
pub fn apply_manifest(
    manifest: &Manifest,
    install_location: &str,
    skip: HashSet<Stage>,
) -> Result<Box<StageActions>, AliError> {
    let mut progress = Box::default();

    if !skip.contains(&Stage::Mountpoints) {
        if let Err(err) = apply_stages::mountpoints(manifest, install_location, &mut progress) {
            return Err(AliError::InstallError {
                error: Box::new(err),
                stages_performed: progress,
            });
        }
    }

    if !skip.contains(&Stage::Bootstrap) {
        if let Err(err) = apply_stages::bootstrap(manifest, install_location, &mut progress) {
            return Err(AliError::InstallError {
                error: Box::new(err),
                stages_performed: progress,
            });
        }
    }

    if !skip.contains(&Stage::Routines) {
        if let Err(err) = apply_stages::routines(manifest, install_location, &mut progress) {
            return Err(AliError::InstallError {
                error: Box::new(err),
                stages_performed: progress,
            });
        }
    }

    if !skip.contains(&Stage::ChrootAli) {
        if let Err(err) = apply_stages::chroot_ali(manifest, install_location, &mut progress) {
            return Err(AliError::InstallError {
                error: Box::new(err),
                stages_performed: progress,
            });
        }
    }

    if !skip.contains(&Stage::ChrootUser) {
        if let Err(err) = apply_stages::chroot_user(manifest, install_location, &mut progress) {
            return Err(AliError::InstallError {
                error: Box::new(err),
                stages_performed: progress,
            });
        }
    }

    if !skip.contains(&Stage::PostInstallUser) {
        if let Err(err) = apply_stages::postinstall_user(manifest, install_location, &mut progress)
        {
            return Err(AliError::InstallError {
                error: Box::new(err),
                stages_performed: progress,
            });
        }
    }

    Ok(progress)
}
