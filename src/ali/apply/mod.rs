mod apply_stages;
mod archchroot;
mod bootstrap;
mod disks;
mod dm;
mod fs;
mod map_err;
mod routines;

use crate::ali::Manifest;
use crate::entity::report::Stages;
use crate::errors::AliError;

// Use manifest to install a new system
pub fn apply_manifest(
    manifest: &Manifest,
    install_location: &str,
) -> Result<Box<Stages>, AliError> {
    let mut progress = Box::default();

    // @TODO: skip stages from arg
    progress = apply_stages::mountpoints(manifest, install_location, progress)?;
    progress = apply_stages::bootstrap(manifest, install_location, progress)?;
    progress = apply_stages::routines(manifest, install_location, progress)?;
    progress = apply_stages::chroot_ali(manifest, install_location, progress)?;
    progress = apply_stages::chroot_user(manifest, install_location, progress)?;
    progress = apply_stages::postinstall_user(manifest, install_location, progress)?;

    Ok(progress)
}
