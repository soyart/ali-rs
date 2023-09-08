mod apply_stages;
mod archchroot;
mod bootstrap;
mod disks;
mod dm;
mod fs;
mod map_err;
mod routines;

use crate::ali::Manifest;
use crate::entity::stage::StageActions;
use crate::errors::AliError;

/// Use `manifest` to install a new system to `install_location`
/// and maps `AliError::ApplyError` to `AliError::InstallError`
pub fn apply_manifest(
    manifest: &Manifest,
    install_location: &str,
) -> Result<Box<StageActions>, AliError> {
    // @TODO: skip stages from arg
    let mut progress = Box::default();

    if let Err(err) = apply_stages::mountpoints(manifest, install_location, &mut progress) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            stages_performed: progress,
        });
    }

    if let Err(err) = apply_stages::bootstrap(manifest, install_location, &mut progress) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            stages_performed: progress,
        });
    }

    if let Err(err) = apply_stages::routines(manifest, install_location, &mut progress) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            stages_performed: progress,
        });
    }

    if let Err(err) = apply_stages::chroot_ali(manifest, install_location, &mut progress) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            stages_performed: progress,
        });
    }

    if let Err(err) = apply_stages::chroot_user(manifest, install_location, &mut progress) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            stages_performed: progress,
        });
    }

    if let Err(err) = apply_stages::postinstall_user(manifest, install_location, &mut progress) {
        return Err(AliError::InstallError {
            error: Box::new(err),
            stages_performed: progress,
        });
    }

    Ok(progress)
}
