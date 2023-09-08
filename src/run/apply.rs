use std::collections::HashSet;
use std::env;

use crate::ali::apply;
use crate::ali::validation;
use crate::ali::{Dm, Manifest};
use crate::cli;
use crate::constants::{self, defaults};
use crate::entity::report::Report;
use crate::errors::AliError;

pub(super) fn run(manifest_file: &str, args: cli::ArgsApply) -> Result<Report, AliError> {
    let start = std::time::Instant::now();

    let manifest_yaml = std::fs::read_to_string(manifest_file)
        .map_err(|err| AliError::NoSuchFile(err, manifest_file.to_string()))?;

    // manifest is mutable because we might have to
    // help add packages such as lvm2 and btrfs-progs
    let mut manifest = Manifest::from_yaml(&manifest_yaml)?;

    if !args.no_validate {
        validation::validate(&manifest, args.overwrite)?;
    }

    // Update manifest in some cases
    update_manifest(&mut manifest);

    // Apply manifest to location
    let location = install_location();
    let stages = apply::apply_manifest(
        &manifest,
        &location,
        HashSet::from_iter(args.skip_stages.iter()),
    )?;

    Ok(Report {
        location,
        summary: stages,
        duration: start.elapsed(),
    })
}

fn install_location() -> String {
    env::var(constants::ENV_ALI_LOC).unwrap_or(defaults::INSTALL_LOCATION.to_string())
}

// Update manifest to suit the manifest
fn update_manifest(manifest: &mut Manifest) {
    let (lvm2, btrfs, btrfs_progs) = (
        "lvm2".to_string(),
        "btrfs".to_string(),
        "btrfs-progs".to_string(),
    );

    let (mut has_lvm, mut has_btrfs) = (false, false);

    // See if root is on Btrfs
    if manifest.rootfs.fs_type.as_str() == btrfs {
        has_btrfs = true;
    }

    // See if other FS is Btrfs
    if let (false, Some(filesystems)) = (has_btrfs, &manifest.filesystems) {
        for fs in filesystems {
            if fs.fs_type.as_str() == btrfs {
                has_btrfs = true;

                break;
            }
        }
    }

    // Update manifest.pacstraps if any of the filesystems is Btrfs
    match (has_btrfs, manifest.pacstraps.as_mut()) {
        (true, Some(ref mut pacstraps)) => {
            pacstraps.insert(btrfs_progs.clone());
        }
        (true, None) => {
            manifest.pacstraps = Some(HashSet::from([btrfs_progs.clone()]));
        }
        _ => {}
    }

    // Find a manifest LVM device
    if let Some(ref dms) = manifest.device_mappers {
        for dm in dms {
            match dm {
                Dm::Lvm(_) => {
                    has_lvm = true;

                    break;
                }
                _ => continue,
            }
        }
    }

    // Update manifest.pacstraps if we have LVMs in manifest
    match (has_lvm, manifest.pacstraps.as_mut()) {
        (true, Some(ref mut pacstraps)) => {
            pacstraps.insert(lvm2.clone());
        }

        (true, None) => {
            manifest.pacstraps = Some(HashSet::from([lvm2.clone()]));
        }
        _ => {}
    }
}
