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
use crate::entity::stage::{self, Stage, StageActions};
use crate::errors::AliError;

/// Use `manifest` to install a new system to `install_location`
/// skipping any stages in `skip`, and maps `AliError::ApplyError`
/// to `AliError::InstallError` with StageActions embedded.
#[rustfmt::skip]
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

        let result = match stage {
            Stage::Mountpoints => {
                apply_stages::mountpoints(manifest, install_location, &mut progress)
            }

            Stage::Bootstrap => {
                apply_stages::bootstrap(manifest, install_location, &mut progress)
            },

            Stage::Routines => {
                apply_stages::routines(manifest, install_location, &mut progress)
            },

            Stage::ChrootAli => {
                apply_stages::chroot_ali(manifest, install_location, &mut progress)
            },

            Stage::ChrootUser => {
                apply_stages::chroot_user(manifest, install_location, &mut progress)
            }

            Stage::PostInstallUser => {
                apply_stages::postinstall_user(manifest, install_location, &mut progress)
            }
        };

        if let Err(err) = result {
            return Err(AliError::InstallError {
                error: Box::new(err),
                stages_performed: progress,
            });
        }
    }

    Ok(progress)
}
