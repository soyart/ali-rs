mod archchroot;
mod bootstrap;
mod disks;
mod dm;
mod fs;
mod map_err;
mod routines;
mod stages;

use std::collections::HashSet;

use crate::ali::Manifest;
use crate::types::stage::{
    self,
    Stage,
    StageActions,
};
use crate::errors::AliError;

type ApplyFn = fn(&Manifest, &str, &mut StageActions) -> Result<(), AliError>;

/// Use `manifest` to install a new system to `install_location`
/// skipping any stages in `skip`, and maps `AliError::ApplyError`
/// to `AliError::InstallError` with StageActions embedded.
pub fn apply_manifest(
    manifest: &Manifest,
    install_location: &str,
    skip: HashSet<Stage>,
) -> Result<Box<StageActions>, AliError> {
    let mut progress = Box::default();

    for stage in stage::STAGES {
        if skip.contains(&stage) {
            continue;
        }

        let f: ApplyFn = match stage {
            Stage::Mountpoints => stages::mountpoints,
            Stage::Bootstrap => stages::bootstrap,
            Stage::Routines => stages::routines,
            Stage::ChrootAli => stages::chroot_ali,
            Stage::ChrootUser => stages::chroot_user,
            Stage::PostInstallUser => stages::postinstall_user,
        };

        if let Err(err) = f(manifest, install_location, &mut progress) {
            return Err(AliError::InstallError {
                error: Box::new(err),
                stages_performed: progress,
            });
        }
    }

    Ok(progress)
}
